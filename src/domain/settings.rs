//! The settings, as one value with its own defaults, validation and text
//! codec (DOMAIN.md section 19).
//!
//! Storage keeps the rows and the application draws the page, but what a
//! setting may hold and what it means is a rule, so it lives here. The
//! domain still reads no clock and no environment: the hour the day
//! begins comes off this value, and the resolved date order arrives in a
//! `Context`.

use jiff::civil::Date;
use jiff::{Span, Zoned};

use super::Rejected;
use super::model::{Change, Model, diff};
use super::rule::Weekday;

/// The keys of the `settings` table. A value the codec cannot read is
/// the default and an unknown key is ignored, so an older binary opens a
/// newer database's settings without losing them.
const DAY_STARTS_AT: &str = "day_starts_at";
const WEEK_STARTS_ON: &str = "week_starts_on";
const WORK_DAYS: &str = "work_days";
const REVIEW_OPENS_ITSELF: &str = "review_opens_itself";
const DUE_AHEAD_DAYS: &str = "due_ahead_days";
const BACKFILL_DAYS: &str = "backfill_days";
const PILE_HORIZON_DAYS: &str = "pile_horizon_days";
const FLOATING_WINDOW: &str = "floating_window";
const WINDOW_SIZE: &str = "window_size";
const MOUSE: &str = "mouse";
const MESSAGE_SECONDS: &str = "message_seconds";
const DATE_STYLE: &str = "date_style";
const CONFIRM_DELETE: &str = "confirm_delete";

/// Where a week begins, which is where the history's weeks fall and
/// which column a calendar opens with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeekStart {
    #[default]
    Monday,
    Sunday,
}

/// Which days "every work day" and the move card's "next work day" mean.
/// A set, so no order and no repeats; at least one day, which is what
/// `change_settings` refuses an empty one for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkDays(u8);

impl WorkDays {
    /// Monday to Friday.
    pub const DEFAULT: WorkDays = WorkDays(0b0001_1111);

    /// The set of exactly these days.
    pub fn of(days: impl IntoIterator<Item = Weekday>) -> WorkDays {
        let mut set = WorkDays(0);
        for day in days {
            set.0 |= bit(day);
        }
        set
    }

    pub fn contains(self, day: Weekday) -> bool {
        self.0 & bit(day) != 0
    }

    pub fn toggle(&mut self, day: Weekday) {
        self.0 ^= bit(day);
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The days in it, Monday first, whatever order they were added in.
    pub fn iter(self) -> impl Iterator<Item = Weekday> {
        Weekday::ALL
            .into_iter()
            .filter(move |day| self.contains(*day))
    }
}

impl Default for WorkDays {
    fn default() -> Self {
        WorkDays::DEFAULT
    }
}

fn bit(day: Weekday) -> u8 {
    1 << position(day)
}

fn position(day: Weekday) -> u8 {
    match day {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    }
}

/// How a date is written, as the setting holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateStyle {
    /// Whatever the locale writes, which the application resolves.
    #[default]
    Locale,
    DayFirst,
    MonthFirst,
}

/// How a date is written, once the locale has been consulted. Every date
/// in the program is formatted from one of these (`day_label`,
/// `short_label`, `stamp_label`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateOrder {
    #[default]
    DayFirst,
    MonthFirst,
}

/// The floating window's size in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSize {
    pub width: u16,
    pub height: u16,
}

impl WindowSize {
    /// The narrowest and widest a window may be asked to be. Below the
    /// low end nothing of the program fits; above the high end is a
    /// typing slip rather than a screen.
    pub const LEAST: i64 = 200;
    pub const MOST: i64 = 10_000;

    /// The five sizes the page steps through, smallest first, each a
    /// whole number of cells in foot with Omarchy's default font
    /// (STACK.md section 7). A size is a pair of pixel counts and a
    /// person picks a window by how much of the program fits in it, so
    /// the row offers the grids and types the pixels only when it has
    /// to.
    pub const PRESETS: [WindowSize; 5] = [
        WindowSize {
            width: 730,
            height: 550,
        },
        WindowSize {
            width: 870,
            height: 650,
        },
        WindowSize {
            width: 1010,
            height: 755,
        },
        WindowSize {
            width: 1150,
            height: 860,
        },
        WindowSize {
            width: 1290,
            height: 960,
        },
    ];

