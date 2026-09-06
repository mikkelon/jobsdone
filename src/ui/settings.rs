//! The settings page: the list of settings, and beside it what the row
//! under the cursor does.
//!
//! Every row is `label ......... value`, because a page of settings is
//! read down the labels and across to the values, and the leader is what
//! keeps the two ends of a wide line one row. What each setting may hold
//! is the domain's (DOMAIN.md section 19); the words for it are here.

use super::{
    Canvas, Column, Content, PaneView, Section, accent, caret_line, count, counted, dim, header,
    plain, wrapped,
};
use crate::app::{App, List, RowId, SettingDraft, SettingGroup, SettingRow, setting_rows};
use crate::domain::{DateStyle, Settings, WeekStart, Weekday};

/// The blank cell each side of the leader, so a label and a value are
/// never read as one word joined by dots.
const GAP: u16 = 1;

/// The list, as the groups the page draws it in. A run of rows under one
/// label is one group; which rows are in which is the page's own
/// (`app::setting_rows`).
pub(super) fn view() -> PaneView<'static> {
    let rows = setting_rows();
    let mut sections = Vec::new();
    let mut at = 0;
    while at < rows.len() {
        let group = rows[at].0;
        let end = at
            + rows[at..]
                .iter()
                .take_while(|(other, _)| *other == group)
                .count();
        sections.push(Section {
            label: group_label(group),
            count: None,
            content: Content::Settings(&rows[at..end]),
            add: None,
        });
        at = end;
    }
    PaneView {
        title: "Settings".to_owned(),
        sub: String::new(),
        // What the app does not hold, said where somebody looking for it
        // would look (DESIGN.md section 11).
        right: "colour and font come from the terminal".to_owned(),
        sections,
        foot: None,
        // Every setting there is is on the page, so it is never empty.
        empty: ["", ""],
    }
}

fn group_label(group: SettingGroup) -> &'static str {
    match group {
        SettingGroup::Day => "Day",
        SettingGroup::WorkDays => "Work days",
        SettingGroup::Review => "Review",
        SettingGroup::Window => "Window",
        SettingGroup::Looks => "Looks",
    }
}

/// ` Day starts at ....................................... 5:00`
///
/// The value is in accent on the cursor row, which is the one row whose
/// value a key would change.
pub(super) fn row(
    canvas: &mut Canvas,
    column: Column,
    y: u16,
    row: SettingRow,
    app: &App,
    on: bool,
) {
    let Column { x, width, .. } = column;
    let left = canvas.put(x + 2, y, label(row), plain());
    let edge = x + width.saturating_sub(2);

    if let Some(draft) = app.setting_draft().filter(|draft| draft.row == row) {
        field(
            canvas,
            left + GAP,
            y,
            edge.saturating_sub(left + GAP),
            draft,
        );
        return;
    }

    let value = value(app.settings(), row);
    let at = edge.saturating_sub(count(&value));
    canvas.put(at, y, &value, if on { accent() } else { dim() });
    let leader = at.saturating_sub(left + 2 * GAP);
    if leader > 0 {
        canvas.put(left + GAP, y, &".".repeat(leader as usize), dim());
    }
}

/// The value being typed in place of the value it will replace.
fn field(canvas: &mut Canvas, x: u16, y: u16, width: u16, draft: &SettingDraft) {
    if width == 0 {
        return;
    }
    caret_line(canvas, x, y, width, &draft.text, draft.caret);
}

/// What the page calls a setting. The seven work days are a row each, so
/// each is called by its own name under the label they share.
fn label(row: SettingRow) -> &'static str {
    match row {
        SettingRow::DayStartsAt => "Day starts at",
        SettingRow::WeekStartsOn => "Week starts on",
        SettingRow::WorkDay(day) => weekday(day),
        SettingRow::ReviewOpensItself => "Open the review on launch",
        SettingRow::DueAheadDays => "Surface due tasks early",
        SettingRow::BackfillDays => "Catch up recurring tasks",
        SettingRow::PileHorizonDays => "Hide pile tasks older than",
        SettingRow::FloatingWindow => "Floating window",
        SettingRow::WindowSize => "Window size",
        SettingRow::Mouse => "Mouse",
        SettingRow::DateOrder => "Date order",
        SettingRow::MessageSeconds => "Hint bar messages stand for",
        SettingRow::ConfirmDelete => "Confirm before delete",
    }
}

/// What the pane beside the list calls it, which for one of the seven
/// toggles is the setting they make up rather than the day.
fn title(row: SettingRow) -> &'static str {
    match row {
        SettingRow::WorkDay(_) => "Work days",
        other => label(other),
    }
}

fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

