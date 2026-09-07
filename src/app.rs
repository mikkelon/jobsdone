//! Application state, the launch sequence, reloading, turning actions
//! into commands, and the screen layout.

use std::ops::Range;

use jiff::civil::Date;
use jiff::{Span, Zoned};
use tracing::warn;
use unicode_segmentation::UnicodeSegmentation;

use crate::domain::{
    self, BacklogView, Change, Command, Context, DateStyle, DayList, DayView, Id, Model, MonthDay,
    NotesView, Pile, Place, Rule, SearchResults, Settings, Store, StoreError, Surfaced, WeekStart,
    Weekday, Write,
};

/// The two domain types the desktop is spoken to in. They cross that
/// seam through here so that `desktop` never names the domain
/// (ARCHITECTURE.md section 2).
pub use crate::domain::{DateOrder, WindowSize};
use crate::input::{
    self, Action, Binding, Field, KeyContext, NotesPane, Pane, PopupKind, ReviewStep, Shown,
};

mod spelling;

#[cfg(test)]
mod tests;

pub mod spelling;

/// The most weeks apart the repeat card offers, which is a year.
const WEEKS_APART: usize = 52;

/// How many of the next dates the repeat card previews.
const PREVIEW: usize = 3;

/// The length the undo stack is held to. The domain does not choose the
/// number (DOMAIN.md section 11); a hundred is more than a day's work and
/// small enough to load with everything else.
const UNDO_CAP: usize = 100;

/// What the window manager can be asked to do about the window the
/// program is in. The application never names one: `main.rs` hands it
/// whatever is out there, and a machine with no window manager it knows
/// gets an implementation that says so.
pub trait Desktop {
    /// Whether there is a window manager here to ask.
    fn available(&self) -> bool;

    /// Puts the window rule where the window manager reads it, or says
    /// in one sentence why it could not.
    fn apply_window(&self, floating: bool, size: WindowSize) -> Result<(), String>;

    /// Gives the window the program is in this size now, so a size being
    /// chosen is seen rather than read. Asked only of a floating window.
    /// The answer is whether there was a window manager running to do it,
    /// because a rule written where nothing is running is not a failure
    /// and is also not something anybody just watched happen.
    fn preview(&self, size: WindowSize) -> Result<bool, String>;
}

/// What the environment says about the person at the keyboard. The
/// domain reads no environment, so `main.rs` resolves this once and the
/// application settles it against the `date_style` setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Locale {
    pub dates: DateOrder,
}

/// What the event loop should do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
    CopyNote(String),
}

/// Which of the three pages the window is showing (DESIGN.md section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Notes,
    /// The settings, which is a page rather than a popup because it is
    /// read down and worked through rather than answered (DESIGN.md
    /// section 11).
    Settings,
}

/// A list of rows, on either page. The cursor is one per list, so
/// switching pane and coming back lands where it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Day,
    Backlog,
    /// The list of days the backlog pane becomes while the day pane is on
    /// a day other than today (DESIGN.md section 6).
    Days,
    Notes,
    /// The rows of whichever step of the review is on screen. The review
    /// takes the whole window, so it is never a list beside another one.
    Review,
    /// The settings, which are one list whatever the window's width: the
    /// pane beside them describes the cursor row rather than listing
    /// anything of its own.
    Settings,
}

/// Which row of a list the cursor is on.
///
/// A pane draws tasks and, under the backlog's groups, the schedules
/// that make tasks; the notes page draws notes. An id alone would not
/// say which of them a row is, and the three number from one apiece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowId {
    Task(Id),
    Schedule(Id),
    Note(Id),
    /// A row of the day list, which is a date rather than a row of the
    /// model: a day exists because something was planned for it.
    Day(Date),
    /// A row of the settings page, which is a setting rather than
    /// anything the model holds a row for.
    Setting(SettingRow),
}

impl RowId {
    /// The task the row is, if it is one.
    pub fn task(self) -> Option<Id> {
        match self {
            RowId::Task(id) => Some(id),
            _ => None,
        }
    }

    /// The schedule the row is, if it is one.
    pub fn schedule(self) -> Option<Id> {
        match self {
            RowId::Schedule(id) => Some(id),
            _ => None,
        }
    }

    /// The note the row is, if it is one.
    pub fn note(self) -> Option<Id> {
        match self {
            RowId::Note(id) => Some(id),
            _ => None,
        }
    }

    /// The day the row is, if it is one.
    pub fn day(self) -> Option<Date> {
        match self {
            RowId::Day(day) => Some(day),
            _ => None,
        }
    }

    /// The setting the row is, if it is one.
    pub fn setting(self) -> Option<SettingRow> {
        match self {
            RowId::Setting(row) => Some(row),
            _ => None,
        }
    }
}

/// Which group of a pane a row is in.
///
/// The domain decides what is in each group; the application needs the
/// name of the group because a key on a row can mean something different
/// in each: `J` swaps neighbours within one group, and a moved row is a
/// pointer rather than a task to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Focus,
    Plan,
    Done,
    Moved,
    Ordinary,
    Waiting,
    /// The schedules the backlog pane lists under its two groups
    /// (DOMAIN.md section 7). A row here is a schedule, not a task.
    Schedules,
    /// The day list the backlog pane becomes while another day is shown.
    /// A row here is a day, not a task.
    Days,
    Notes,
    /// A row of the review. Its steps are grouped by day and by what
    /// surfaced the task, and neither changes what a key on the row
    /// means, so one group is the whole of it.
    Review,
    /// A row of the settings page. The page draws its rows under five
    /// labels, but every key on one means the same thing, so the keys
    /// know one group.
    Settings,
}

impl Group {
    /// Whether `J` and `K` mean anything in it. Done is ordered by the
    /// time each task was closed and Moved by when the task left, so
    /// neither has an order to change.
    fn is_ordered_by_hand(self) -> bool {
        matches!(
            self,
            Group::Focus | Group::Plan | Group::Ordinary | Group::Waiting
        )
    }
}

/// One row of the settings page: one of the settings of DOMAIN.md
/// section 19, except the work days, which are a row each because each
/// day is a toggle of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingRow {
    DayStartsAt,
    WeekStartsOn,
    WorkDay(Weekday),
    ReviewOpensItself,
    DueAheadDays,
    BackfillDays,
    PileHorizonDays,
    FloatingWindow,
    WindowSize,
    Mouse,
    DateOrder,
    MessageSeconds,
    ConfirmDelete,
    SpellCheckNotes,
}

impl SettingRow {
    /// Whether the value is typed rather than stepped: a number, or a
    /// size, which has no neighbour worth calling the next one.
    pub fn is_typed(self) -> bool {
        matches!(
            self,
            SettingRow::DayStartsAt
                | SettingRow::DueAheadDays
                | SettingRow::BackfillDays
                | SettingRow::PileHorizonDays
                | SettingRow::WindowSize
                | SettingRow::MessageSeconds
        )
    }
}

/// The label a run of settings rows is drawn under. What a setting does
/// decides which group it is in, so the grouping belongs with the page
/// rather than with the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingGroup {
    Day,
    WorkDays,
    Review,
    Window,
    Looks,
    Notes,
}

/// The rows of the settings page, in the order they are drawn, each
/// under its group. One list, so the cursor walks it the way it walks
/// any other.
const SETTINGS: [(SettingGroup, SettingRow); 20] = [
    (SettingGroup::Day, SettingRow::DayStartsAt),
    (SettingGroup::Day, SettingRow::WeekStartsOn),
    (SettingGroup::WorkDays, SettingRow::WorkDay(Weekday::Mon)),
    (SettingGroup::WorkDays, SettingRow::WorkDay(Weekday::Tue)),
    (SettingGroup::WorkDays, SettingRow::WorkDay(Weekday::Wed)),
    (SettingGroup::WorkDays, SettingRow::WorkDay(Weekday::Thu)),
    (SettingGroup::WorkDays, SettingRow::WorkDay(Weekday::Fri)),
    (SettingGroup::WorkDays, SettingRow::WorkDay(Weekday::Sat)),
    (SettingGroup::WorkDays, SettingRow::WorkDay(Weekday::Sun)),
    (SettingGroup::Review, SettingRow::ReviewOpensItself),
    (SettingGroup::Review, SettingRow::DueAheadDays),
    (SettingGroup::Review, SettingRow::BackfillDays),
    (SettingGroup::Review, SettingRow::PileHorizonDays),
    (SettingGroup::Window, SettingRow::FloatingWindow),
    (SettingGroup::Window, SettingRow::WindowSize),
    (SettingGroup::Window, SettingRow::Mouse),
    (SettingGroup::Looks, SettingRow::DateOrder),
    (SettingGroup::Looks, SettingRow::MessageSeconds),
    (SettingGroup::Looks, SettingRow::ConfirmDelete),
    (SettingGroup::Notes, SettingRow::SpellCheckNotes),
];

/// The settings page as a list of rows. `ui` draws them in this order
/// and under these labels, and the cursor moves down the same list.
pub fn setting_rows() -> &'static [(SettingGroup, SettingRow)] {
    &SETTINGS
}

/// A number or a window size being typed on a settings row.
///
/// Uncommitted text lives only in the application (ARCHITECTURE.md rule
/// 8); Enter turns it into one `change_settings` and Escape drops it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingDraft {
    pub row: SettingRow,
    pub text: String,
    /// Where the caret is, in grapheme clusters, as in every other field.
    pub caret: usize,
}

/// What the review did to a row, which is what the row says it did.
///
/// It is the session's own memory rather than something read back from
/// the model: `k keep` changes nothing at all, and a task another window
/// has moved since is still one this review has not answered (DOMAIN.md
/// sections 13 and 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decided {
    Done,
    Moved(Place),
    Deleted,
    Kept,
    Dated,
    Waiting,
}

/// The morning review while it is on screen: the rows each step opened
/// with, and the decision made for each.
///
/// It is held in memory for the length of the review and stored nowhere.
/// Closing the window mid-review forgets it and the pile itself is the
/// record (DOMAIN.md section 13).
pub struct Review {
    step: ReviewStep,
    /// The steps this review shows, in order. An empty step is skipped,
    /// so there is one of them or both.
    steps: Vec<ReviewStep>,
    /// The pile as the review opened it, drawn again on every change.
    pile: Option<Pile>,
    /// The surfaced set as the second step opened it, which is later
    /// than the review itself: a task sent back to the backlog on the
    /// first step surfaces on the second (DOMAIN.md section 8).
    surfaced: Option<Surfaced>,
    /// The decisions, newest last, so that `u` takes back the last one.
    decided: Vec<(Id, Decided)>,
}

impl Review {
    pub fn step(&self) -> ReviewStep {
        self.step
    }

    /// Which step is on screen and how many there are: "step 1 of 2".
    pub fn steps(&self) -> (usize, usize) {
        let at = self
            .steps
            .iter()
            .position(|step| *step == self.step)
            .unwrap_or_default();
        (at + 1, self.steps.len())
    }

    pub fn pile(&self) -> Option<&Pile> {
        self.pile.as_ref()
    }

    pub fn surfaced(&self) -> Option<&Surfaced> {
        self.surfaced.as_ref()
    }

    /// What the review did to a row, if it has done anything yet.
    pub fn decision(&self, task: Id) -> Option<Decided> {
        self.decided
            .iter()
            .find(|(other, _)| *other == task)
            .map(|(_, how)| *how)
    }

    /// How many rows the step asks a decision about, and how many of
    /// them have had one.
    pub fn progress(&self) -> (usize, usize) {
        let asked = self.asked();
        let handled = asked
            .iter()
            .filter(|task| self.decision(**task).is_some())
            .count();
        (handled, asked.len())
    }

    /// The rows of the step on screen, in the order they are drawn.
    fn rows(&self) -> Vec<Id> {
        match self.step {
            ReviewStep::Pile => self
                .pile
                .iter()
                .flat_map(|pile| pile.days.iter())
                .flat_map(|day| day.rows.iter())
                .map(|row| row.task)
                .collect(),
            ReviewStep::Surfaced => self
                .surfaced
                .iter()
                .flat_map(|surfaced| {
                    surfaced
                        .due
                        .iter()
                        .chain(&surfaced.reminders)
                        .chain(&surfaced.also_starting_today)
                })
                .map(|row| row.task)
                .collect(),
        }
    }

    /// The rows the step asks a decision about, which on the second step
    /// leaves out the copies it lists for information.
    fn asked(&self) -> Vec<Id> {
        match self.step {
            ReviewStep::Pile => self.rows(),
            ReviewStep::Surfaced => self
                .surfaced
                .iter()
                .flat_map(|surfaced| surfaced.due.iter().chain(&surfaced.reminders))
                .map(|row| row.task)
                .collect(),
        }
    }

    fn forget(&mut self, task: Id) {
        self.decided.retain(|(other, _)| *other != task);
    }
}

/// The hint bar's last word: what just happened, and whether `u` takes it
/// back. It stands until the next key or for the seconds
/// `message_seconds` names, whichever comes first (DESIGN.md section 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub undo: bool,
    pub said_at: Zoned,
}

/// A title being typed on a row. Uncommitted text lives here and nowhere
/// else (ARCHITECTURE.md rule 8); Enter turns it into one command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Editor {
    pub field: Field,
    /// The list the field is in, which is also the place a new task goes.
    pub list: List,
    /// The task being renamed; none while one is being added.
    pub task: Option<Id>,
    pub text: String,
    pub caret: usize,
}

/// The body of the open note while it is being typed.
///
/// Uncommitted text lives only in the application (ARCHITECTURE.md rule
/// 8). A body is not confirmed the way a title is: it becomes an
/// `EditNote` on the first tick after it changes and again when the note
/// is left, so at most a quarter of a second of typing is ever at risk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    /// The note being typed into, held by id so that a reload cannot turn
    /// it into another one.
    pub note: Id,
    pub text: String,
    /// Where the caret is, in grapheme clusters from the start of the
    /// body: what a person calls a character, and what a terminal draws
    /// in one cell (or two).
    pub caret: usize,
}

/// The misspellings of the open note, kept between redraws.
///
/// Drawing is a pure function of what the application holds
/// (ARCHITECTURE.md rule 4), so the checker is run here. It is asked
/// once per change to the body and at no other time: a redraw, a caret
/// moving and a tick with nothing typed all read what it last found.
///
/// The dictionary the checker reads is built when the first note is
/// checked rather than at launch, so a program that only ever looks at
/// tasks never pays for one.
#[derive(Default)]
struct Spelling {
    checker: Option<spelling::SpellChecker>,
    /// The note the words are of, and the body they were read from,
    /// which together are what a second run is spared by.
    note: Option<Id>,
    text: String,
    /// Every word the checker did not know, in grapheme clusters from
    /// the start of the body, which is what the caret and the drawing
    /// both count in.
    found: Vec<Range<usize>>,
    /// The ones drawn: the word the caret is in is left alone while it
    /// is being typed.
    shown: Vec<Range<usize>>,
    /// The caret `shown` was worked out for, and none while the note is
    /// only being looked at, when every word found is drawn.
    caret: Option<usize>,
    /// How many times the checker was asked, which is what holds the
    /// reuse honest: a redraw, a caret moving and a quiet tick may not
    /// add to it.
    #[cfg(test)]
    runs: usize,
}