    /// A size, held to the range whatever it is given.
    pub fn new(width: i64, height: i64) -> WindowSize {
        WindowSize {
            width: clamp(width, WindowSize::LEAST, WindowSize::MOST) as u16,
            height: clamp(height, WindowSize::LEAST, WindowSize::MOST) as u16,
        }
    }

    /// The grid this size gives, for the sizes that were chosen to give
    /// a whole one. Any other size is a number of pixels and nothing
    /// more, because what a cell measures depends on the font and the
    /// padding the terminal was started with.
    pub fn cells(self) -> Option<(u16, u16)> {
        WindowSize::PRESETS
            .iter()
            .position(|preset| *preset == self)
            .map(|at| PRESET_CELLS[at])
    }
}

/// The grid each preset gives, in the order the presets are in.
const PRESET_CELLS: [(u16, u16); 5] = [(100, 30), (120, 36), (140, 42), (160, 48), (180, 54)];

impl Default for WindowSize {
    /// The middle preset, 120 by 36 cells in foot with Omarchy's default
    /// font, which is the size the wireframes are drawn at.
    fn default() -> Self {
        WindowSize::PRESETS[1]
    }
}

/// Every setting, as one value. The fields are private because each has
/// a range and the type may not hold a value outside it: a setter takes
/// what a page or a codec has, and holds it to the range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    day_starts_at: u8,
    week_starts_on: WeekStart,
    work_days: WorkDays,
    review_opens_itself: bool,
    due_ahead_days: u16,
    backfill_days: u16,
    pile_horizon_days: u16,
    floating_window: bool,
    window_size: WindowSize,
    mouse: bool,
    message_seconds: u8,
    date_style: DateStyle,
    confirm_delete: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            day_starts_at: 5,
            week_starts_on: WeekStart::default(),
            work_days: WorkDays::default(),
            review_opens_itself: true,
            due_ahead_days: 0,
            backfill_days: 0,
            pile_horizon_days: 0,
            floating_window: true,
            window_size: WindowSize::default(),
            mouse: true,
            message_seconds: 4,
            date_style: DateStyle::default(),
            confirm_delete: false,
        }
    }
}

/// The ranges the numbers are held to.
const HOURS: (i64, i64) = (0, 23);
const AHEAD: (i64, i64) = (0, 365);
const BACKFILL: (i64, i64) = (0, 365);
const HORIZON: (i64, i64) = (0, 3650);
const SECONDS: (i64, i64) = (0, 60);

impl Settings {
    /// The working day of an instant: the date it falls on once the
    /// hours before the day began are taken off it, so 01:30 on Saturday
    /// belongs to Friday while the day starts at 5 (DOMAIN.md section 2).
    pub fn working_day(&self, instant: &Zoned) -> Date {
        instant
            .saturating_sub(Span::new().hours(i64::from(self.day_starts_at)))
            .date()
    }

    pub fn day_starts_at(&self) -> u8 {
        self.day_starts_at
    }

    pub fn set_day_starts_at(&mut self, hour: i64) {
        self.day_starts_at = clamp(hour, HOURS.0, HOURS.1) as u8;
    }

    pub fn week_starts_on(&self) -> WeekStart {
        self.week_starts_on
    }

    pub fn set_week_starts_on(&mut self, start: WeekStart) {
        self.week_starts_on = start;
    }

    pub fn work_days(&self) -> WorkDays {
        self.work_days
    }

    /// An empty set is held here and refused by `change_settings`, so
    /// that the last day can be turned off on the way to turning another
    /// one on.
    pub fn set_work_days(&mut self, days: WorkDays) {
        self.work_days = days;
    }