/// A setting as the row writes it: a toggle as `on` or `off`, a number
/// with its unit, and the number that means "none of it" as the words
/// for what none of it does.
fn value(settings: &Settings, row: SettingRow) -> String {
    let days = |n: u16| counted(n as usize, "day", "days");
    match row {
        SettingRow::DayStartsAt => format!("{}:00", settings.day_starts_at()),
        SettingRow::WeekStartsOn => match settings.week_starts_on() {
            WeekStart::Monday => "Monday",
            WeekStart::Sunday => "Sunday",
        }
        .to_owned(),
        SettingRow::WorkDay(day) => on_off(settings.work_days().contains(day)),
        SettingRow::ReviewOpensItself => on_off(settings.review_opens_itself()),
        SettingRow::DueAheadDays => match settings.due_ahead_days() {
            0 => "on its day".to_owned(),
            n => format!("{} before", days(n)),
        },
        SettingRow::BackfillDays => match settings.backfill_days() {
            0 => "every missed day".to_owned(),
            n => format!("the last {}", days(n)),
        },
        SettingRow::PileHorizonDays => match settings.pile_horizon_days() {
            0 => "never".to_owned(),
            n => days(n),
        },
        SettingRow::FloatingWindow => on_off(settings.floating_window()),
        SettingRow::WindowSize => {
            let size = settings.window_size();
            let grid = match size.cells() {
                Some((columns, rows)) => format!("{columns} by {rows} cells"),
                None => "custom".to_owned(),
            };
            format!("{}x{} · {grid}", size.width, size.height)
        }
        SettingRow::Mouse => on_off(settings.mouse()),
        SettingRow::DateOrder => match settings.date_style() {
            DateStyle::Locale => "as the locale writes it",
            DateStyle::DayFirst => "day first",
            DateStyle::MonthFirst => "month first",
        }
        .to_owned(),
        SettingRow::MessageSeconds => match settings.message_seconds() {
            0 => "until the next key".to_owned(),
            n => counted(n as usize, "second", "seconds"),
        },
        SettingRow::ConfirmDelete => on_off(settings.confirm_delete()),
    }
}

fn on_off(on: bool) -> String {
    if on { "on" } else { "off" }.to_owned()
}

/// What a setting does, in the words DOMAIN.md section 19 gives it.
fn about(row: SettingRow) -> &'static str {
    match row {
        SettingRow::DayStartsAt => {
            "The hour the working day rolls over. 01:30 on Saturday belongs to Friday while it \
             is 5."
        }
        SettingRow::WeekStartsOn => {
            "Where the history's \"this week\" and \"last week\" fall, and the first column of \
             the calendar and the weekday row of the repeat card."
        }
        SettingRow::WorkDay(_) => {
            "What \"every work day\" repeats on and what \"next work day\" on the move card \
             means. At least one day of the week has to be one."
        }
        SettingRow::ReviewOpensItself => {
            "Whether the morning review opens itself on the first launch of a day. Off, it is \
             only opened with M."
        }
        SettingRow::DueAheadDays => {
            "How many days before its due date a backlog task is put in front of you in the \
             morning review. At 0 it surfaces on the due date and on every day after until it \
             is dealt with; at 3 it also surfaces on the three days before."
        }
        SettingRow::BackfillDays => {
            "A recurring task gets a fresh copy on every day its schedule names. After days \
             away from the app, the copies for the days you missed are made on the next launch, \
             each landing on the review pile. This caps how far back that goes: at 7, only the \
             last week's missed copies are made and older ones are skipped for good. Every \
             missed day makes them all."
        }
        SettingRow::PileHorizonDays => {
            "An unfinished task from a day more than N days ago stays on its day but is left \
             out of the pile and its count."
        }
        SettingRow::FloatingWindow => {
            "On Hyprland, whether the app opens in a centred floating window or tiles. Written \
             to the Hyprland rule the moment it changes."
        }
        SettingRow::WindowSize => {
            "The floating window's size in logical pixels. h and l step through five sizes, \
             named by the cells they give in foot with Omarchy's default font; Enter types any \
             other. 870 by 650 is 120 by 36 cells, the size the screens are designed at."
        }
        SettingRow::Mouse => {
            "Whether the app takes the mouse. Off, the terminal's own text selection works \
             again and the keyboard does everything."
        }
        SettingRow::DateOrder => {
            "Fri 5 Sep or Fri Sep 5, everywhere a date is written. The locale is what LC_ALL, \
             LC_TIME or LANG says about where you are."
        }
        SettingRow::MessageSeconds => {
            "How long \"closed X · u undo\" stays in the hint bar when no key follows it."
        }
        SettingRow::ConfirmDelete => {
            "x asks first instead of deleting and offering u. Applies to tasks, notes and the \
             review pile."
        }
    }
}

/// The pane beside the list: what the cursor row does, and what it holds
/// when nobody has changed it. Under 100 columns there is no room for it
/// and the list has the window (DESIGN.md section 11).
pub(super) fn about_the_setting(canvas: &mut Canvas, app: &App, column: Column, header_row: u16) {
    let Column { x, width, .. } = column;
    let Some(row) = app.cursor(List::Settings).and_then(RowId::setting) else {
        return;
    };
    let view = PaneView {
        title: title(row).to_owned(),
        sub: String::new(),
        right: String::new(),
        sections: Vec::new(),
        foot: None,
        empty: ["", ""],
    };
    header(canvas, x, width, header_row, &view, false);

    let text = wrapped(about(row), width.saturating_sub(3));
    let mut y = column.top;
    for (line, _) in text {
        if y > column.bottom {
            return;
        }
        canvas.put(x + 2, y, &line, plain());
        y += 1;
    }
    if y + 1 > column.bottom {
        return;
    }
    let default = value(&Settings::default(), row);
    let at = canvas.put(x + 2, y + 1, "Default: ", dim());
    canvas.put(at, y + 1, &default, dim());
}