impl Spelling {
    /// The misspellings of this body: from the checker when the body has
    /// changed, and from the last run when it has not.
    fn of(&mut self, note: Id, text: &str, caret: Option<usize>) {
        let same = self.note == Some(note) && self.text == text;
        if !same {
            // A note with nothing written in it has nothing to check,
            // and building a dictionary to say so is a poor way to spend
            // the moment `a` opens one.
            self.found = if text.trim().is_empty() {
                Vec::new()
            } else {
                let mut found = self
                    .checker
                    .get_or_insert_with(spelling::SpellChecker::default)
                    .check(text);
                #[cfg(test)]
                {
                    self.runs += 1;
                }
                // In the order the body is drawn, and no two of them
                // over the same character, so that a line being drawn
                // can walk them alongside its text rather than reading
                // the whole note for every character of it.
                found.sort_by_key(|word| (word.start, word.end));
                merged(found)
            };
            self.note = Some(note);
            self.text.clear();
            self.text.push_str(text);
        }
        // The caret moves far more often than the body changes, and what
        // it changes is only which of the words found are drawn.
        if !same || self.caret != caret {
            self.caret = caret;
            self.shown = self
                .found
                .iter()
                .filter(|word| !being_typed(word, caret))
                .cloned()
                .collect();
        }
    }

    /// Nothing to check: another page, the setting off, or no note open.
    /// The checker itself stays, so a setting turned off and on again
    /// does not build the dictionary a second time.
    fn forget(&mut self) {
        self.note = None;
        self.text.clear();
        self.found.clear();
        self.shown.clear();
        self.caret = None;
    }
}

/// Words in the order they were sorted into, with any that lie over one
/// another joined. Two underlines over one character are one underline,
/// and a checker that answers with a word inside a word says nothing
/// more than the outer one already does.
fn merged(words: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut joined: Vec<Range<usize>> = Vec::with_capacity(words.len());
    for word in words {
        match joined.last_mut() {
            // Touching is not overlapping: the caret between two words
            // is in the one it is at the end of, and joining them would
            // hold both back while one of them is being typed.
            Some(last) if word.start < last.end => last.end = last.end.max(word.end),
            _ => joined.push(word),
        }
    }
    joined
}

/// Whether the caret is in a word, its far end included. A word is left
/// alone until the caret has moved off it, so that what is being typed
/// is not underlined before it is finished: the end-of-word caret is
/// where every word spends its whole life being written.
fn being_typed(word: &Range<usize>, caret: Option<usize>) -> bool {
    caret.is_some_and(|at| word.start <= at && at <= word.end)
}

/// A popup over the page. What is typed into it is application state for
/// the same reason a title being edited is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    pub kind: PopupKind,
    /// What has been typed into it, and where the caret is, in clusters.
    pub text: String,
    pub caret: usize,
    /// Which row of its list is selected. A popup lists commands, results
    /// and days, not model rows, so an index is what it means.
    pub selected: usize,
    /// The row the popup is about, held by id so that a reload cannot
    /// turn it into another one. The repeat card is about a schedule as
    /// readily as about a task.
    pub target: Option<RowId>,
    /// What a card is building before Enter turns it into a command.
    pub card: Card,
}

impl Popup {
    /// The date card's draft, if that is what this popup is.
    pub fn date(&self) -> Option<&DateDraft> {
        match &self.card {
            Card::Date(draft) => Some(draft),
            _ => None,
        }
    }

    /// The repeat card's draft, if that is what this popup is.
    pub fn repeat(&self) -> Option<&RepeatDraft> {
        match &self.card {
            Card::Repeat(draft) => Some(draft),
            _ => None,
        }
    }

    /// The task the popup is about, if the row it is about is one.
    pub fn task(&self) -> Option<Id> {
        self.target.and_then(RowId::task)
    }
}

/// A card is a form, so it holds what it has been told until Enter turns
/// it into a command. Uncommitted state, like the note being typed beside
/// it (ARCHITECTURE.md rule 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Card {
    /// A popup that answers with a key or a row, and builds nothing.
    None,
    Date(DateDraft),
    Repeat(RepeatDraft),
}

/// Which date the card is setting. One card, three things to set, and the
/// title says which (wireframe 06).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateKind {
    Due,
    Remind,
    /// The day the move card was asked to pick.
    Move,
    /// The day the day pane goes to, which is about no task at all.
    Go,
}

/// The date card: the day it is on, however that day was arrived at, and
/// which of its two controls has the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateDraft {
    pub kind: DateKind,
    /// What the calendar marks and what Enter applies. Typing a date the
    /// domain can read moves it, and so do the calendar keys.
    pub on: Date,
    /// Whether `tab` has moved the keyboard into the calendar, where
    /// single keys work again (DESIGN.md section 4).
    pub in_calendar: bool,
}

/// One row of the date card: its key and name from the key table, and the
/// day it means. No day is the pick that takes the date off.
#[derive(Clone, Copy, Debug)]
pub struct DateChoice {
    pub key: &'static str,
    pub label: &'static str,
    pub date: Option<Date>,
}

/// The repeat card: the parameters of every shape, kept while another
/// shape is looked at, so that stepping through the rows loses nothing.
/// Which shape is selected is the popup's selected row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepeatDraft {
    /// The weekdays the weekly shape repeats on, and which of the seven
    /// `h` and `l` are on.
    pub weekdays: Vec<Weekday>,
    pub weekday: usize,
    pub month_day: MonthDay,
    /// How many weeks apart, and the date they are counted from.
    pub weeks: u32,
    pub from: Date,
    /// The date the preview of the next dates counts on from, which is
    /// how far the schedule has already been generated.
    pub after: Date,
}

/// Where the move card sends a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveTarget {
    Day(Date),
    Backlog,
    /// The date card, which phase 8 draws.
    Pick,
}

/// One row of the move card: its key and its name from the key table, and
/// the day it means, which only the application can work out.
#[derive(Clone, Copy, Debug)]
pub struct MoveChoice {
    pub key: &'static str,
    pub label: &'static str,
    pub target: MoveTarget,
}

/// A rectangle of cells, in the terminal's own coordinates. `app` may not
/// name ratatui, and a mouse click is resolved here rather than there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn holds(self, column: u16, row: u16) -> bool {
        column >= self.x
            && column < self.x.saturating_add(self.width)
            && row >= self.y
            && row < self.y.saturating_add(self.height)
    }
}

/// Where a list was drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListArea {
    pub list: List,
    pub area: Rect,
}

/// Where one row was drawn, and which row it is. By id, never by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowArea {
    pub list: List,
    pub id: RowId,
    pub area: Rect,
}

/// Which pane and which row, with its task or note id, occupies which cell
/// rectangle. `ui::draw` returns one and the mouse is resolved against it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// Whether the window was too narrow for two panes and collapsed to
    /// tabs.
    pub narrow: bool,
    pub lists: Vec<ListArea>,
    pub rows: Vec<RowArea>,
}

impl Layout {
    fn row_at(&self, column: u16, row: u16) -> Option<RowArea> {
        self.rows
            .iter()
            .find(|drawn| drawn.area.holds(column, row))
            .copied()
    }

    fn list_at(&self, column: u16, row: u16) -> Option<List> {
        self.lists
            .iter()
            .find(|drawn| drawn.area.holds(column, row))
            .map(|drawn| drawn.list)
    }
}

/// The cursor of each list, by id.
#[derive(Clone, Copy, Debug, Default)]
struct Cursors {
    day: Option<RowId>,
    backlog: Option<RowId>,
    days: Option<RowId>,
    notes: Option<RowId>,
    review: Option<RowId>,
    settings: Option<RowId>,
}

/// The views the screen is drawn from, recomputed whenever the model or
/// the working day changes and at no other time.
#[derive(Default)]
struct Views {
    day: DayView,
    backlog: BacklogView,
    days: DayList,
    notes: NotesView,
    /// The size of the pile, which the status line counts in red.
    pile: usize,
}

pub struct App {
    store: Box<dyn Store>,
    desktop: Box<dyn Desktop>,
    /// What the environment writes dates like, which the `date_style`
    /// setting may override.
    locale: Locale,
    model: Model,
    /// The database version the model was loaded at, so a change made by
    /// another window can be noticed.
    version: u64,
    today: Date,
    /// The day the day pane is on, which is today until `[` steps it
    /// back. History is this page on another day, not a screen of its
    /// own (DESIGN.md section 6).
    showing: Date,
    views: Views,
    page: Page,
    /// The page `,` was pressed on, which is the page every way off the
    /// settings leads back to.
    came_from: Page,
    pane: Pane,
    notes_pane: NotesPane,
    popup: Option<Popup>,
    editor: Option<Editor>,
    /// The review, while it is on screen. It is a mode over the page
    /// rather than a page of its own (DESIGN.md section 5).
    review: Option<Review>,
    /// The open note, while the keyboard is in it.
    draft: Option<Draft>,
    /// The misspellings of the note on screen, worked out once per
    /// change to it and read by every redraw until it changes again.
    spelling: Spelling,
    /// The value being typed on a settings row, while one is.
    setting_draft: Option<SettingDraft>,
    message: Option<Message>,
    cursors: Cursors,
    /// The row a reorder is happening to, marked "moving" until the next
    /// key. Application state, like the title being typed (DOMAIN.md
    /// section 18).
    moving: Option<Id>,
    /// The row the mouse took hold of, while it holds it.
    dragging: Option<(List, RowId)>,
    /// Whether the window manager still has to be told what the window
    /// settings say. Held over the keys and settled on the next tick, so
    /// that a key held down on the size row costs one reload rather than
    /// one per repeat.
    window_owed: bool,
    layout: Layout,
    /// Where the clock comes from. The application is the only module
    /// that reads it (ARCHITECTURE.md section 3), which is also what
    /// makes it something a test can hold still: a test's app keeps the
    /// instant it was built with, and the program itself has no field
    /// here at all.
    #[cfg(test)]
    clock: Zoned,
}

impl App {
    /// Loads the model and runs the launch sequence: the recurring copies
    /// owed since the last launch, and then the review gate, unless the
    /// review is set to be opened by hand.
    pub fn new(
        store: Box<dyn Store>,
        desktop: Box<dyn Desktop>,
        locale: Locale,
        now: &Zoned,
    ) -> Result<App, StoreError> {
        let model = store.load()?;
        let version = store.version()?;
        let today = model.settings.working_day(now);

        let mut app = App {
            store,
            desktop,
            locale,
            model,
            version,
            today,
            showing: today,
            views: Views::default(),
            page: Page::Home,
            came_from: Page::Home,
            pane: Pane::Day,
            notes_pane: NotesPane::List,
            popup: None,
            editor: None,
            review: None,
            draft: None,
            spelling: Spelling::default(),
            setting_draft: None,
            message: None,
            cursors: Cursors::default(),
            moving: None,
            dragging: None,
            window_owed: false,
            layout: Layout::default(),
            #[cfg(test)]
            clock: now.clone(),
        };
        app.generate(now);
        app.refresh();
        app.rest_the_cursors();
        if app.model.settings.review_opens_itself() {
            app.open_the_review(true);
        }
        Ok(app)
    }

    /// The copies for every scheduled date since the last launch that
    /// the backfill setting still reaches.
    ///
    /// Recurring schedules are the one thing in the program that creates
    /// tasks on their own (DESIGN.md section 7). Generation is not a user
    /// action: it pushes nothing on the undo stack, and a second window
    /// making the same copy at the same moment loses the race, which is
    /// the failure DOMAIN.md section 10 says to ignore.
    fn generate(&mut self, now: &Zoned) {
        let change = domain::generate_copies(&self.model, now);
        if change.writes.is_empty() {
            return;
        }
        match self.store.commit(&change) {
            Ok(()) => self.model.apply(&change),
            Err(StoreError::Conflict) => {
                self.reload_if_stale();
            }
            Err(StoreError::Other(why)) => {
                warn!(%why, "the recurring copies could not be made");
            }
        }
        if let Ok(version) = self.store.version() {
            self.version = version;
        }
    }

    /// The single entry point for every event, ticks included.
    ///
    /// What an action leaves the open note saying is settled once, after
    /// it, rather than in each arm: a note is reached by typing in it,
    /// by the cursor moving to another one, by one being thrown away, by
    /// a reload on a tick, and by the setting being turned off, and
    /// several of the arms in front of that return early.
    pub fn update(&mut self, action: Action) -> Flow {
        let flow = self.dispatch(action);
        // A window being closed has no next redraw to check a note for.
        if flow != Flow::Quit {
            self.check_the_spelling();
        }
        flow
    }