    pub fn toggle_work_day(&mut self, day: Weekday) {
        self.work_days.toggle(day);
    }

    pub fn review_opens_itself(&self) -> bool {
        self.review_opens_itself
    }

    pub fn set_review_opens_itself(&mut self, opens: bool) {
        self.review_opens_itself = opens;
    }

    pub fn due_ahead_days(&self) -> u16 {
        self.due_ahead_days
    }

    pub fn set_due_ahead_days(&mut self, days: i64) {
        self.due_ahead_days = clamp(days, AHEAD.0, AHEAD.1) as u16;
    }

    pub fn backfill_days(&self) -> u16 {
        self.backfill_days
    }

    /// Zero is no cap: every copy of a schedule is made.
    pub fn set_backfill_days(&mut self, days: i64) {
        self.backfill_days = clamp(days, BACKFILL.0, BACKFILL.1) as u16;
    }

    pub fn pile_horizon_days(&self) -> u16 {
        self.pile_horizon_days
    }

    /// Zero hides nothing: the whole pile is on the pile.
    pub fn set_pile_horizon_days(&mut self, days: i64) {
        self.pile_horizon_days = clamp(days, HORIZON.0, HORIZON.1) as u16;
    }

    pub fn floating_window(&self) -> bool {
        self.floating_window
    }

    pub fn set_floating_window(&mut self, floating: bool) {
        self.floating_window = floating;
    }

    pub fn window_size(&self) -> WindowSize {
        self.window_size
    }

    pub fn set_window_size(&mut self, size: WindowSize) {
        self.window_size = size;
    }

    pub fn mouse(&self) -> bool {
        self.mouse
    }

    pub fn set_mouse(&mut self, mouse: bool) {
        self.mouse = mouse;
    }

    pub fn message_seconds(&self) -> u8 {
        self.message_seconds
    }

    /// Zero keeps a message until the next key.
    pub fn set_message_seconds(&mut self, seconds: i64) {
        self.message_seconds = clamp(seconds, SECONDS.0, SECONDS.1) as u8;
    }

    pub fn date_style(&self) -> DateStyle {
        self.date_style
    }

    pub fn set_date_style(&mut self, style: DateStyle) {
        self.date_style = style;
    }

    pub fn confirm_delete(&self) -> bool {
        self.confirm_delete
    }

    pub fn set_confirm_delete(&mut self, confirm: bool) {
        self.confirm_delete = confirm;
    }

    /// The rows of the `settings` table, read into a value. Anything the
    /// codec does not recognise leaves that setting at its default.
    pub fn from_pairs<K: AsRef<str>, V: AsRef<str>>(
        pairs: impl IntoIterator<Item = (K, V)>,
    ) -> Settings {
        let mut settings = Settings::default();
        for (key, value) in pairs {
            let value = value.as_ref().trim();
            match key.as_ref() {
                DAY_STARTS_AT => number(value, |hour| settings.set_day_starts_at(hour)),
                WEEK_STARTS_ON => match value {
                    "monday" => settings.week_starts_on = WeekStart::Monday,
                    "sunday" => settings.week_starts_on = WeekStart::Sunday,
                    _ => {}
                },
                WORK_DAYS => {
                    if let Some(days) = work_days(value) {
                        settings.work_days = days;
                    }
                }
                REVIEW_OPENS_ITSELF => flag(value, |on| settings.review_opens_itself = on),
                DUE_AHEAD_DAYS => number(value, |days| settings.set_due_ahead_days(days)),
                BACKFILL_DAYS => number(value, |days| settings.set_backfill_days(days)),
                PILE_HORIZON_DAYS => number(value, |days| settings.set_pile_horizon_days(days)),
                FLOATING_WINDOW => flag(value, |on| settings.floating_window = on),
                WINDOW_SIZE => {
                    if let Some(size) = window_size(value) {
                        settings.window_size = size;
                    }
                }
                MOUSE => flag(value, |on| settings.mouse = on),
                MESSAGE_SECONDS => number(value, |seconds| settings.set_message_seconds(seconds)),
                DATE_STYLE => match value {
                    "locale" => settings.date_style = DateStyle::Locale,
                    "day_first" => settings.date_style = DateStyle::DayFirst,
                    "month_first" => settings.date_style = DateStyle::MonthFirst,
                    _ => {}
                },
                CONFIRM_DELETE => flag(value, |on| settings.confirm_delete = on),
                _ => {}
            }
        }
        settings
    }