    fn dispatch(&mut self, action: Action) -> Flow {
        // What the hint bar last said, and the mark on a row being
        // carried, stand until the next key.
        if !matches!(action, Action::Tick | Action::Resize | Action::FocusGained) {
            self.message = None;
            self.moving = None;
        }
        if matches!(action, Action::Tick) {
            self.forget_an_old_message();
        }

        match action {
            Action::Quit => {
                self.save_the_note();
                // The last quarter second of settings still has to
                // reach the rule. Nothing is shown along with it: the
                // window is closing.
                self.pay_the_window(false);
                return Flow::Quit;
            }
            Action::Tick | Action::FocusGained => {
                // The clock is read here and nowhere else, so the date
                // rolling over while the window is open is just a tick.
                let now = self.now();
                let today = self.model.settings.working_day(&now);
                let rolled = today != self.today;
                // A pane that was on today follows the day over; one
                // stepped back stays on the day it was looking at.
                if self.showing == self.today {
                    self.showing = today;
                }
                self.today = today;
                // A window left open past the hour the day starts has
                // reached a new day without a launch, and today's copies
                // are owed to it.
                if rolled {
                    self.generate(&now);
                }
                if self.reload_if_stale() || rolled {
                    self.refresh();
                }
                // The pause between keystrokes is when a note body is
                // written (ARCHITECTURE.md rule 8), after the reload, so
                // that a note another window has thrown away is not
                // written back.
                self.save_the_note();
                // Focus coming back is not the keys going quiet, so the
                // window manager waits for a tick.
                if matches!(action, Action::Tick) {
                    self.pay_the_window(true);
                }
            }
            Action::Resize => {}

            // In the date card's calendar the four keys walk the month
            // rather than a list of rows.
            Action::Down => {
                if !self.walk_the_calendar(Span::new().days(7)) {
                    self.step(true);
                }
            }
            Action::Up => {
                if !self.walk_the_calendar(Span::new().days(-7)) {
                    self.step(false);
                }
            }
            Action::PaneLeft => self.shift_pane(false, false),
            Action::PaneRight => self.shift_pane(true, false),
            Action::NextPane => self.next_control(),
            Action::NotesPage => self.turn_the_page(),
            Action::SettingsPage => self.turn_to_the_settings(),
            Action::OpenReview => self.reopen_the_review(),

            Action::Commands => self.open(PopupKind::Palette, None),
            Action::Search => self.open(PopupKind::Search, None),
            Action::Help => self.open(PopupKind::Help, None),
            Action::Cancel => self.back_out(),
            Action::Confirm => return self.confirm(),

            Action::Add => self.add(),
            Action::Edit => self.start_renaming(),
            Action::Close => self.close_or_reopen(),
            Action::Focus => self.turn_focus_over(),
            Action::Delete => self.delete(),
            Action::CopyNote => return self.copy_note(),
            Action::MoveDown => self.reorder(true),
            Action::MoveUp => self.reorder(false),
            Action::ToToday => self.pull_onto_today(),
            Action::ToBacklog => self.move_it(MoveTarget::Backlog),
            Action::Tomorrow
            | Action::NextWorkDay
            | Action::NextMonday
            | Action::InAWeek
            | Action::EndOfMonth => self.quick_pick(action),
            Action::ClearDate => self.take_the_date(None),
            Action::MoveToDay => self.open_the_move_card(),
            Action::GoToDate => self.pick_a_date(),
            Action::ThisCopy => self.answer_the_question(false),
            Action::ThisAndFuture => self.answer_the_question(true),
            Action::Undo => self.undo(),

            Action::PrevDay => self.step_the_day(-1),
            Action::NextDay => self.step_the_day(1),
            Action::Today => self.show_the_day(self.today),
            Action::DueBy => self.open_the_date_card(DateKind::Due),
            Action::RemindOn => self.open_the_date_card(DateKind::Remind),
            Action::PrevMonth => {
                self.walk_the_calendar(Span::new().months(-1));
            }
            Action::NextMonth => {
                self.walk_the_calendar(Span::new().months(1));
            }
            Action::Waiting => self.wait_on_someone(),
            Action::Repeat => self.open_the_repeat_card(),
            Action::EveryWorkDay
            | Action::EveryDay
            | Action::EveryWeek
            | Action::EveryMonth
            | Action::EveryFewWeeks
            | Action::StopRepeat => self.choose_the_shape(action),
            Action::Pick => self.pick(),
            Action::Keep => self.keep(),

            Action::Insert(typed) => self.type_in(typed),
            Action::Backspace => self.rub_out(),
            Action::DeleteForward => self.rub_forward(),
            Action::Left => {
                if !self.walk_the_calendar(Span::new().days(-1))
                    && !self.adjust(false)
                    && !self.adjust_a_setting(false)
                {
                    self.move_caret(false);
                }
            }
            Action::Right => {
                if !self.walk_the_calendar(Span::new().days(1))
                    && !self.adjust(true)
                    && !self.adjust_a_setting(true)
                {
                    self.move_caret(true);
                }
            }
            Action::LineStart => self.jump_to_the_edge(false),
            Action::LineEnd => self.jump_to_the_edge(true),

            Action::MouseDown { column, row } => self.point_at(column, row),
            Action::MouseDrag { column, row } => self.drag_to(column, row),
            Action::MouseUp { .. } => self.dragging = None,
            Action::Scroll { down, .. } => self.step(down),
        }
        Flow::Continue
    }

    /// The instant an action happens at, read once per action.
    /// What the domain is told about the world outside it, made afresh
    /// for every action because the clock has moved.
    fn context(&self) -> Context {
        Context {
            now: self.now(),
            undo_cap: UNDO_CAP,
            dates: self.dates(),
        }
    }

    /// The settings the program is running with.
    pub fn settings(&self) -> &Settings {
        &self.model.settings
    }

    /// Which way round a date is written: what the setting says, or what
    /// the locale does where the setting leaves it to the locale.
    pub fn dates(&self) -> DateOrder {
        match self.model.settings.date_style() {
            domain::DateStyle::Locale => self.locale.dates,
            domain::DateStyle::DayFirst => DateOrder::DayFirst,
            domain::DateStyle::MonthFirst => DateOrder::MonthFirst,
        }
    }

    /// The settings the program runs with from now on, or the sentence
    /// saying why not.
    ///
    /// The day may now start at another hour, so the working day is
    /// worked out again and a pane that was on today follows it. A window
    /// setting is the window manager's to keep, so it is owed to it from
    /// here and handed over on the next tick.
    pub fn change_settings(&mut self, settings: Settings) {
        self.take_the_settings(settings);
        // Whether a note is spell-checked is one of the settings, and
        // the reload in front of the change may have moved the note
        // besides. This is a way in of its own, so it settles the open
        // note the way `update` does rather than leaving the last
        // underlines on screen.
        self.check_the_spelling();
    }

    fn take_the_settings(&mut self, settings: Settings) {
        self.reload_if_stale();
        let window = (settings.floating_window(), settings.window_size());
        let was = (
            self.model.settings.floating_window(),
            self.model.settings.window_size(),
        );
        let change = match domain::change_settings(&self.model, settings) {
            Ok(change) => change,
            Err(rejected) => {
                self.say(rejected.to_string(), false);
                return;
            }
        };
        if self.commit(&change).is_none() {
            return;
        }

        let today = self.model.settings.working_day(&self.now());
        if self.showing == self.today {
            self.showing = today;
        }
        self.today = today;
        self.refresh();

        if window != was {
            self.window_owed = true;
        }
    }

    /// The window settings the window manager has not been told about,
    /// told to it now.
    ///
    /// A tick comes 250 ms after the last key (STACK.md section 2), so a
    /// key held down on the size row walks the presets and only the one
    /// it stops on reaches Hyprland, which rewrites its configuration and
    /// reloads for each one it is given. A floating window is then
    /// resized to what was chosen, so the size is seen rather than read;
    /// `showing` is false where there would be nobody to see it. The hint
    /// bar says which of the two happened, because a size on screen and a
    /// size that waits for the next launch are different answers.
    fn pay_the_window(&mut self, showing: bool) {
        if !std::mem::take(&mut self.window_owed) {
            return;
        }
        let floating = self.model.settings.floating_window();
        let size = self.model.settings.window_size();
        if let Err(why) = self.desktop.apply_window(floating, size) {
            self.say(why, false);
            return;
        }
        let shown = if showing && floating {
            match self.desktop.preview(size) {
                Ok(shown) => shown,
                Err(why) => {
                    self.say(why, false);
                    return;
                }
            }
        } else {
            false
        };
        self.say(
            if shown {
                "The window rule is written and the window is shown at that size."
            } else {
                "The window rule is written. It applies the next time the app opens."
            },
            false,
        );
    }

    fn now(&self) -> Zoned {
        #[cfg(test)]
        return self.clock.clone();
        #[cfg(not(test))]
        Zoned::now()
    }

    // ---- the model ---------------------------------------------------

    /// Recomputes every view. Views are pure functions of the model and
    /// the working day, so this is the only thing that has to happen when
    /// either changes.
    fn refresh(&mut self) {
        self.views = Views {
            day: domain::day_view(&self.model, self.showing, self.today),
            backlog: domain::backlog_view(&self.model, self.today),
            days: domain::day_list(&self.model, self.today),
            notes: domain::notes(&self.model),
            pile: domain::pile(&self.model, self.today).total,
        };
        // The review keeps the rows it opened with and draws them as the
        // tasks are now, so it is refreshed with everything else.
        if let Some(mut review) = self.review.take() {
            review.pile = review
                .pile
                .as_ref()
                .map(|opened| domain::pile_again(&self.model, self.today, opened));
            review.surfaced = review
                .surfaced
                .as_ref()
                .map(|opened| domain::surfaced_again(&self.model, self.today, opened));
            review.steps = self.steps_of(&review);
            self.review = Some(review);
        }
    }

    // ---- the morning review ------------------------------------------

    /// Opens the review, on the launch of a day that has something to
    /// review or whenever it is asked for, and says whether it opened.
    ///
    /// `gated` is the once-a-day rule: a launch takes it, and asking for
    /// the review again does not. A review with nothing in it is not an
    /// empty ceremony, so neither opens one (DESIGN.md section 5).
    fn open_the_review(&mut self, gated: bool) -> bool {
        let pile = domain::pile(&self.model, self.today);
        let surfaced = domain::surfaced(&self.model, self.today);
        if pile.total == 0 && surfaced.is_empty() {
            return false;
        }
        // StartReview changes nothing on a day it has already run on,
        // which is what makes the second window of a morning skip it.
        let gate = domain::start_review(&self.model, self.today);
        if gated && gate.is_none() {
            return false;
        }
        if let Some(gate) = gate {
            self.commit(&gate);
        }

        // The review opens on the first step with something in it.
        let (step, pile, surfaced) = if pile.total > 0 {
            (ReviewStep::Pile, Some(pile), None)
        } else {
            (ReviewStep::Surfaced, None, Some(surfaced))
        };
        self.review = Some(Review {
            step,
            steps: Vec::new(),
            pile,
            surfaced,
            decided: Vec::new(),
        });
        self.editor = None;
        self.popup = None;
        // The review is about today and ends by starting it, so a window
        // browsing another day comes back first.
        self.showing = self.today;
        self.refresh();
        self.rest_the_review_cursor();
        true
    }

    /// `M`, and the palette row it teaches: the review again, on a day
    /// it has already run on or after it was left half done.
    fn reopen_the_review(&mut self) {
        if self.review.is_some() {
            return;
        }
        if !self.open_the_review(false) {
            self.say(
                "Nothing to review: the pile is empty and nothing is due.",
                false,
            );
        }
    }

    /// The steps a review shows. The pile is the one it opened with; the
    /// surfaced step is whatever has surfaced by the time it is reached,
    /// so a date unearthed on the first step makes a second one.
    fn steps_of(&self, review: &Review) -> Vec<ReviewStep> {
        let mut steps = Vec::new();
        if review.pile.is_some() {
            steps.push(ReviewStep::Pile);
        }
        if review.surfaced.is_some() || !domain::surfaced(&self.model, self.today).is_empty() {
            steps.push(ReviewStep::Surfaced);
        }
        steps
    }

    /// Enter: on to the next step that has something in it, and off the
    /// review altogether when there is none.
    fn next_step(&mut self) {
        let Some(review) = &self.review else {
            return;
        };
        let next = review
            .steps
            .iter()
            .skip_while(|step| **step != review.step)
            .nth(1)
            .copied();
        if next != Some(ReviewStep::Surfaced) {
            self.review = None;
            return;
        }
        // The step is drawn from what has surfaced by now, which is what
        // DOMAIN.md section 13 means by computing it when it is shown.
        let surfaced = domain::surfaced(&self.model, self.today);
        if let Some(review) = &mut self.review {
            review.surfaced = Some(surfaced);
            review.step = ReviewStep::Surfaced;
        }
        self.refresh();
        self.rest_the_review_cursor();
    }

    /// One row answered: the review remembers what was done to it, and
    /// the cursor moves on to the next row that has not been.
    fn note_the_decision(&mut self, task: Id, how: Decided) {
        let Some(review) = &mut self.review else {
            return;
        };
        review.forget(task);
        review.decided.push((task, how));

        let review = &*review;
        let rows = review.rows();
        let at = rows.iter().position(|row| *row == task).unwrap_or_default();
        let waiting = |row: &&Id| review.decision(**row).is_none();
        // From here on, then from the top; when every row has been
        // answered the cursor stays where it is and Enter is what is left.
        let next = rows
            .get(at + 1..)
            .unwrap_or_default()
            .iter()
            .find(waiting)
            .or_else(|| rows[..at].iter().find(waiting))
            .copied();
        if let Some(next) = next {
            self.set_cursor(List::Review, RowId::Task(next));
        }
    }

    /// A key that puts a row back the way it was takes its decision back
    /// with it, so the review counts the row as still to be dealt with.
    fn take_the_decision_back(&mut self, task: Id) {
        if let Some(review) = &mut self.review {
            review.forget(task);
        }
    }

    /// `u` in the review takes back the last decision as well as the
    /// change it made. Only the last one, and only if the undo is of the
    /// same task: a decision further down was made by an earlier key,
    /// which an earlier `u` is what takes back.
    fn take_the_last_decision_back(&mut self, change: &Change) {
        let Some(review) = &mut self.review else {
            return;
        };
        let Some((task, _)) = review.decided.last().copied() else {
            return;
        };
        let undone = change
            .writes
            .iter()
            .any(|write| matches!(write, Write::PutTask(row) if row.id == task));
        if undone {
            review.decided.pop();
        }
    }

    /// `k`: the row is answered by leaving it exactly as it is, which is
    /// the one decision the model keeps no record of (DOMAIN.md section
    /// 12).
    fn keep(&mut self) {
        if self.review.is_none() {
            return;
        }
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        self.note_the_decision(id, Decided::Kept);
    }

    /// Where the cursor goes after a key on a row: on to the next row of
    /// the review still to be dealt with, and otherwise to the next row
    /// of the group the row left (DESIGN.md section 4).
    fn step_on(&mut self, list: List, next: Option<RowId>, task: Id, how: Decided) {
        if self.review.is_some() {
            self.note_the_decision(task, how);
            return;
        }
        if let Some(next) = next {
            self.set_cursor(list, next);
        }
    }

    /// Picks up what another window has done, and says whether it did.
    /// `version` moves only for a write made on another connection, so
    /// this is free when nothing has happened.
    fn reload_if_stale(&mut self) -> bool {
        let version = match self.store.version() {
            Ok(version) => version,
            Err(error) => {
                warn!(%error, "the database version could not be read");
                return false;
            }
        };
        if version == self.version {
            return false;
        }
        match self.store.load() {
            Ok(model) => {
                self.model = model;
                self.version = version;
                true
            }
            Err(error) => {
                warn!(%error, "the model could not be reloaded");
                false
            }
        }
    }

    /// One command, all the way through: reload if another window has
    /// written, ask the domain what the change would be, have storage
    /// commit it, and apply it here. The change comes back so that the
    /// caller can find the row it created.
    fn run(&mut self, command: Command) -> Option<Change> {
        self.reload_if_stale();
        let change = match domain::apply(&self.model, command, &self.context()) {
            Ok(change) => change,
            Err(rejected) => {
                self.say(rejected.to_string(), false);
                return None;
            }
        };
        let label = label_of(&change);
        self.commit(&change)?;
        if let Some(label) = label {
            self.say(label, true);
        }
        Some(change)
    }

    /// Commits a change and applies it to the model, or leaves the model
    /// as it was and says what went wrong (ARCHITECTURE.md rule 10).
    fn commit(&mut self, change: &Change) -> Option<()> {
        let committed = self.store.commit(change);
        if let Err(error) = committed {
            match error {
                StoreError::Conflict => {
                    self.say(
                        "Another window changed that first. Nothing was saved.",
                        false,
                    );
                }
                StoreError::Other(ref why) => {
                    warn!(%why, "the change could not be saved");
                    self.say("The change could not be saved.", false);
                }
            }
            return None;
        }
        self.model.apply(change);
        if let Ok(version) = self.store.version() {
            self.version = version;
        }
        self.refresh();
        Some(())
    }

    /// `u`: the top of the undo stack, applied as its inverse. An entry
    /// whose inverse no longer holds is dropped, and the hint bar says so
    /// (DOMAIN.md section 11).
    fn undo(&mut self) {
        self.reload_if_stale();
        let undone = match domain::undo(&self.model, &self.context()) {
            Ok(undone) => undone,
            Err(rejected) => {
                self.say(rejected.to_string(), false);
                return;
            }
        };
        if self.commit(&undone.change).is_none() {
            return;
        }
        self.take_the_last_decision_back(&undone.change);
        if let Some(task) = undone.task {
            self.follow_the_undo(task);
        }
        match undone.dropped {
            Some(why) => self.say(
                format!("{} could not be undone: {why}", undone.label),
                false,
            ),
            None => self.say(format!("Undone: {}", undone.label), false),
        }
    }

    /// Where the cursor is after `u`: on the task the undo brought back
    /// or changed, when the list holding it is on screen (DESIGN.md
    /// section 4). Closing steps the cursor on, so without this the key
    /// that takes a close back leaves the cursor on the row after it.
    fn follow_the_undo(&mut self, task: Id) {
        let Some(place) = self.model.live_task(task).map(|task| task.place()) else {
            return;
        };
        let list = match place {
            Place::Day(day) if day == self.showing => List::Day,
            Place::Day(_) => return,
            Place::Backlog => List::Backlog,
        };
        if !self.on_screen(list) {
            return;
        }
        self.set_cursor(list, RowId::Task(task));
        self.pane = match list {
            List::Day => Pane::Day,
            _ => Pane::Backlog,
        };
    }

    /// Whether a list is drawn now. The review takes the whole window, a
    /// narrow one draws only the pane the keyboard is on, and the backlog
    /// is only ever beside today (DESIGN.md sections 5 and 6).
    fn on_screen(&self, list: List) -> bool {
        if self.review.is_some() {
            return list == List::Review;
        }
        if self.layout.narrow {
            return list == self.focused();
        }
        match self.page {
            Page::Home => match list {
                List::Day => true,
                List::Backlog => !self.browsing(),
                List::Days => self.browsing(),
                List::Notes | List::Review | List::Settings => false,
            },
            Page::Notes => list == List::Notes,
            Page::Settings => list == List::Settings,
        }
    }

    fn say(&mut self, text: impl Into<String>, undo: bool) {
        self.message = Some(Message {
            text: text.into(),
            undo,
            said_at: self.now(),
        });
    }

    /// The hint bar goes back to its keys once a message nobody has typed
    /// past has stood for `message_seconds`, so that a pause to read them
    /// is never a pause in front of the wrong line. At 0 no tick ever
    /// takes it away and only the next key does.
    fn forget_an_old_message(&mut self) {
        let seconds = i64::from(self.model.settings.message_seconds());
        if seconds == 0 {
            return;
        }
        let now = self.now();
        let stands = Span::new().seconds(seconds);
        let old = self.message.as_ref().is_some_and(|message| {
            message
                .said_at
                .checked_add(stands)
                .is_ok_and(|until| now >= until)
        });
        if old {
            self.message = None;
        }
    }

    // ---- the rows ----------------------------------------------------

    /// Every row of a list in the order they are drawn, which is the
    /// order the cursor moves in, each with the group it is in. The
    /// groups and their order are the domain's (DOMAIN.md sections 6
    /// and 7).
    fn rows_of(&self, list: List) -> Vec<(RowId, Group)> {
        let tasks = |rows: &[domain::Row], group| {
            rows.iter()
                .map(|row| (RowId::Task(row.task), group))
                .collect::<Vec<_>>()
        };
        match list {
            List::Day => {
                let day = &self.views.day;
                [
                    tasks(&day.focus, Group::Focus),
                    tasks(&day.plan, Group::Plan),
                    tasks(&day.done, Group::Done),
                    tasks(&day.moved, Group::Moved),
                ]
                .concat()
            }
            List::Backlog => {
                let backlog = &self.views.backlog;
                [
                    tasks(&backlog.ordinary, Group::Ordinary),
                    tasks(&backlog.waiting, Group::Waiting),
                    backlog
                        .schedules
                        .iter()
                        .map(|row| (RowId::Schedule(row.schedule), Group::Schedules))
                        .collect(),
                ]
                .concat()
            }
            List::Days => self
                .views
                .days
                .stretches
                .iter()
                .flat_map(|stretch| &stretch.days)
                .map(|row| (RowId::Day(row.day), Group::Days))
                .collect(),
            List::Notes => self
                .views
                .notes
                .rows
                .iter()
                .map(|row| (RowId::Note(row.note), Group::Notes))
                .collect(),
            List::Review => self
                .review
                .iter()
                .flat_map(|review| review.rows())
                .map(|task| (RowId::Task(task), Group::Review))
                .collect(),
            List::Settings => setting_rows()
                .iter()
                .map(|(_, row)| (RowId::Setting(*row), Group::Settings))
                .collect(),
        }
    }

    fn group_of(&self, list: List, id: RowId) -> Option<Group> {
        self.rows_of(list)
            .into_iter()
            .find(|(row, _)| *row == id)
            .map(|(_, group)| group)
    }

    /// The task a key on the cursor row acts on, or nothing and a reason.
    fn task_at_cursor(&mut self) -> Option<Id> {
        if self.page == Page::Notes {
            self.say("That key is for tasks, and this page is notes.", false);
            return None;
        }
        let list = self.focused();
        let Some(id) = self.cursor(list) else {
            self.say("There is no task here yet.", false);
            return None;
        };
        // The cursor answers with the first row when the row it was left
        // on is not in the list, and a reorder changes which row that is.
        // Acting on a row settles the cursor there, so the answer cannot
        // move under the key that asked for it (ARCHITECTURE.md rule 6).
        self.set_cursor(list, id);
        if self.group_of(list, id) == Some(Group::Moved) {
            self.say("That row only points at the task; it has moved.", false);
            return None;
        }
        let Some(task) = id.task() else {
            self.say(
                match id {
                    RowId::Day(_) => "That row is a day, not a task.",
                    _ => "That row is a repeat schedule, not a task.",
                },
                false,
            );
            return None;
        };
        Some(task)
    }

    /// The row the cursor lands on when the one it is on leaves the list:
    /// the next of its group, then the one before it, then whatever is
    /// nearest.
    fn neighbour_of(&self, list: List, id: RowId) -> Option<RowId> {
        let rows = self.rows_of(list);
        let at = rows.iter().position(|(row, _)| *row == id)?;
        let group = rows[at].1;
        let same = |(row, other): &&(RowId, Group)| *other == group && *row != id;

        rows[at + 1..]
            .iter()
            .find(same)
            .or_else(|| rows[..at].iter().rev().find(same))
            .or_else(|| rows[at + 1..].first())
            .or_else(|| rows[..at].last())
            .map(|(row, _)| *row)
    }

    // ---- the keys on a row -------------------------------------------

    /// `space`: close an open task, reopen a closed one. Closing a task
    /// in the backlog puts it on today first, because that is where it
    /// was done.
    fn close_or_reopen(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let closed = self.model.live_task(id).is_some_and(|task| !task.is_open());
        let list = self.focused();
        // A reopened task stays in the list, so the cursor stays on it.
        let next = if closed {
            None
        } else {
            self.neighbour_of(list, RowId::Task(id))
        };

        let command = if closed {
            Command::Reopen { task: id }
        } else {
            Command::Close { task: id }
        };
        if self.run(command).is_none() {
            return;
        }
        if closed {
            self.take_the_decision_back(id);
        } else {
            self.step_on(list, next, id, Decided::Done);
        }
    }

    /// `f`: one of the day's must-dos, or not.
    fn turn_focus_over(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let Some(task) = self.model.live_task(id) else {
            return;
        };
        let focus = !task.focus;
        self.run(Command::SetFocus { task: id, focus });
    }

    /// `w`: blocked on someone or something else. Waiting is a backlog
    /// state, so on a day task the domain moves the task to the backlog
    /// with the flag, and the cursor steps on as it does for any move
    /// (DOMAIN.md section 9).
    fn wait_on_someone(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let Some(task) = self.model.live_task(id) else {
            return;
        };
        let waiting = !task.waiting;
        let leaves = waiting && task.day.is_some();
        let list = self.focused();
        let next = leaves
            .then(|| self.neighbour_of(list, RowId::Task(id)))
            .flatten();

        if self
            .run(Command::SetWaiting { task: id, waiting })
            .is_none()
        {
            return;
        }
        if waiting {
            self.step_on(list, next, id, Decided::Waiting);
        } else {
            self.take_the_decision_back(id);
        }
    }

    /// `x`: no confirm, and `u` in the hint bar until the next key —
    /// unless `confirm_delete` is on, when the question comes first
    /// (DESIGN.md section 8).
    fn delete(&mut self) {
        // A note is left before it can be the row thrown away, so that
        // what was typed into it is written first (ARCHITECTURE.md rule
        // 8) and the keyboard is back on the list to answer.
        if self.page == Page::Notes {
            self.leave_the_note();
        }
        let Some(row) = self.row_to_delete() else {
            return;
        };
        if self.model.settings.confirm_delete() {
            self.open(PopupKind::DeleteQuestion, Some(row));
            return;
        }
        self.throw_the_row_away(row);
    }

    /// The row `x` is about: a note on the notes page, a task everywhere
    /// else, each with the reason when there is none.
    fn row_to_delete(&mut self) -> Option<RowId> {
        match self.page {
            Page::Notes => self.note_at_cursor().map(RowId::Note),
            Page::Home => self.task_at_cursor().map(RowId::Task),
            // A setting is not a row that can go: `x` is not a key there.
            Page::Settings => None,
        }
    }

    /// The delete itself, once there is nothing left to ask. A note goes
    /// the way a task does, and the cursor lands on the row that follows.
    fn throw_the_row_away(&mut self, row: RowId) {
        match row {
            RowId::Task(id) => {
                let list = self.focused();
                let next = self.neighbour_of(list, RowId::Task(id));
                if self.run(Command::DeleteTask { task: id }).is_some() {
                    self.step_on(list, next, id, Decided::Deleted);
                }
            }
            RowId::Note(id) => {
                let next = self.neighbour_of(List::Notes, RowId::Note(id));
                if self.run(Command::DeleteNote { note: id }).is_some()
                    && let Some(next) = next
                {
                    self.set_cursor(List::Notes, next);
                }
            }
            RowId::Schedule(_) | RowId::Day(_) | RowId::Setting(_) => {}
        }
    }