    /// Every setting as a row, including the ones left at their default,
    /// so that what the program is running with can be read off the
    /// table.
    pub fn to_pairs(&self) -> Vec<(String, String)> {
        let days: Vec<&str> = self.work_days.iter().map(weekday_key).collect();
        [
            (DAY_STARTS_AT, self.day_starts_at.to_string()),
            (
                WEEK_STARTS_ON,
                match self.week_starts_on {
                    WeekStart::Monday => "monday".to_owned(),
                    WeekStart::Sunday => "sunday".to_owned(),
                },
            ),
            (WORK_DAYS, days.join(",")),
            (REVIEW_OPENS_ITSELF, self.review_opens_itself.to_string()),
            (DUE_AHEAD_DAYS, self.due_ahead_days.to_string()),
            (BACKFILL_DAYS, self.backfill_days.to_string()),
            (PILE_HORIZON_DAYS, self.pile_horizon_days.to_string()),
            (FLOATING_WINDOW, self.floating_window.to_string()),
            (
                WINDOW_SIZE,
                format!("{}x{}", self.window_size.width, self.window_size.height),
            ),
            (MOUSE, self.mouse.to_string()),
            (MESSAGE_SECONDS, self.message_seconds.to_string()),
            (
                DATE_STYLE,
                match self.date_style {
                    DateStyle::Locale => "locale".to_owned(),
                    DateStyle::DayFirst => "day_first".to_owned(),
                    DateStyle::MonthFirst => "month_first".to_owned(),
                },
            ),
            (CONFIRM_DELETE, self.confirm_delete.to_string()),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
    }

    /// What is wrong with these settings, as the sentence the hint bar
    /// shows. Everything a range can catch is caught by the setters, so
    /// the one thing left is a set that cannot be empty.
    fn problem(&self) -> Option<&'static str> {
        self.work_days
            .is_empty()
            .then_some("At least one day of the week must be a work day.")
    }
}

/// The settings the program is to run with from now on. Not undoable and
/// nothing on the undo stack, like the system operations (DOMAIN.md
/// section 12).
pub fn change_settings(model: &Model, settings: Settings) -> Result<Change, Rejected> {
    if let Some(problem) = settings.problem() {
        return Err(Rejected(problem.to_owned()));
    }
    let mut after = model.clone();
    after.settings = settings;
    Ok(Change {
        writes: diff(model, &after),
    })
}

fn clamp(value: i64, least: i64, most: i64) -> i64 {
    value.clamp(least, most)
}

fn number(value: &str, mut set: impl FnMut(i64)) {
    if let Ok(number) = value.parse() {
        set(number);
    }
}

fn flag(value: &str, mut set: impl FnMut(bool)) {
    match value {
        "true" => set(true),
        "false" => set(false),
        _ => {}
    }
}

/// `mon,tue,wed,thu,fri`. A list with a name in it that is not a day is
/// no list at all, rather than a shorter one.
fn work_days(value: &str) -> Option<WorkDays> {
    let mut days = Vec::new();
    for name in value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        days.push(weekday(name)?);
    }
    Some(WorkDays::of(days))
}

/// `870x650`.
fn window_size(value: &str) -> Option<WindowSize> {
    let (width, height) = value.split_once('x')?;
    Some(WindowSize::new(
        width.trim().parse().ok()?,
        height.trim().parse().ok()?,
    ))
}

fn weekday(name: &str) -> Option<Weekday> {
    Weekday::ALL
        .into_iter()
        .find(|day| weekday_key(*day) == name)
}

fn weekday_key(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}