    /// Enter on the delete question: the row it captured when it opened,
    /// by its id, so that a reload cannot turn it into another one.
    fn take_the_delete(&mut self) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        let Some(row) = popup.target else {
            return;
        };
        self.throw_the_row_away(row);
    }

    /// `J` and `K`: swap with the neighbour in the same group, because
    /// what the screen shows as two groups is one sequence of positions
    /// underneath (DOMAIN.md section 4).
    fn reorder(&mut self, down: bool) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let list = self.focused();
        let Some(group) = self.group_of(list, RowId::Task(id)) else {
            return;
        };
        if !group.is_ordered_by_hand() {
            self.say("Those rows keep the order they are in.", false);
            return;
        }

        let siblings: Vec<Id> = self
            .rows_of(list)
            .into_iter()
            .filter(|(_, other)| *other == group)
            .filter_map(|(row, _)| row.task())
            .collect();
        let Some(at) = siblings.iter().position(|row| *row == id) else {
            return;
        };
        let swap = if down {
            Some(at + 1)
        } else {
            at.checked_sub(1)
        };
        let Some(other) = swap.and_then(|swap| siblings.get(swap)) else {
            return;
        };
        let Some(position) = self.model.live_task(*other).map(|task| task.position) else {
            return;
        };
        self.reorder_to(id, position);
    }

    /// One step of a reorder, wherever it came from. The row is marked
    /// "moving" until the next key, which is the only sign the screen has
    /// that a row is being carried rather than just selected.
    fn reorder_to(&mut self, task: Id, position: usize) {
        if self.run(Command::Reorder { task, position }).is_some() {
            self.moving = Some(task);
        }
    }

    /// `t`: the cursor row onto today, or, in search, the result the
    /// cursor is on as a new task there.
    fn pull_onto_today(&mut self) {
        if self
            .popup
            .as_ref()
            .is_some_and(|open| open.kind == PopupKind::Search)
        {
            self.readd_from_search();
            return;
        }
        self.move_it(MoveTarget::Day(self.today));
    }

    /// `t`, `b`, and the days of the move card. With the card open the
    /// task is the one the card was opened on; otherwise it is the row
    /// the cursor is on.
    fn move_it(&mut self, target: MoveTarget) {
        if target == MoveTarget::Pick {
            self.pick_a_date();
            return;
        }
        let carded = self.take_the_card();
        let Some(id) = carded.or_else(|| self.task_at_cursor()) else {
            return;
        };
        let place = match target {
            MoveTarget::Day(day) => Place::Day(day),
            MoveTarget::Backlog => Place::Backlog,
            MoveTarget::Pick => return,
        };
        self.move_task(id, place);
    }

    /// A task to a place, wherever the key came from. The row leaves its
    /// group, so the cursor steps to the next one (DESIGN.md section 4).
    fn move_task(&mut self, id: Id, place: Place) {
        let list = self.focused();
        let next = self.neighbour_of(list, RowId::Task(id));
        if self.run(Command::Move { task: id, place }).is_some() {
            self.step_on(list, next, id, Decided::Moved(place));
        }
    }

    /// Closes the move card and answers the task it was opened on.
    fn take_the_card(&mut self) -> Option<Id> {
        let open = self.popup.as_ref()?;
        if open.kind != PopupKind::Move {
            return None;
        }
        let target = open.task();
        self.popup = None;
        target
    }

    fn open_the_move_card(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        self.open(PopupKind::Move, Some(RowId::Task(id)));
    }

    /// The day a quick pick on either card means. "Next work day" and
    /// "next Monday" are the rules' own definitions of those days
    /// (DOMAIN.md section 10); the rest are arithmetic.
    fn day_for(&self, action: Action) -> Option<Date> {
        let work_days = self.model.settings.work_days();
        let next = |rule: Rule| {
            domain::next_dates(&rule, self.today, 1, &work_days)
                .first()
                .copied()
        };
        match action {
            Action::ToToday => Some(self.today),
            Action::Tomorrow => self.today.tomorrow().ok(),
            Action::NextWorkDay => next(Rule::Workdays),
            Action::NextMonday => next(Rule::Weekly {
                weekdays: vec![Weekday::Mon],
            }),
            Action::InAWeek => self.today.checked_add(Span::new().days(7)).ok(),
            Action::EndOfMonth => Some(self.today.last_of_month()),
            _ => None,
        }
    }

    /// The day each row of the move card sends a task to.
    fn target_for(&self, action: Action) -> MoveTarget {
        // A day the calendar cannot reach, which is only ever the last
        // day it has, is offered as the date card rather than as some
        // other day the row does not name.
        match action {
            Action::ToBacklog => MoveTarget::Backlog,
            _ => self
                .day_for(action)
                .map_or(MoveTarget::Pick, MoveTarget::Day),
        }
    }

    /// A quick pick, on whichever card is open: a day to move a task to,
    /// or a day to write on it.
    fn quick_pick(&mut self, action: Action) {
        if self.popup.as_ref().and_then(Popup::date).is_some() {
            if let Some(date) = self.day_for(action) {
                self.take_the_date(Some(date));
            }
            return;
        }
        self.move_it(self.target_for(action));
    }

    /// The rows of the move card: the keys and names from the key table,
    /// and the day each one means.
    pub fn move_choices(&self) -> Vec<MoveChoice> {
        input::bindings(KeyContext::Popup {
            kind: PopupKind::Move,
            text_field: false,
        })
        .iter()
        .filter_map(|binding| {
            let (key, action) = *binding.keys.first()?;
            let target = match action {
                Action::ToToday
                | Action::Tomorrow
                | Action::NextWorkDay
                | Action::NextMonday
                | Action::ToBacklog => self.target_for(action),
                Action::GoToDate => MoveTarget::Pick,
                _ => return None,
            };
            Some(MoveChoice {
                key,
                label: binding.label,
                target,
            })
        })
        .collect()
    }

    // ---- the days ----------------------------------------------------

    /// `[` and `]`: the day pane one day back or on. Nothing else moves,
    /// so the keyboard stays in the pane it was in and the pane beside
    /// the day becomes the list of days.
    fn step_the_day(&mut self, days: i64) {
        if let Ok(day) = self.showing.checked_add(Span::new().days(days)) {
            self.show_the_day(day);
        }
    }

    /// The day pane on a day, wherever the key came from: `.`, the day
    /// list, the go-to card, or a moved row being followed.
    fn show_the_day(&mut self, day: Date) {
        self.showing = day;
        self.refresh();
    }

    /// `⏎` on the home page: the day a row of the day list is, or the
    /// place a moved row points at. Everywhere else it is the field
    /// under the cursor, and there is no field here.
    fn follow_the_row(&mut self) {
        let list = self.focused();
        let Some(id) = self.cursor(list) else {
            return;
        };
        if let Some(day) = id.day() {
            self.show_the_day(day);
            // Going to a day means looking at it, so the keyboard goes
            // to the pane the day is drawn in.
            self.pane = Pane::Day;
            return;
        }
        if self.group_of(list, id) != Some(Group::Moved) {
            return;
        }
        if let Some(task) = id.task() {
            self.follow_the_task(task);
        }
    }

    /// Wherever a task is now, with the cursor on it. A moved row is a
    /// pointer and a search result is another (DESIGN.md section 6).
    fn follow_the_task(&mut self, task: Id) {
        let Some(place) = self.model.live_task(task).map(|task| task.place()) else {
            self.say("That task is gone.", false);
            return;
        };
        self.page = Page::Home;
        match place {
            Place::Day(day) => {
                self.show_the_day(day);
                self.pane = Pane::Day;
                self.set_cursor(List::Day, RowId::Task(task));
            }
            Place::Backlog => {
                // The backlog is only beside today, so following a task
                // into it comes back to today.
                self.show_the_day(self.today);
                self.pane = Pane::Backlog;
                self.set_cursor(List::Backlog, RowId::Task(task));
            }
        }
    }

    // ---- the date card -----------------------------------------------

    /// `d` and `r`: the card that puts a date on a backlog task. Pressed
    /// again inside the card they switch which date is being set, keeping
    /// the day and the text, because it is one card with two modes
    /// (wireframe 06).
    fn open_the_date_card(&mut self, kind: DateKind) {
        if let Some(popup) = &mut self.popup {
            if let Card::Date(draft) = &mut popup.card {
                // Only a date the task carries has two modes; the day a
                // move or a step goes to has none.
                if matches!(draft.kind, DateKind::Due | DateKind::Remind) {
                    draft.kind = kind;
                }
            }
            return;
        }
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let Some(task) = self.model.live_task(id) else {
            return;
        };
        // The card opens on the date the task already has, so a date is
        // adjusted rather than looked up again.
        let on = match kind {
            DateKind::Due => task.due_on,
            DateKind::Remind => task.remind_on,
            DateKind::Move | DateKind::Go => task.day,
        }
        .unwrap_or(self.today);
        self.open_card(PopupKind::Date, Some(RowId::Task(id)), kind, on);
    }

    /// `g`: on the move card, the calendar for the day it could not
    /// name. On the page itself, the day the day pane goes to.
    fn pick_a_date(&mut self) {
        let Some(task) = self.take_the_card() else {
            self.open_card(PopupKind::Date, None, DateKind::Go, self.showing);
            return;
        };
        let on = self
            .model
            .live_task(task)
            .and_then(|task| task.day)
            .unwrap_or(self.today);
        self.open_card(PopupKind::Date, Some(RowId::Task(task)), DateKind::Move, on);
    }

    fn open_card(&mut self, kind: PopupKind, target: Option<RowId>, date: DateKind, on: Date) {
        self.open(kind, target);
        if let Some(popup) = &mut self.popup {
            popup.card = Card::Date(DateDraft {
                kind: date,
                on,
                in_calendar: false,
            });
        }
    }

    /// The rows of the date card: the keys and names from the key table,
    /// and the day each one means. The move card's date has no "clear":
    /// a task with no day is in the backlog, which is a place, not a
    /// missing date.
    pub fn date_choices(&self) -> Vec<DateChoice> {
        let clears = self
            .popup
            .as_ref()
            .and_then(Popup::date)
            .is_some_and(|draft| matches!(draft.kind, DateKind::Due | DateKind::Remind));
        input::bindings(KeyContext::Popup {
            kind: PopupKind::Date,
            text_field: true,
        })
        .iter()
        .filter_map(|binding| {
            let (key, action) = *binding.keys.first()?;
            let date = match action {
                Action::Tomorrow | Action::NextMonday | Action::InAWeek | Action::EndOfMonth => {
                    Some(self.day_for(action)?)
                }
                Action::ClearDate if clears => None,
                _ => return None,
            };
            Some(DateChoice {
                key,
                label: binding.label,
                date,
            })
        })
        .collect()
    }

    /// Enter on the date card. What was typed has already moved the day
    /// the card is on, so the day is the answer; text the domain cannot
    /// read is refused rather than turned into some other day.
    fn take_the_typed_date(&mut self) {
        let Some(popup) = &self.popup else {
            return;
        };
        let typed = popup.text.trim().to_owned();
        let Some(draft) = popup.date().copied() else {
            return;
        };
        if !typed.is_empty() && domain::parse_date(&typed, self.today).is_none() {
            self.say("That is not a date I can read.", false);
            return;
        }
        self.take_the_date(Some(draft.on));
    }

    /// The day the card ended on: a date the task carries, or the day it
    /// is moved to. Either way the card has said its piece and closes.
    fn take_the_date(&mut self, date: Option<Date>) {
        let Some(draft) = self.popup.as_ref().and_then(Popup::date).copied() else {
            return;
        };
        // The card that goes to a day is about no task, so it answers
        // before the task is looked for.
        if draft.kind == DateKind::Go {
            self.popup = None;
            if let Some(day) = date {
                self.show_the_day(day);
            }
            return;
        }
        let Some(task) = self.popup.as_ref().and_then(Popup::task) else {
            return;
        };
        // Clearing is a date the task carries; the move card's day is a
        // place, and there is no such thing as moving a task to no day.
        if draft.kind == DateKind::Move && date.is_none() {
            return;
        }
        self.popup = None;
        let list = self.focused();
        match draft.kind {
            DateKind::Due => {
                if self.run(Command::SetDue { task, date }).is_some() {
                    self.step_on(list, None, task, Decided::Dated);
                }
            }
            DateKind::Remind => {
                if self.run(Command::SetRemind { task, date }).is_some() {
                    self.step_on(list, None, task, Decided::Dated);
                }
            }
            DateKind::Move => {
                if let Some(day) = date {
                    self.move_task(task, Place::Day(day));
                }
            }
            DateKind::Go => {}
        }
    }

    /// The calendar keys. Says whether the card took the key, because the
    /// same four actions move a cursor everywhere else.
    fn walk_the_calendar(&mut self, span: Span) -> bool {
        let Some(popup) = &mut self.popup else {
            return false;
        };
        let Card::Date(draft) = &mut popup.card else {
            return false;
        };
        if !draft.in_calendar {
            return false;
        }
        if let Ok(date) = draft.on.checked_add(span) {
            draft.on = date;
        }
        // The field and the calendar are two ways to the same day, so
        // what was typed goes when the calendar is used.
        popup.text.clear();
        popup.caret = 0;
        true
    }

    /// `tab`: the card's other control if a card is open, and the next
    /// pane or tab otherwise.
    fn next_control(&mut self) {
        if let Some(popup) = &mut self.popup
            && let Card::Date(draft) = &mut popup.card
        {
            draft.in_calendar = !draft.in_calendar;
            return;
        }
        self.shift_pane(true, true);
    }

    // ---- the repeat card ---------------------------------------------

    /// `R`: the schedule behind a copy, the schedule a row of the backlog
    /// list is, or a new one for an ordinary task. The card opens on what
    /// the schedule already is, so a rule is adjusted rather than
    /// described again.
    fn open_the_repeat_card(&mut self) {
        let list = self.focused();
        let on_a_schedule = self
            .cursor(list)
            .filter(|_| self.page == Page::Home)
            .and_then(RowId::schedule);
        let target = match on_a_schedule {
            Some(schedule) => RowId::Schedule(schedule),
            None => match self.task_at_cursor() {
                Some(task) => RowId::Task(task),
                None => return,
            },
        };

        // Where "every N weeks" counts from and where the preview picks
        // up: the schedule's own dates if there is one, else the day the
        // task is on.
        let anchor = match target {
            RowId::Task(task) => self
                .model
                .live_task(task)
                .and_then(|task| task.day)
                .unwrap_or(self.today),
            _ => self.today,
        };
        let schedule = self
            .schedule_of(target)
            .and_then(|id| self.model.schedule(id));
        let rule = schedule.map(|schedule| schedule.rule.clone());
        let after = schedule.map_or(anchor, |schedule| schedule.generated_through);

        let draft = RepeatDraft {
            weekdays: match &rule {
                Some(Rule::Weekly { weekdays }) => weekdays.clone(),
                _ => vec![Weekday::of(anchor)],
            },
            weekday: Weekday::week(self.model.settings.week_starts_on())
                .iter()
                .position(|day| *day == Weekday::of(anchor))
                .unwrap_or(0),
            month_day: match &rule {
                Some(Rule::Monthly { day }) => *day,
                _ => MonthDay::Day(anchor.day() as u8),
            },
            weeks: match &rule {
                Some(Rule::EveryNWeeks { n, .. }) => *n,
                _ => 2,
            },
            from: match &rule {
                Some(Rule::EveryNWeeks { from, .. }) => *from,
                _ => anchor,
            },
            after,
        };
        let selected = rule.as_ref().and_then(shape_of).unwrap_or(0);

        self.open(PopupKind::Repeat, Some(target));
        if let Some(popup) = &mut self.popup {
            popup.card = Card::Repeat(draft);
            popup.selected = selected;
        }
    }

    /// The schedule a row is, or the one behind the copy it is.
    fn schedule_of(&self, row: RowId) -> Option<Id> {
        match row {
            RowId::Schedule(id) => Some(id),
            RowId::Task(id) => self.model.live_task(id).and_then(|task| task.schedule_id),
            RowId::Note(_) | RowId::Day(_) | RowId::Setting(_) => None,
        }
    }

    /// A digit on the repeat card: the shape it names.
    fn choose_the_shape(&mut self, action: Action) {
        let Some(at) = repeat_shapes().iter().position(|shape| *shape == action) else {
            return;
        };
        if let Some(popup) = &mut self.popup
            && popup.kind == PopupKind::Repeat
        {
            popup.selected = at;
        }
    }

    /// `h` and `l`: whatever the selected shape has to adjust. Says
    /// whether the card took the key.
    fn adjust(&mut self, forward: bool) -> bool {
        let Some(popup) = &mut self.popup else {
            return false;
        };
        let shape = repeat_shapes().get(popup.selected).copied();
        let Card::Repeat(draft) = &mut popup.card else {
            return false;
        };
        let step = |at: usize, last: usize| {
            if forward {
                (at + 1).min(last)
            } else {
                at.saturating_sub(1)
            }
        };
        match shape {
            Some(Action::EveryWeek) => draft.weekday = step(draft.weekday, 6),
            Some(Action::EveryMonth) => {
                // The days of a month and then "last", as one row of
                // choices (DOMAIN.md section 10).
                let at = match draft.month_day {
                    MonthDay::Day(day) => day.clamp(1, 31) as usize - 1,
                    MonthDay::Last => 31,
                };
                let at = step(at, 31);
                draft.month_day = match at {
                    31 => MonthDay::Last,
                    at => MonthDay::Day(at as u8 + 1),
                };
            }
            Some(Action::EveryFewWeeks) => {
                draft.weeks = step(draft.weeks as usize - 1, WEEKS_APART - 1) as u32 + 1;
            }
            _ => {}
        }
        true
    }

    /// `space`: the setting under the cursor on the settings page, and
    /// the weekday under it on the repeat card.
    fn pick(&mut self) {
        if self.page == Page::Settings && self.popup.is_none() {
            self.change_the_setting();
            return;
        }
        self.pick_a_weekday();
    }

    /// `space` on the weekly shape: the highlighted weekday joins the
    /// set, or leaves it.
    fn pick_a_weekday(&mut self) {
        let week = Weekday::week(self.model.settings.week_starts_on());
        let Some(popup) = &mut self.popup else {
            return;
        };
        if repeat_shapes().get(popup.selected) != Some(&Action::EveryWeek) {
            return;
        }
        let Card::Repeat(draft) = &mut popup.card else {
            return;
        };
        let day = week[draft.weekday.min(6)];
        match draft.weekdays.iter().position(|other| *other == day) {
            Some(at) => {
                draft.weekdays.remove(at);
            }
            None => draft.weekdays.push(day),
        }
        draft.weekdays.sort();
    }

    /// The rule the card is describing, or nothing on the row that ends
    /// the schedule instead of changing it.
    fn drafted_rule(&self) -> Option<Rule> {
        let popup = self.popup.as_ref()?;
        let draft = popup.repeat()?;
        match repeat_shapes().get(popup.selected)? {
            Action::EveryWorkDay => Some(Rule::Workdays),
            Action::EveryDay => Some(Rule::Daily),
            Action::EveryWeek => Some(Rule::Weekly {
                weekdays: draft.weekdays.clone(),
            }),
            Action::EveryMonth => Some(Rule::Monthly {
                day: draft.month_day,
            }),
            Action::EveryFewWeeks => Some(Rule::EveryNWeeks {
                n: draft.weeks,
                from: draft.from,
            }),
            _ => None,
        }
    }

    /// The next few dates the drafted rule falls on, so that "the 1st"
    /// and "every 2 weeks" are unambiguous before they are saved
    /// (DOMAIN.md section 10).
    pub fn repeat_preview(&self) -> Vec<Date> {
        let Some(after) = self.popup.as_ref().and_then(Popup::repeat) else {
            return Vec::new();
        };
        match self.drafted_rule() {
            Some(rule) => domain::next_dates(
                &rule,
                after.after.max(self.today),
                PREVIEW,
                &self.model.settings.work_days(),
            ),
            None => Vec::new(),
        }
    }

    /// Enter on the repeat card: the rule of the schedule behind the row,
    /// a schedule where there was none, or the end of one.
    fn take_the_rule(&mut self) {
        let Some(target) = self.popup.as_ref().and_then(|popup| popup.target) else {
            return;
        };
        let schedule = self.schedule_of(target);
        let command = match (schedule, self.drafted_rule()) {
            (Some(schedule), Some(rule)) => Command::SetRule { schedule, rule },
            (Some(schedule), None) => Command::StopSchedule { schedule },
            (None, Some(rule)) => match target.task() {
                Some(task) => Command::CreateSchedule { task, rule },
                None => return,
            },
            (None, None) => {
                self.say("That task does not repeat.", false);
                return;
            }
        };
        // A stopped schedule leaves the backlog's list, so the cursor
        // steps on as it does for any row that goes.
        let list = self.focused();
        let stopping = matches!(command, Command::StopSchedule { .. });
        let next = stopping.then(|| self.neighbour_of(list, target)).flatten();
        // A rule the domain refuses leaves the card open with the reason
        // in the hint bar, so the shape can be changed and saved again.
        if self.run(command).is_some() {
            self.popup = None;
            if let Some(next) = next {
                self.set_cursor(list, next);
            }
        }
    }

    // ---- the settings page -------------------------------------------

    /// `,`: the settings, and `,` again the page they were opened from.
    /// The page is not a popup over another one, so what was on screen
    /// is put away first (DESIGN.md section 11).
    fn turn_to_the_settings(&mut self) {
        if self.page == Page::Settings {
            self.leave_the_settings();
            return;
        }
        self.leave_the_note();
        self.editor = None;
        self.came_from = self.page;
        self.page = Page::Settings;
    }

    /// `,` or `esc` on the page: back where it was opened from, with
    /// anything half typed on a row dropped.
    fn leave_the_settings(&mut self) {
        self.setting_draft = None;
        self.page = self.came_from;
    }

    /// The setting a key acts on, which is the cursor row settled where
    /// it is, for the reason a task is (ARCHITECTURE.md rule 6).
    fn setting_at_cursor(&mut self) -> Option<SettingRow> {
        let id = self.cursor(List::Settings)?;
        self.set_cursor(List::Settings, id);
        id.setting()
    }

    /// `h` and `l`: the row's value one step down or up. Says whether
    /// the page took the key, because the same two actions move a caret
    /// everywhere else.
    fn adjust_a_setting(&mut self, forward: bool) -> bool {
        if self.page != Page::Settings || self.popup.is_some() || self.setting_draft.is_some() {
            return false;
        }
        let Some(row) = self.setting_at_cursor() else {
            return true;
        };
        let settings = stepped(self.settings(), row, forward);
        self.change_settings(settings);
        true
    }

    /// `space` and Enter: the row's next value, or, on a row that holds
    /// a number or a size, the field it is typed into.
    fn change_the_setting(&mut self) {
        let Some(row) = self.setting_at_cursor() else {
            return;
        };
        if row.is_typed() {
            let text = typed_value(self.settings(), row);
            self.setting_draft = Some(SettingDraft {
                row,
                caret: glyphs(&text),
                text,
            });
            return;
        }
        let settings = cycled(self.settings(), row);
        self.change_settings(settings);
    }

    /// Enter in the field: the number or the size that was typed, or the
    /// field left open with the reason in the hint bar, the way the date
    /// card leaves a line it cannot read.
    fn take_the_typed_setting(&mut self) {
        let Some(draft) = &self.setting_draft else {
            return;
        };
        let (row, typed) = (draft.row, draft.text.trim().to_owned());
        let Some(settings) = typed_into(self.settings(), row, &typed) else {
            self.say(
                match row {
                    SettingRow::WindowSize => "That is not a size I can read; write it as 870x650.",
                    _ => "That is not a number I can read.",
                },
                false,
            );
            return;
        };
        self.setting_draft = None;
        self.change_settings(settings);
    }

    /// The value being typed on a settings row, while one is.
    pub fn setting_draft(&self) -> Option<&SettingDraft> {
        self.setting_draft.as_ref()
    }

    // ---- the notes page ----------------------------------------------

    /// Copy the live draft without moving its caret or waiting for a save.
    fn copy_note(&mut self) -> Flow {
        if self.page != Page::Notes {
            return Flow::Continue;
        }
        if let Some(draft) = &self.draft {
            return Flow::CopyNote(draft.text.clone());
        }
        let Some(note) = self.note_at_cursor() else {
            return Flow::Continue;
        };
        self.model
            .note(note)
            .map_or(Flow::Continue, |note| Flow::CopyNote(note.body.clone()))
    }

    /// Clipboard failures stay in the app, just like storage failures.
    pub fn copied_note(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => self.say("Note copied", false),
            Err(message) => self.say(message, false),
        }
    }

    /// The note a key on the cursor row acts on, or nothing and a reason.
    fn note_at_cursor(&mut self) -> Option<Id> {
        let Some(id) = self.cursor(List::Notes).and_then(RowId::note) else {
            self.say("There is no note here yet.", false);
            return None;
        };
        // Settled for the same reason a task is.
        self.set_cursor(List::Notes, RowId::Note(id));
        Some(id)
    }

    /// `a` on the notes page: an empty note at the top of the list, open
    /// and ready to be typed into, because there is nothing else to do
    /// with an empty note.
    fn new_note(&mut self) {
        self.leave_the_note();
        if let Some(change) = self.run(Command::CreateNote)
            && let Some(note) = added_note(&change)
        {
            self.set_cursor(List::Notes, RowId::Note(note));
            self.open_the_note();
        }
    }

    /// Enter on the list, and `tab` off it: the note under the cursor
    /// takes the keyboard, with the caret at the end of what is there.
    fn open_the_note(&mut self) {
        let Some(note) = self.note_at_cursor() else {
            return;
        };
        let Some(body) = self.model.note(note).map(|note| note.body.clone()) else {
            return;
        };
        self.draft = Some(Draft {
            note,
            caret: glyphs(&body),
            text: body,
        });
        self.notes_pane = NotesPane::Note;
    }

    /// Leaving the note: what was typed is written and the keyboard goes
    /// back to the list. A note is left by `esc`, by `tab`, by the page
    /// turning, and by a click anywhere else.
    fn leave_the_note(&mut self) {
        if self.draft.is_none() {
            return;
        }
        self.save_the_note();
        self.draft = None;
        self.notes_pane = NotesPane::List;
    }

    /// The note body as one command, when it differs from the row. This is
    /// the moment ARCHITECTURE.md rule 8 leaves to this phase: the first
    /// tick after a keystroke, and every leaving of the note.
    fn save_the_note(&mut self) {
        let Some(draft) = &self.draft else {
            return;
        };
        let (note, body) = (draft.note, draft.text.clone());
        let Some(held) = self.model.note(note).filter(|note| note.is_live()) else {
            // Another window threw it away while it was open. There is
            // nothing left to write it to.
            self.draft = None;
            self.notes_pane = NotesPane::List;
            return;
        };
        if held.body == body {
            return;
        }
        self.run(Command::EditNote { note, body });
    }

    /// The words the open note is drawn with underlined, worked out
    /// after every action so that drawing has only to read them.
    ///
    /// The body a note is checked from is the one it is drawn from: what
    /// is being typed while the keyboard is in it, and the note as it
    /// was last saved otherwise. A note only being looked at shows every
    /// word found in it; one being typed into keeps the word the caret
    /// is in to itself until the caret has left it.
    fn check_the_spelling(&mut self) {
        if self.page != Page::Notes || !self.model.settings.spell_check_notes() {
            self.spelling.forget();
            return;
        }
        let Some(note) = self.cursor(List::Notes).and_then(RowId::note) else {
            self.spelling.forget();
            return;
        };
        let draft = self.draft.as_ref().filter(|draft| draft.note == note);
        let caret = draft.map(|draft| draft.caret);
        let body = match draft {
            Some(draft) => Some(draft.text.as_str()),
            None => self.model.note(note).map(|note| note.body.as_str()),
        };
        match body {
            Some(body) => self.spelling.of(note, body, caret),
            // The note the cursor names is gone, which the next reload
            // moves the cursor off.
            None => self.spelling.forget(),
        }
    }

    // ---- the title being typed ---------------------------------------

    /// `a`: a new note on the notes page, and a field at the end of the
    /// list on the home page.
    fn add(&mut self) {
        match self.page {
            Page::Notes => self.new_note(),
            Page::Home => self.start_adding(),
            // The settings page binds no key that adds anything: its
            // rows are the settings there are.
            Page::Settings => {}
        }
    }

    /// A field at the end of the list, which Enter empties and keeps open,
    /// so a list of tasks is typed in one go.
    fn start_adding(&mut self) {
        self.editor = Some(Editor {
            field: Field::Adding,
            list: self.focused(),
            task: None,
            text: String::new(),
            caret: 0,
        });
    }

    /// `e`: the row itself becomes the field. Editing is always in place
    /// (DESIGN.md section 8).
    fn start_renaming(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let Some(task) = self.model.live_task(id) else {
            return;
        };
        let text = task.title.clone();
        self.editor = Some(Editor {
            field: Field::Renaming,
            list: self.focused(),
            task: Some(id),
            caret: glyphs(&text),
            text,
        });
    }

    /// Enter in the field: one command, and then either the field again
    /// or the question a recurring copy asks.
    fn commit_the_title(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };
        let (field, list, task, title) = (
            editor.field,
            editor.list,
            editor.task,
            editor.text.trim().to_owned(),
        );

        match (field, task) {
            (Field::Adding, _) => {
                // An empty field is how quick add is stopped.
                if title.is_empty() {
                    self.editor = None;
                    return;
                }
                let place = self.place_of(list);
                if let Some(change) = self.run(Command::AddTask { title, place })
                    && let Some(added) = added_task(&change)
                {
                    self.set_cursor(list, RowId::Task(added));
                }
                if let Some(editor) = &mut self.editor {
                    editor.text.clear();
                    editor.caret = 0;
                }
            }
            (Field::Renaming, Some(id)) => {
                let copy = self
                    .model
                    .live_task(id)
                    .is_some_and(|task| task.schedule_id.is_some());
                if copy && !title.is_empty() {
                    // The one deliberate question (DESIGN.md section 8).
                    self.editor = None;
                    self.open(PopupKind::CopyQuestion, Some(RowId::Task(id)));
                    if let Some(popup) = &mut self.popup {
                        popup.text = title;
                    }
                    return;
                }
                if self.run(Command::EditTitle { task: id, title }).is_some() {
                    self.editor = None;
                }
            }
            (Field::Renaming, None) => self.editor = None,
        }
    }

    /// "This copy" renames the task; "this and future copies" renames the
    /// schedule with it (DOMAIN.md section 10).
    fn answer_the_question(&mut self, future: bool) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        let Some(task) = popup.task() else {
            return;
        };
        let title = popup.text;
        let command = if future {
            Command::EditTitleAndFuture {
                task,
                schedule_title: title.clone(),
                title,
            }
        } else {
            Command::EditTitle { task, title }
        };
        self.run(command);
    }

    fn place_of(&self, list: List) -> Place {
        match list {
            List::Backlog => Place::Backlog,
            // A task added while a past day is shown belongs to that
            // day, which is what its add line says (wireframe 08).
            List::Day => Place::Day(self.showing),
            List::Days | List::Notes | List::Review | List::Settings => Place::Day(self.today),
        }
    }

    // ---- what the keyboard is on ------------------------------------

    pub fn key_context(&self) -> KeyContext {
        match &self.popup {
            Some(popup) => KeyContext::Popup {
                kind: popup.kind,
                // The palette and search are typed into; the date card
                // is until `tab` moves the keyboard into its calendar;
                // the others are read and answered with a key.
                text_field: match popup.kind {
                    PopupKind::Palette | PopupKind::Search => true,
                    PopupKind::Date => popup.date().is_none_or(|draft| !draft.in_calendar),
                    _ => false,
                },
            },
            None => self.page_context(),
        }
    }

    /// The context of the page itself, which is what the palette lists the
    /// commands of even while it is over it.
    pub fn page_context(&self) -> KeyContext {
        // The review is a mode over the page rather than a page of its
        // own, and it is what the keyboard is on while it is up.
        if let Some(review) = &self.review {
            return KeyContext::Review {
                step: review.step,
                // A step of copies that only started this morning asks
                // nothing, and a key that would answer it acts on the
                // wrong thing (DESIGN.md section 5).
                asks: review.progress().1 > 0,
                text_field: self.editor.is_some(),
            };
        }
        match self.page {
            Page::Home => KeyContext::Home {
                pane: self.pane,
                day: self.shown(),
                field: self.editor.as_ref().map(|editor| editor.field),
            },
            Page::Notes => KeyContext::Notes {
                pane: self.notes_pane,
                // The note pane is a text field exactly while a note is
                // open in it.
                text_field: self.draft.is_some(),
            },
            Page::Settings => KeyContext::Settings {
                field: self.setting_draft.is_some(),
            },
        }
    }

    /// The list the cursor is in.
    pub fn focused(&self) -> List {
        if self.review.is_some() {
            return List::Review;
        }
        match (self.page, self.pane) {
            (Page::Home, Pane::Day) => List::Day,
            (Page::Home, Pane::Backlog) if self.browsing() => List::Days,
            (Page::Home, Pane::Backlog) => List::Backlog,
            (Page::Notes, _) => List::Notes,
            (Page::Settings, _) => List::Settings,
        }
    }

    /// Whether the day pane is on a day other than today, which is the
    /// whole of "history is being browsed".
    pub fn browsing(&self) -> bool {
        self.showing != self.today
    }

    // ---- reading the state, for `ui` --------------------------------

    /// The working day, which is what the status line calls today.
    pub fn today(&self) -> Date {
        self.today
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The day the day pane is on, which the status line names.
    pub fn showing(&self) -> Date {
        self.showing
    }

    /// Which side of today that day is on.
    pub fn shown(&self) -> Shown {
        match self.showing.cmp(&self.today) {
            std::cmp::Ordering::Less => Shown::Past,
            std::cmp::Ordering::Equal => Shown::Today,
            std::cmp::Ordering::Greater => Shown::Future,
        }
    }

    pub fn day(&self) -> &DayView {
        &self.views.day
    }

    /// The days that have something planned on them, which the pane
    /// beside a past day lists.
    pub fn days(&self) -> &DayList {
        &self.views.days
    }

    pub fn backlog(&self) -> &BacklogView {
        &self.views.backlog
    }

    pub fn notes(&self) -> &NotesView {
        &self.views.notes
    }

    /// The size of the pile: the "n in review" the status line counts,
    /// whatever was or was not decided (DOMAIN.md section 13).
    pub fn review_count(&self) -> usize {
        self.views.pile
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn pane(&self) -> Pane {
        self.pane
    }

    pub fn notes_pane(&self) -> NotesPane {
        self.notes_pane
    }

    pub fn popup(&self) -> Option<&Popup> {
        self.popup.as_ref()
    }

    /// The review, while it is on screen.
    pub fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }

    pub fn editor(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }

    /// The open note, while the keyboard is in it.
    pub fn draft(&self) -> Option<&Draft> {
        self.draft.as_ref()
    }

    /// The words of the open note to underline, in grapheme clusters
    /// from the start of its body, which is what the body is drawn and
    /// the caret counted in.
    ///
    /// They come in the order the body is drawn and no two of them lie
    /// over the same character, so a line being drawn walks them
    /// alongside its own text. The word the caret is in is not among
    /// them while the note is being typed into.
    pub fn misspellings(&self) -> &[Range<usize>] {
        &self.spelling.shown
    }

    /// What `u` would take back, for the palette to say beside the key.
    pub fn next_undo(&self) -> Option<&str> {
        self.model.undo.last().map(|entry| entry.label.as_str())
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// The row being carried up or down its group, if one is.
    pub fn moving(&self) -> Option<Id> {
        self.moving
    }

    /// The cursor of a list, re-resolved against what the list holds now:
    /// a row that has gone clamps to the first one (ARCHITECTURE.md rule
    /// 6).
    pub fn cursor(&self, list: List) -> Option<RowId> {
        let rows = self.rows_of(list);
        let wanted = match list {
            List::Day => self.cursors.day,
            List::Backlog => self.cursors.backlog,
            List::Days => self.cursors.days,
            List::Notes => self.cursors.notes,
            List::Review => self.cursors.review,
            List::Settings => self.cursors.settings,
        };
        match wanted {
            Some(id) if rows.iter().any(|(row, _)| *row == id) => Some(id),
            // The row has gone since, so the cursor clamps to the first.
            _ => rows.first().map(|(row, _)| *row),
        }
    }

    /// The commands the palette offers: the rows of the key table for the
    /// page beneath it, narrowed by what has been typed, and the ones
    /// that act on the cursor row first, because that is the order the
    /// palette's two sections are in.
    pub fn palette_rows(&self) -> Vec<&'static Binding> {
        let typed = self.popup.as_ref().map(|popup| popup.text.to_lowercase());
        let typed = typed.unwrap_or_default();
        let wanted = typed.trim();
        let mut rows: Vec<&'static Binding> = input::bindings(self.page_context())
            .iter()
            .filter(|binding| !binding.keys.is_empty())
            .filter(|binding| {
                binding.label.to_lowercase().contains(wanted) || binding.shown == wanted
            })
            .collect();
        rows.sort_by_key(|binding| !binding.acts_on_the_row());
        rows
    }

    /// What the search box has found.
    pub fn search_results(&self) -> SearchResults {
        match &self.popup {
            Some(popup) if popup.kind == PopupKind::Search => {
                domain::search(&self.model, popup.text.trim(), self.today)
            }
            _ => SearchResults::default(),
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    // ---- moving about ------------------------------------------------

    fn rest_the_cursors(&mut self) {
        self.cursors = Cursors {
            day: self.rows_of(List::Day).first().map(|(id, _)| *id),
            backlog: self.rows_of(List::Backlog).first().map(|(id, _)| *id),
            days: self.rows_of(List::Days).first().map(|(id, _)| *id),
            notes: self.rows_of(List::Notes).first().map(|(id, _)| *id),
            review: None,
            settings: self.rows_of(List::Settings).first().map(|(id, _)| *id),
        };
    }

    /// The review opens on its first row, whichever step it opens on.
    fn rest_the_review_cursor(&mut self) {
        self.cursors.review = self.rows_of(List::Review).first().map(|(id, _)| *id);
    }

    fn set_cursor(&mut self, list: List, id: RowId) {
        let slot = match list {
            List::Day => &mut self.cursors.day,
            List::Backlog => &mut self.cursors.backlog,
            List::Days => &mut self.cursors.days,
            List::Notes => &mut self.cursors.notes,
            List::Review => &mut self.cursors.review,
            List::Settings => &mut self.cursors.settings,
        };
        *slot = Some(id);
    }

    /// One row down or up, in the popup if one is open, by a line of the
    /// open note if the keyboard is in one, and in the focused list
    /// otherwise. Both ends stop rather than wrap.
    fn step(&mut self, forward: bool) {
        if self.popup.is_none() && self.draft.is_some() {
            self.step_the_caret(forward);
            return;
        }
        if self.popup.is_some() {
            let last = self.popup_rows().saturating_sub(1);
            if let Some(popup) = &mut self.popup {
                popup.selected = if forward {
                    (popup.selected + 1).min(last)
                } else {
                    popup.selected.saturating_sub(1)
                };
            }
            return;
        }

        let list = self.focused();
        let ids: Vec<RowId> = self.rows_of(list).into_iter().map(|(id, _)| id).collect();
        let Some(at) = self
            .cursor(list)
            .and_then(|id| ids.iter().position(|other| *other == id))
        else {
            return;
        };
        let next = if forward {
            (at + 1).min(ids.len().saturating_sub(1))
        } else {
            at.saturating_sub(1)
        };
        self.set_cursor(list, ids[next]);
    }

    /// How many rows the open popup offers.
    fn popup_rows(&self) -> usize {
        match self.popup.as_ref().map(|popup| popup.kind) {
            Some(PopupKind::Palette) => self.palette_rows().len(),
            Some(PopupKind::Search) => self.search_results().total,
            Some(PopupKind::Move) => self.move_choices().len(),
            Some(PopupKind::Repeat) => repeat_shapes().len(),
            _ => 0,
        }
    }

    /// The next pane, or the next tab when the window has collapsed to
    /// one. `wrap` is `tab`, which goes round; `h` and `l` stop.
    fn shift_pane(&mut self, forward: bool, wrap: bool) {
        if self.popup.is_some() || self.editor.is_some() {
            return;
        }
        let narrow = self.layout.narrow;
        match (self.page, self.pane, self.notes_pane, forward) {
            (Page::Home, Pane::Day, _, true) => self.pane = Pane::Backlog,
            (Page::Home, Pane::Backlog, _, false) => self.pane = Pane::Day,
            // Notes is the third tab when the panes have collapsed.
            (Page::Home, Pane::Backlog, _, true) if narrow => self.page = Page::Notes,
            (Page::Home, Pane::Backlog, _, true) if wrap => self.pane = Pane::Day,
            (Page::Home, Pane::Day, _, false) if wrap => self.pane = Pane::Backlog,

            (Page::Notes, _, NotesPane::List, true) => self.open_the_note(),
            (Page::Notes, _, NotesPane::Note, false) => self.leave_the_note(),
            (Page::Notes, _, NotesPane::List, false) if narrow => {
                self.page = Page::Home;
                self.pane = Pane::Backlog;
            }
            (Page::Notes, _, NotesPane::Note, true) if wrap && narrow => {
                self.leave_the_note();
                self.page = Page::Home;
                self.pane = Pane::Day;
            }
            (Page::Notes, _, NotesPane::Note, true) if wrap => self.leave_the_note(),
            _ => {}
        }
    }

    fn turn_the_page(&mut self) {
        self.leave_the_note();
        self.page = match self.page {
            Page::Home => Page::Notes,
            Page::Notes => Page::Home,
            // `n` is not a key of the settings page; the page it was
            // opened from is where every way off it leads.
            Page::Settings => self.came_from,
        };
    }

    /// A click puts the cursor on the row it landed on and moves the
    /// keyboard to that pane. A click on a pane's empty space moves the
    /// keyboard and leaves the cursor where it was.
    fn point_at(&mut self, column: u16, row: u16) {
        if self.popup.is_some() || self.editor.is_some() {
            return;
        }
        let Some(list) = self.layout.list_at(column, row) else {
            return;
        };
        self.focus_on(list);
        if let Some(clicked) = self.layout.row_at(column, row) {
            self.set_cursor(clicked.list, clicked.id);
            self.dragging = Some((clicked.list, clicked.id));
        }
    }

    /// Dragging carries the row under the pointer, one reorder per row it
    /// passes, so that it follows the mouse instead of jumping when the
    /// button comes up. Nothing is only reachable by mouse: this is the
    /// same command `J` and `K` send.
    fn drag_to(&mut self, column: u16, row: u16) {
        let Some((list, task)) = self.dragging else {
            return;
        };
        let Some(over) = self.layout.row_at(column, row) else {
            return;
        };
        if over.list != list || over.id == task {
            return;
        }
        let group = self.group_of(list, task);
        if group != self.group_of(list, over.id) || !group.is_some_and(Group::is_ordered_by_hand) {
            return;
        }
        let (Some(task), Some(under)) = (task.task(), over.id.task()) else {
            return;
        };
        let Some(position) = self.model.live_task(under).map(|task| task.position) else {
            return;
        };
        self.set_cursor(list, RowId::Task(task));
        self.reorder_to(task, position);
    }

    fn focus_on(&mut self, list: List) {
        self.leave_the_note();
        match list {
            List::Day => {
                self.page = Page::Home;
                self.pane = Pane::Day;
            }
            List::Backlog | List::Days => {
                self.page = Page::Home;
                self.pane = Pane::Backlog;
            }
            List::Notes => {
                self.page = Page::Notes;
                self.notes_pane = NotesPane::List;
            }
            List::Settings => self.page = Page::Settings,
            // The review is the whole window, so there is no other pane
            // for a click to move the keyboard to.
            List::Review => {}
        }
    }

    // ---- popups and their text fields --------------------------------

    fn open(&mut self, kind: PopupKind, target: Option<RowId>) {
        self.editor = None;
        self.popup = Some(Popup {
            kind,
            text: String::new(),
            caret: 0,
            selected: 0,
            target,
            card: Card::None,
        });
    }

    /// Escape backs out one level: the popup, then the field, then the
    /// notes page, which `esc` leaves the same way `n` does.
    fn back_out(&mut self) {
        if self.popup.take().is_some() {
            return;
        }
        if self.editor.take().is_some() {
            return;
        }
        if self.setting_draft.take().is_some() {
            return;
        }
        if self.page == Page::Settings {
            self.leave_the_settings();
            return;
        }
        // Escape leaves the review with the pile intact; the home screen
        // counts what is left of it in red (DESIGN.md section 5).
        if self.review.take().is_some() {
            return;
        }
        if self.page == Page::Notes {
            // The note first, then the page: one level at a time.
            if self.draft.is_some() {
                self.leave_the_note();
            } else {
                self.turn_the_page();
            }
        }
    }

    /// Enter: the popup if one is open, and the field under it otherwise.
    fn confirm(&mut self) -> Flow {
        let Some(kind) = self.popup.as_ref().map(|popup| popup.kind) else {
            if self.editor.is_some() {
                self.commit_the_title();
            } else if self.setting_draft.is_some() {
                self.take_the_typed_setting();
            } else if self.page == Page::Settings {
                self.change_the_setting();
            } else if self.review.is_some() {
                self.next_step();
            } else if self.page == Page::Notes {
                self.open_the_note();
            } else {
                self.follow_the_row();
            }
            return Flow::Continue;
        };
        match kind {
            PopupKind::Palette => return self.run_the_selected_command(),
            PopupKind::Search => self.take_the_search(),
            PopupKind::Move => self.take_the_chosen_day(),
            PopupKind::Date => self.take_the_typed_date(),
            PopupKind::Repeat => self.take_the_rule(),
            PopupKind::DeleteQuestion => self.take_the_delete(),
            // The copy question has no answer safe enough to be Enter's.
            PopupKind::Help | PopupKind::CopyQuestion => {}
        }
        Flow::Continue
    }

    /// Enter in the palette runs the row it is on, as if its key had been
    /// pressed on the page beneath.
    fn run_the_selected_command(&mut self) -> Flow {
        let Some(popup) = &self.popup else {
            return Flow::Continue;
        };
        let chosen = self
            .palette_rows()
            .get(popup.selected)
            .and_then(|binding| binding.keys.first())
            .map(|(_, action)| *action);
        self.popup = None;
        match chosen {
            Some(action) => self.update(action),
            None => Flow::Continue,
        }
    }

    /// Enter on the move card moves the task to the day the cursor is on.
    fn take_the_chosen_day(&mut self) {
        let Some(popup) = &self.popup else {
            return;
        };
        let Some(choice) = self.move_choices().get(popup.selected).copied() else {
            return;
        };
        self.move_it(choice.target);
    }

    /// The result the cursor is on: the open matches and then the closed,
    /// which is the order the box draws them in.
    fn found_at_cursor(&self) -> Option<Id> {
        let popup = self.popup.as_ref()?;
        let results = self.search_results();
        results
            .open
            .iter()
            .chain(results.closed.iter())
            .nth(popup.selected)
            .map(|row| row.task)
    }

    /// Enter in search: the day the result is on, which for an open
    /// backlog task is the backlog beside today. A search that found
    /// nothing offers to add what was typed as a task on today instead
    /// (DOMAIN.md section 14).
    fn take_the_search(&mut self) {
        if let Some(task) = self.found_at_cursor() {
            self.popup = None;
            self.follow_the_task(task);
            return;
        }
        let Some(popup) = &self.popup else {
            return;
        };
        let title = popup.text.trim().to_owned();
        if title.is_empty() {
            return;
        }
        self.popup = None;
        self.add_to_today(title);
    }

    /// `alt-t` in search: the title of the result, as a fresh task on
    /// today. What was found stays where it is; nothing about a closed
    /// task is reopened (DOMAIN.md section 14).
    fn readd_from_search(&mut self) {
        let title = self
            .found_at_cursor()
            .and_then(|task| self.model.live_task(task))
            .map(|task| task.title.clone());
        let Some(title) = title else {
            self.say("There is no task here to re-add.", false);
            return;
        };
        self.popup = None;
        self.add_to_today(title);
    }

    /// A new task at the end of today's plan, with the keyboard and the
    /// cursor on it, which is where both offers of the search box end.
    fn add_to_today(&mut self, title: String) {
        let place = Place::Day(self.today);
        if let Some(change) = self.run(Command::AddTask { title, place })
            && let Some(added) = added_task(&change)
        {
            self.show_the_day(self.today);
            self.focus_on(List::Day);
            self.set_cursor(List::Day, RowId::Task(added));
        }
    }

    /// The text field with the keyboard: the one in a popup that is typed
    /// into, the field on a row, or the open note.
    fn field(&mut self) -> Option<(&mut String, &mut usize)> {
        if let Some(popup) = &mut self.popup {
            let typed = match popup.kind {
                PopupKind::Palette | PopupKind::Search => true,
                PopupKind::Date => !matches!(&popup.card, Card::Date(draft) if draft.in_calendar),
                _ => false,
            };
            if !typed {
                return None;
            }
            return Some((&mut popup.text, &mut popup.caret));
        }
        if let Some(editor) = &mut self.editor {
            return Some((&mut editor.text, &mut editor.caret));
        }
        if let Some(draft) = &mut self.setting_draft {
            return Some((&mut draft.text, &mut draft.caret));
        }
        let draft = self.draft.as_mut()?;
        Some((&mut draft.text, &mut draft.caret))
    }

    /// The caret one line down or up the open note, keeping the column it
    /// was in as far as the line it lands on has one.
    fn step_the_caret(&mut self, down: bool) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        let starts = line_starts(&draft.text);
        let at = line_at(&starts, draft.caret);
        let next = if down { at + 1 } else { at.wrapping_sub(1) };
        let Some(start) = starts.get(next).copied() else {
            return;
        };
        let column = draft.caret - starts[at];
        let end = starts
            .get(next + 1)
            .map_or(glyphs(&draft.text), |after| after - 1);
        draft.caret = (start + column).min(end);
    }

    /// Home and End, which in a note are the ends of the line the caret is
    /// on rather than the ends of the whole body.
    fn jump_to_the_edge(&mut self, end: bool) {
        let Some(draft) = &mut self.draft else {
            self.set_caret(if end { usize::MAX } else { 0 });
            return;
        };
        let starts = line_starts(&draft.text);
        let at = line_at(&starts, draft.caret);
        draft.caret = if end {
            starts
                .get(at + 1)
                .map_or(glyphs(&draft.text), |after| after - 1)
        } else {
            starts[at]
        };
    }

    /// What a keystroke changes besides the text: a filtered list starts
    /// at the top again, and the date card follows what has been typed as
    /// far as the domain can read it.
    fn after_typing(&mut self) {
        if let Some(popup) = &mut self.popup {
            popup.selected = 0;
        }
        let today = self.today;
        let Some(popup) = &mut self.popup else {
            return;
        };
        let Some(date) = domain::parse_date(&popup.text, today) else {
            return;
        };
        if let Card::Date(draft) = &mut popup.card {
            draft.on = date;
        }
    }

    /// A character typed goes in at the caret. Where the caret lands
    /// afterwards is counted from the text again rather than stepped on,
    /// because a combining mark joins the cluster in front of it and adds
    /// no cluster of its own: the accent of a decomposed `é` is typed
    /// after the `e` and the caret stays where it was.
    fn type_in(&mut self, typed: char) {
        if let Some((text, caret)) = self.field() {
            let at = byte_at(text, *caret);
            text.insert(at, typed);
            *caret = glyphs(&text[..at + typed.len_utf8()]);
        }
        self.after_typing();
    }

    /// Backspace takes the whole cluster before the caret, so a family
    /// emoji leaves in one press rather than coming apart.
    fn rub_out(&mut self) {
        if let Some((text, caret)) = self.field() {
            if *caret == 0 {
                return;
            }
            let from = byte_at(text, *caret - 1);
            let to = byte_at(text, *caret);
            text.replace_range(from..to, "");
            *caret -= 1;
        }
        self.after_typing();
    }

    /// Delete takes the whole cluster at the caret, for the same reason.
    fn rub_forward(&mut self) {
        if let Some((text, caret)) = self.field() {
            let from = byte_at(text, *caret);
            let to = byte_at(text, *caret + 1);
            text.replace_range(from..to, "");
        }
        self.after_typing();
    }

    fn move_caret(&mut self, forward: bool) {
        if let Some((text, caret)) = self.field() {
            let last = glyphs(text);
            *caret = if forward {
                (*caret + 1).min(last)
            } else {
                caret.saturating_sub(1)
            };
        }
    }

    fn set_caret(&mut self, at: usize) {
        if let Some((text, caret)) = self.field() {
            *caret = at.min(glyphs(text));
        }
    }
}

/// The rows of the repeat card, in the order the key table puts them,
/// which is what a row's number means. Both the card and its drawing read
/// the shapes from here, so neither can invent a row the other lacks.
pub(crate) fn repeat_shapes() -> Vec<Action> {
    input::bindings(KeyContext::Popup {
        kind: PopupKind::Repeat,
        text_field: false,
    })
    .iter()
    .filter_map(|binding| match binding.keys.first() {
        Some((_, action)) if is_a_shape(*action) => Some(*action),
        _ => None,
    })
    .collect()
}

fn is_a_shape(action: Action) -> bool {
    matches!(
        action,
        Action::EveryWorkDay
            | Action::EveryDay
            | Action::EveryWeek
            | Action::EveryMonth
            | Action::EveryFewWeeks
            | Action::StopRepeat
    )
}

/// Which row of the card a rule is, so that the card opens on the shape
/// the schedule already has.
fn shape_of(rule: &Rule) -> Option<usize> {
    let shape = match rule {
        Rule::Workdays => Action::EveryWorkDay,
        Rule::Daily => Action::EveryDay,
        Rule::Weekly { .. } => Action::EveryWeek,
        Rule::Monthly { .. } => Action::EveryMonth,
        Rule::EveryNWeeks { .. } => Action::EveryFewWeeks,
    };
    repeat_shapes().iter().position(|other| *other == shape)
}

/// Where every line of a body starts, in clusters. A body has at least
/// one line, and a trailing newline opens another.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (at, glyph) in text.graphemes(true).enumerate() {
        if glyph == "\n" {
            starts.push(at + 1);
        }
    }
    starts
}

/// Which of those lines a caret is on.
fn line_at(starts: &[usize], caret: usize) -> usize {
    starts
        .iter()
        .rposition(|start| *start <= caret)
        .unwrap_or_default()
}

/// How many grapheme clusters a string is, which is the unit a caret
/// counts in: `café` written as an `e` and a combining accent is four,
/// not five, and a family emoji is one.
fn glyphs(text: &str) -> usize {
    text.graphemes(true).count()
}

/// The byte offset of a cluster offset, so that a caret counted in
/// clusters can index a `String`.
fn byte_at(text: &str, caret: usize) -> usize {
    text.grapheme_indices(true)
        .nth(caret)
        .map_or(text.len(), |(at, _)| at)
}

/// The three date orders in the order the row steps through them, which
/// is what a step and a cycle both count in.
const DATE_STYLES: [DateStyle; 3] = [
    DateStyle::Locale,
    DateStyle::DayFirst,
    DateStyle::MonthFirst,
];

/// The value a settings row is stepped to by `h` and `l`: the state on
/// that side of the one it holds, and the state it holds when there is
/// none, so the ends of a row stop rather than wrap. Every setter holds
/// what it is given to the setting's range, so a step off the end is the
/// end (DOMAIN.md section 19).
fn stepped(settings: &Settings, row: SettingRow, forward: bool) -> Settings {
    let step = |n: u16| i64::from(n) + if forward { 1 } else { -1 };
    let mut next = settings.clone();
    match row {
        SettingRow::DayStartsAt => {
            next.set_day_starts_at(step(u16::from(settings.day_starts_at())));
        }
        SettingRow::WeekStartsOn => next.set_week_starts_on(if forward {
            WeekStart::Sunday
        } else {
            WeekStart::Monday
        }),
        SettingRow::WorkDay(day) => {
            let mut days = settings.work_days();
            if days.contains(day) != forward {
                days.toggle(day);
            }
            next.set_work_days(days);
        }
        SettingRow::ReviewOpensItself => next.set_review_opens_itself(forward),
        SettingRow::DueAheadDays => next.set_due_ahead_days(step(settings.due_ahead_days())),
        SettingRow::BackfillDays => next.set_backfill_days(step(settings.backfill_days())),
        SettingRow::PileHorizonDays => {
            next.set_pile_horizon_days(step(settings.pile_horizon_days()));
        }
        SettingRow::FloatingWindow => next.set_floating_window(forward),
        SettingRow::WindowSize => {
            next.set_window_size(next_preset(settings.window_size(), forward))
        }
        SettingRow::Mouse => next.set_mouse(forward),
        SettingRow::DateOrder => {
            let at = date_style_at(settings);
            let to = if forward {
                (at + 1).min(DATE_STYLES.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            next.set_date_style(DATE_STYLES[to]);
        }
        SettingRow::MessageSeconds => {
            next.set_message_seconds(step(u16::from(settings.message_seconds())));
        }
        SettingRow::ConfirmDelete => next.set_confirm_delete(forward),
        SettingRow::SpellCheckNotes => next.set_spell_check_notes(forward),
    }
    next
}

/// The value `space` and Enter change a row to: the other state of a
/// toggle, and the next of a row with more than two, round to the first.
/// A row whose value is typed has no next one; Enter opens its field.
fn cycled(settings: &Settings, row: SettingRow) -> Settings {
    let mut next = settings.clone();
    match row {
        SettingRow::WeekStartsOn => next.set_week_starts_on(match settings.week_starts_on() {
            WeekStart::Monday => WeekStart::Sunday,
            WeekStart::Sunday => WeekStart::Monday,
        }),
        SettingRow::WorkDay(day) => next.toggle_work_day(day),
        SettingRow::ReviewOpensItself => {
            next.set_review_opens_itself(!settings.review_opens_itself());
        }
        SettingRow::FloatingWindow => next.set_floating_window(!settings.floating_window()),
        SettingRow::Mouse => next.set_mouse(!settings.mouse()),
        SettingRow::DateOrder => {
            let at = (date_style_at(settings) + 1) % DATE_STYLES.len();
            next.set_date_style(DATE_STYLES[at]);
        }
        SettingRow::ConfirmDelete => next.set_confirm_delete(!settings.confirm_delete()),
        SettingRow::SpellCheckNotes => {
            next.set_spell_check_notes(!settings.spell_check_notes());
        }
        SettingRow::DayStartsAt
        | SettingRow::DueAheadDays
        | SettingRow::BackfillDays
        | SettingRow::PileHorizonDays
        | SettingRow::WindowSize
        | SettingRow::MessageSeconds => {}
    }
    next
}

/// The preset on one side of a size: the next one along from a preset,
/// the nearest one that way from a size somebody typed, and the size
/// itself when there is none, so the row stops at both ends.
fn next_preset(size: WindowSize, forward: bool) -> WindowSize {
    let here = (size.width, size.height);
    let mut beyond = WindowSize::PRESETS
        .into_iter()
        .filter(|preset| ((preset.width, preset.height) > here) == forward)
        .filter(|preset| (preset.width, preset.height) != here);
    if forward {
        beyond.next()
    } else {
        beyond.next_back()
    }
    .unwrap_or(size)
}

fn date_style_at(settings: &Settings) -> usize {
    DATE_STYLES
        .iter()
        .position(|style| *style == settings.date_style())
        .unwrap_or_default()
}

/// What the field opens on, which is the value the row already holds,
/// written the way it is typed.
fn typed_value(settings: &Settings, row: SettingRow) -> String {
    match row {
        SettingRow::DayStartsAt => settings.day_starts_at().to_string(),
        SettingRow::DueAheadDays => settings.due_ahead_days().to_string(),
        SettingRow::BackfillDays => settings.backfill_days().to_string(),
        SettingRow::PileHorizonDays => settings.pile_horizon_days().to_string(),
        SettingRow::MessageSeconds => settings.message_seconds().to_string(),
        SettingRow::WindowSize => {
            let size = settings.window_size();
            format!("{}x{}", size.width, size.height)
        }
        _ => String::new(),
    }
}

/// The settings a typed line means, or nothing when it is not a number
/// or a size. A number outside its range is held to the range rather
/// than refused, so only nonsense comes back empty-handed.
fn typed_into(settings: &Settings, row: SettingRow, typed: &str) -> Option<Settings> {
    let mut next = settings.clone();
    if row == SettingRow::WindowSize {
        let (width, height) = typed.split_once(['x', 'X'])?;
        next.set_window_size(WindowSize::new(
            width.trim().parse().ok()?,
            height.trim().parse().ok()?,
        ));
        return Some(next);
    }
    let number: i64 = typed.parse().ok()?;
    match row {
        SettingRow::DayStartsAt => next.set_day_starts_at(number),
        SettingRow::DueAheadDays => next.set_due_ahead_days(number),
        SettingRow::BackfillDays => next.set_backfill_days(number),
        SettingRow::PileHorizonDays => next.set_pile_horizon_days(number),
        SettingRow::MessageSeconds => next.set_message_seconds(number),
        _ => return None,
    }
    Some(next)
}

/// What the hint bar calls a change, which is the label the domain put on
/// the undo entry. A change with no entry is one nobody can undo.
fn label_of(change: &Change) -> Option<String> {
    change.writes.iter().find_map(|write| match write {
        Write::PushUndo(entry) => Some(entry.label.clone()),
        _ => None,
    })
}

/// The note a CreateNote wrote, so the cursor can land on it.
fn added_note(change: &Change) -> Option<Id> {
    change.writes.iter().find_map(|write| match write {
        Write::PutNote(note) => Some(note.id),
        _ => None,
    })
}

/// The task an AddTask wrote, so the cursor can land on it.
fn added_task(change: &Change) -> Option<Id> {
    change.writes.iter().find_map(|write| match write {
        Write::PutTask(task) => Some(task.id),
        _ => None,
    })
}

/// The window settings, set from the command line rather than from the
/// page.
///
/// `jobsdone desktop` is how the install script asks for the window it
/// was told to ask for, and how a machine that gains a Hyprland later
/// gets the rule written. None of the rest of a launch happens
/// here: no copies are made and no review is opened, so the command never
/// spends the day's review. A flag that was not passed leaves its setting
/// alone.
///
/// What comes back is the one line the command prints. Where there is
/// no window manager the settings are saved for when there is one and
/// the line says so; the error is what a window manager that is there
/// said, the settings being saved either way.
pub fn set_window(
    store: &mut dyn Store,
    desktop: &dyn Desktop,
    floating: Option<bool>,
    size: Option<WindowSize>,
) -> Result<String, String> {
    let mut model = store.load().map_err(|error| error.to_string())?;
    let mut settings = model.settings.clone();
    if let Some(floating) = floating {
        settings.set_floating_window(floating);
    }
    if let Some(size) = size {
        settings.set_window_size(size);
    }

    let change = domain::change_settings(&model, settings).map_err(|why| why.to_string())?;
    if !change.writes.is_empty() {
        store.commit(&change).map_err(|error| error.to_string())?;
        model.apply(&change);
    }

    let floating = model.settings.floating_window();
    let size = model.settings.window_size();
    if !desktop.available() {
        return Ok("Hyprland is not here; the settings are kept for when it is.".to_owned());
    }
    desktop.apply_window(floating, size)?;
    Ok(if floating {
        format!("the window floats at {}x{}", size.width, size.height)
    } else {
        "the window tiles".to_owned()
    })
}
