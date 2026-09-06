//! A terminal event plus the current key context in, a named action out.
//! Owns the key table.
//!
//! This module knows nothing about tasks: an action names what the user
//! asked for, never what it is asked of.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};

#[cfg(test)]
mod tests;

/// A pane of the home page, or the tab that stands for it when the window
/// is too narrow for both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Day,
    Backlog,
}

/// Which day the day pane of the home page is on, because history is the
/// same page stepped to another day and its keys are not the same
/// (DESIGN.md section 6): `t` puts a task from a past day onto today,
/// which on today itself would mean nothing, and the pane beside it is
/// the list of days rather than the backlog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    Today,
    Past,
    Future,
}

/// A pane of the notes page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotesPane {
    List,
    Note,
}

/// Which step of the morning review is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewStep {
    Pile,
    Surfaced,
}

/// Which popup is over the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupKind {
    Palette,
    Search,
    Help,
    /// The move card: the day a task is sent to.
    Move,
    /// The date card: due by, remind on, or the day the move card sends
    /// a task to. One card, three things to set.
    Date,
    /// The repeat card: the five rule shapes, and stopping.
    Repeat,
    /// The one deliberate question: whether a recurring copy's new title
    /// is for this copy or for this and future copies.
    CopyQuestion,
    /// The question `x` asks while `confirm_delete` is on, about a task
    /// or a note.
    DeleteQuestion,
}

/// The in-place text field on the home page, which is the only place a
/// task's title is written. Which one it is decides what Enter is called,
/// because adding keeps the field open and renaming does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Adding,
    Renaming,
}

/// Where the keyboard is, which is what decides what a key means.
///
/// With a text field set, every printable key becomes `Insert` and only
/// `Enter`, `Escape`, `Tab`, `↑`, `↓`, the editing keys and the `Alt`
/// shortcuts keep a name of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyContext {
    Home {
        pane: Pane,
        day: Shown,
        field: Option<Field>,
    },
    Notes {
        pane: NotesPane,
        text_field: bool,
    },
    Review {
        step: ReviewStep,
        /// Whether the step asks a decision about any of its rows. A step
        /// of copies that only started this morning asks none, so it
        /// offers no outcome at all (DESIGN.md section 5).
        asks: bool,
        text_field: bool,
    },
    Popup {
        kind: PopupKind,
        text_field: bool,
    },
    /// The settings page. `field` is the number or the window size being
    /// typed on a row, which is the only text field it has, so there is
    /// nothing for the hint bar to tell two of them apart by.
    Settings {
        field: bool,
    },
}

impl KeyContext {
    /// Whether a text field has the keyboard.
    pub fn text_field(self) -> bool {
        match self {
            KeyContext::Home { field, .. } => field.is_some(),
            KeyContext::Notes { text_field, .. }
            | KeyContext::Review { text_field, .. }
            | KeyContext::Popup { text_field, .. } => text_field,
            KeyContext::Settings { field } => field,
        }
    }
}

/// What the user asked for. Cursor-relative, never carrying an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // The loop.
    Tick,
    Resize,
    FocusGained,
    Quit,

    // Moving about.
    Down,
    Up,
    PaneLeft,
    PaneRight,
    NextPane,
    MoveDown,
    MoveUp,
    PrevDay,
    NextDay,
    Today,
    GoToDate,
    NotesPage,
    /// The settings page, and the way back off it: `,` opens it from a
    /// page and closes it again.
    SettingsPage,
    /// The morning review again, after it was left or on a day it has
    /// already run on.
    OpenReview,

    // The cursor row.
    Close,
    Focus,
    Add,
    Edit,
    Delete,
    ToToday,
    ToBacklog,
    MoveToDay,
    /// The move card's own days, which are the same Monday-to-Friday
    /// definition the work-days rule uses.
    Tomorrow,
    NextWorkDay,
    NextMonday,
    DueBy,
    RemindOn,
    /// The date card's own days, and the pick that takes a date off.
    InAWeek,
    EndOfMonth,
    ClearDate,
    /// The month the calendar is showing.
    PrevMonth,
    NextMonth,
    Waiting,
    Repeat,
    /// The rows of the repeat card, which are the five rule shapes of
    /// DOMAIN.md section 10 and the end of them all.
    EveryWorkDay,
    EveryDay,
    EveryWeek,
    EveryMonth,
    EveryFewWeeks,
    StopRepeat,
    /// Take the highlighted thing into the row's answer, which is how a
    /// weekday joins the set the weekly shape repeats on.
    Pick,
    Keep,
    Undo,
    /// The two answers to the copy question.
    ThisCopy,
    ThisAndFuture,

    // Popups.
    Search,
    Commands,
    Help,
    Confirm,
    Cancel,

    // Text fields.
    Insert(char),
    Backspace,
    DeleteForward,
    Left,
    Right,
    LineStart,
    LineEnd,

    // The mouse, in cell coordinates.
    MouseDown {
        column: u16,
        row: u16,
    },
    MouseUp {
        column: u16,
        row: u16,
    },
    MouseDrag {
        column: u16,
        row: u16,
    },
    Scroll {
        column: u16,
        row: u16,
        down: bool,
    },
}

/// Which end of the hint bar a row sits at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Where the hint bar puts a row, and what it calls it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    /// Not in the bar.
    Off,
    /// At the left end, in table order, under the row's own label.
    Left,
    /// At the right end, in table order, under the row's own label.
    Right,
    /// On that side under a shorter name, because the bar is tight.
    Short(Side, &'static str),
}

impl Bar {
    /// The side and the name the bar uses, or nothing when the bar leaves
    /// the row out.
    pub fn slot(self, label: &'static str) -> Option<(Side, &'static str)> {
        match self {
            Bar::Off => None,
            Bar::Left => Some((Side::Left, label)),
            Bar::Right => Some((Side::Right, label)),
            Bar::Short(side, name) => Some((side, name)),
        }
    }
}

/// One row of the key table: the keys that run it, what they do, and what
/// to call it.
///
/// The hint bar, the command palette and the help overlay are drawn from
/// these rows and from nothing else, so none of them can disagree with the
/// dispatcher. Two keys share a row when they are one idea: `J` and `K`
/// are "reorder", and a row may accept more keys than `shown` advertises,
/// which is how the arrow keys stand in for `j` and `k`.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    /// Every key that runs the row, with the action it means.
    pub keys: &'static [(&'static str, Action)],
    /// How the keys are written when the row is named: `J/K`, `tab h/l`,
    /// `[ ]`. A row with no keys at all, such as "type to filter", is a
    /// line of the hint bar and nothing else.
    pub shown: &'static str,
    /// What the row is called, in the hint bar, the palette and the help
    /// overlay. Empty only on a caption row, which names the rows beside
    /// it in the bar rather than a key of its own.
    pub label: &'static str,
    /// Where the hint bar puts it when both panes are on screen.
    pub bar: Bar,
    /// Where it puts it when the window has collapsed to tabs and the bar
    /// has room for about five keys.
    pub narrow: Bar,
}

impl Binding {
    /// Whether the row does something to the row the cursor is on, rather
    /// than to the page or the program. The command palette is in two
    /// sections along this line, the row's and the app's (wireframe 11).
    ///
    /// `Add` is the app's: it puts a new row in the pane rather than
    /// touching the one under the cursor. `Confirm` is the row's in every
    /// page table, where it opens a note, follows a moved task or goes to
    /// a day.
    pub fn acts_on_the_row(&self) -> bool {
        self.keys.first().is_some_and(|(_, action)| {
            matches!(
                action,
                Action::Close
                    | Action::Focus
                    | Action::Edit
                    | Action::Delete
                    | Action::ToToday
                    | Action::ToBacklog
                    | Action::MoveToDay
                    | Action::DueBy
                    | Action::RemindOn
                    | Action::Waiting
                    | Action::Repeat
                    | Action::MoveUp
                    | Action::MoveDown
                    | Action::Confirm
            )
        })
    }
}

// ---- the key table ---------------------------------------------------
//
// One table per context. The order of the rows is the order of the hint
// bar; a row the bar leaves out still teaches its key in the palette and
// the help overlay.

/// A home table: the three day keys, its own rows, then switching pane
/// and the rows every page shares. One macro rather than one const per
/// group, because [`bindings`] hands out a single slice and the order of
/// the rows is the order of the hint bar.
///
/// The day keys take their places in the bar as arguments, wide and
/// narrow, because on today they are worth no room and on any other day
/// they are what the pane is for (wireframe 08).
macro_rules! home_table {
    (
        steps: $steps:expr, $steps_narrow:expr;
        today: $today:expr, $today_narrow:expr;
        go_to: $goto:expr, $goto_narrow:expr;
        $($own:expr),* $(,)?
    ) => {
        &[
            Binding {
                keys: &[("[", Action::PrevDay), ("]", Action::NextDay)],
                shown: "[ ]",
                label: "prev/next day",
                bar: $steps,
                narrow: $steps_narrow,
            },
            Binding {
                keys: &[(".", Action::Today)],
                shown: ".",
                label: "today",
                bar: $today,
                narrow: $today_narrow,
            },
            Binding {
                keys: &[("g", Action::GoToDate)],
                shown: "g",
                label: "go to date",
                bar: $goto,
                narrow: $goto_narrow,
            },
            $($own,)*
            Binding {
                keys: &[
                    ("tab", Action::NextPane),
                    ("h", Action::PaneLeft),
                    ("l", Action::PaneRight),
                ],
                shown: "tab h/l",
                label: "pane",
                bar: Bar::Right,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[
                    ("j", Action::Down),
                    ("k", Action::Up),
                    ("down", Action::Down),
                    ("up", Action::Up),
                ],
                shown: "j/k",
                label: "move",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("u", Action::Undo)],
                shown: "u",
                label: "undo",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("/", Action::Search)],
                shown: "/",
                label: "search",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[(":", Action::Commands)],
                shown: ":",
                label: "commands",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("?", Action::Help)],
                shown: "?",
                label: "help",
                bar: Bar::Off,
                narrow: Bar::Short(Side::Right, "more"),
            },
            Binding {
                keys: &[("n", Action::NotesPage)],
                shown: "n",
                label: "notes page",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[(",", Action::SettingsPage)],
                shown: ",",
                label: "settings",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            // The review opens itself once a morning; this is how it is
            // picked up again after it was left (DOMAIN.md section 13).
            Binding {
                keys: &[("M", Action::OpenReview)],
                shown: "M",
                label: "morning review",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("q", Action::Quit)],
                shown: "q",
                label: "quit",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
        ]
    };
}

const HOME_DAY: &[Binding] = home_table![
    steps: Bar::Off, Bar::Off;
    today: Bar::Off, Bar::Off;
    go_to: Bar::Off, Bar::Off;
    Binding {
        keys: &[("J", Action::MoveDown), ("K", Action::MoveUp)],
        shown: "J/K",
        label: "reorder",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("space", Action::Close)],
        shown: "space",
        label: "done",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("f", Action::Focus)],
        shown: "f",
        label: "focus",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("a", Action::Add)],
        shown: "a",
        label: "add",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("e", Action::Edit)],
        shown: "e",
        label: "edit",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("b", Action::ToBacklog)],
        shown: "b",
        label: "to backlog",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "backlog"),
    },
    Binding {
        keys: &[("m", Action::MoveToDay)],
        shown: "m",
        label: "move to day…",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("R", Action::Repeat)],
        shown: "R",
        label: "repeat",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("x", Action::Delete)],
        shown: "x",
        label: "delete",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "del"),
    },
    // Waiting is a backlog state, so on a day task it is a move as well
    // as a flag (DOMAIN.md section 9). The bar is full by here, and the
    // palette and the help overlay teach it.
    Binding {
        keys: &[("w", Action::Waiting)],
        shown: "w",
        label: "waiting",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    // Today has a Moved group as readily as a past day does, so the key
    // that follows a pointer belongs here too.
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "follow moved",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

const HOME_BACKLOG: &[Binding] = home_table![
    steps: Bar::Off, Bar::Off;
    today: Bar::Off, Bar::Off;
    go_to: Bar::Off, Bar::Off;
    // The backlog is ordered by hand, like a day, so it reorders by
    // keyboard, like a day (DOMAIN.md section 4). Ten keys already fill
    // the bar here, so the palette and the help overlay teach it.
    Binding {
        keys: &[("J", Action::MoveDown), ("K", Action::MoveUp)],
        shown: "J/K",
        label: "reorder",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("space", Action::Close)],
        shown: "space",
        label: "done",
        bar: Bar::Off,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("t", Action::ToToday)],
        shown: "t",
        label: "to today",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "today"),
    },
    Binding {
        keys: &[("m", Action::MoveToDay)],
        shown: "m",
        label: "move…",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("d", Action::DueBy)],
        shown: "d",
        label: "due by",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("r", Action::RemindOn)],
        shown: "r",
        label: "remind on",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("w", Action::Waiting)],
        shown: "w",
        label: "waiting",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("R", Action::Repeat)],
        shown: "R",
        label: "repeat",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("a", Action::Add)],
        shown: "a",
        label: "add",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("e", Action::Edit)],
        shown: "e",
        label: "edit",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("x", Action::Delete)],
        shown: "x",
        label: "delete",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "del"),
    },
];

/// A day that is not today. Stepping is what the pane is for, so the day
/// keys lead the bar; `t` puts a task from a day that has passed onto
/// today, which on today itself would mean nothing (wireframe 08).
const HOME_OTHER_DAY: &[Binding] = home_table![
    steps: Bar::Short(Side::Left, "day"), Bar::Short(Side::Left, "day");
    // The narrow bar calls the way home "back", as the notes page does,
    // because `t to today` is beside it and means something else.
    today: Bar::Left, Bar::Short(Side::Left, "back");
    go_to: Bar::Left, Bar::Off;
    Binding {
        keys: &[("space", Action::Close)],
        shown: "space",
        label: "close",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("t", Action::ToToday)],
        shown: "t",
        label: "to today",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "today"),
    },
    Binding {
        keys: &[("b", Action::ToBacklog)],
        shown: "b",
        label: "to backlog",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("m", Action::MoveToDay)],
        shown: "m",
        label: "move…",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    // A pointer, so Enter goes to wherever the task is
    // now rather than doing anything to the row (DESIGN.md section 6).
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "follow moved",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("a", Action::Add)],
        shown: "a",
        label: "add to this day",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("e", Action::Edit)],
        shown: "e",
        label: "edit",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("x", Action::Delete)],
        shown: "x",
        label: "delete",
        bar: Bar::Off,
        narrow: Bar::Short(Side::Left, "del"),
    },
];

/// The list of days the backlog pane becomes while history is browsed.
/// Its rows are days, so nothing that acts on a task is bound here.
const HOME_DAYS: &[Binding] = home_table![
    steps: Bar::Short(Side::Left, "day"), Bar::Short(Side::Left, "day");
    today: Bar::Left, Bar::Short(Side::Left, "back");
    go_to: Bar::Left, Bar::Off;
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "go to that day",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

const NOTES_LIST: &[Binding] = &[
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "open",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("a", Action::Add)],
        shown: "a",
        label: "new",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("x", Action::Delete)],
        shown: "x",
        label: "delete",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "del"),
    },
    // `esc` leaves the page as well, which is what the status line
    // promises; `n` is the key the bar has room to name.
    Binding {
        keys: &[("n", Action::NotesPage), ("esc", Action::Cancel)],
        shown: "n",
        label: "back to today",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "back"),
    },
    Binding {
        keys: &[(",", Action::SettingsPage)],
        shown: ",",
        label: "settings",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[
            ("tab", Action::NextPane),
            ("h", Action::PaneLeft),
            ("l", Action::PaneRight),
        ],
        shown: "tab h/l",
        label: "pane",
        bar: Bar::Right,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j/k",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("u", Action::Undo)],
        shown: "u",
        label: "undo",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("/", Action::Search)],
        shown: "/",
        label: "search",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[(":", Action::Commands)],
        shown: ":",
        label: "commands",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("?", Action::Help)],
        shown: "?",
        label: "help",
        bar: Bar::Off,
        narrow: Bar::Short(Side::Right, "more"),
    },
    Binding {
        keys: &[("q", Action::Quit)],
        shown: "q",
        label: "quit",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

const NOTES_NOTE: &[Binding] = &[
    Binding {
        keys: &[],
        shown: "type",
        label: "to edit",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "back to the list",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "back"),
    },
    // A note is a text area, so Enter is a line of it rather than
    // something to confirm.
    Binding {
        keys: &[("enter", Action::Insert('\n'))],
        shown: "⏎",
        label: "new line",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("up", Action::Up), ("down", Action::Down)],
        shown: "↑/↓",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("tab", Action::NextPane)],
        shown: "tab",
        label: "pane",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    // The list's keys, named but not bound: here every letter types, so
    // these rows say where they work instead of claiming to work here.
    Binding {
        keys: &[],
        shown: "in the list:",
        label: "",
        bar: Bar::Right,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[],
        shown: "⏎",
        label: "open",
        bar: Bar::Right,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[],
        shown: "a",
        label: "new",
        bar: Bar::Right,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[],
        shown: "x",
        label: "delete",
        bar: Bar::Right,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[],
        shown: "n",
        label: "back to today",
        bar: Bar::Right,
        narrow: Bar::Off,
    },
];

const REVIEW_PILE: &[Binding] = &[
    Binding {
        keys: &[("d", Action::Close)],
        shown: "d",
        label: "done",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("t", Action::ToToday)],
        shown: "t",
        label: "today",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("b", Action::ToBacklog)],
        shown: "b",
        label: "backlog",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("m", Action::MoveToDay)],
        shown: "m",
        label: "move…",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("x", Action::Delete)],
        shown: "x",
        label: "delete",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "del"),
    },
    Binding {
        keys: &[("u", Action::Undo)],
        shown: "u",
        label: "undo",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("e", Action::Edit)],
        shown: "e",
        label: "edit",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "next step",
        bar: Bar::Right,
        narrow: Bar::Right,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "skip",
        bar: Bar::Right,
        narrow: Bar::Right,
    },
    // `k` is keep here, so the cursor moves with `j` and the arrows
    // (DESIGN.md section 4).
    Binding {
        keys: &[
            ("j", Action::Down),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j ↑/↓",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("?", Action::Help)],
        shown: "?",
        label: "help",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("q", Action::Quit)],
        shown: "q",
        label: "quit",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

const REVIEW_SURFACED: &[Binding] = &[
    Binding {
        keys: &[("t", Action::ToToday)],
        shown: "t",
        label: "today",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("k", Action::Keep)],
        shown: "k",
        label: "keep",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("d", Action::DueBy)],
        shown: "d",
        label: "due…",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("r", Action::RemindOn)],
        shown: "r",
        label: "remind…",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("w", Action::Waiting)],
        shown: "w",
        label: "waiting",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("space", Action::Close)],
        shown: "space",
        label: "done",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    // Six keys fill the bar, and every one of them can be taken back.
    Binding {
        keys: &[("u", Action::Undo)],
        shown: "u",
        label: "undo",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "start the day",
        bar: Bar::Right,
        narrow: Bar::Right,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "skip",
        bar: Bar::Right,
        narrow: Bar::Right,
    },
    Binding {
        keys: &[
            ("j", Action::Down),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j ↑/↓",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("?", Action::Help)],
        shown: "?",
        label: "help",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("q", Action::Quit)],
        shown: "q",
        label: "quit",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

/// A step whose rows are all information: the copies a schedule started
/// this morning. Nothing is decided about them, so the one thing to
/// press is all the bar has to name (DESIGN.md section 5).
const REVIEW_INFORMATION: &[Binding] = &[
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "start the day",
        bar: Bar::Right,
        narrow: Bar::Right,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "skip",
        bar: Bar::Right,
        narrow: Bar::Right,
    },
    // `k` keeps its silence here too: the review is one set of keys
    // whether or not the step in front of you asks anything.
    Binding {
        keys: &[
            ("j", Action::Down),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j ↑/↓",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("?", Action::Help)],
        shown: "?",
        label: "help",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("q", Action::Quit)],
        shown: "q",
        label: "quit",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

/// One row of the panel beside the review: the key that decides the row
/// the cursor is on, what the panel calls it, and what it does to the
/// task.
///
/// The panel says more than the hint bar has room for, so the names are
/// its own; the keys are rows of the step's own table, and a test holds
/// the two together, so the panel cannot offer a key the dispatcher does
/// not have.
#[derive(Clone, Copy, Debug)]
pub struct Decision {
    pub key: &'static str,
    pub action: Action,
    /// What the panel calls it: "Move to today" where the bar says
    /// "today".
    pub label: &'static str,
    /// What it does to the task, in the few words at the right of the
    /// row. Empty where the name says it all.
    pub note: &'static str,
}

/// The outcomes PRODUCT.md gives a task on the pile, plus "move to a
/// day".
const PILE_DECISIONS: &[Decision] = &[
    Decision {
        key: "d",
        action: Action::Close,
        label: "Done",
        note: "was finished",
    },
    Decision {
        key: "t",
        action: Action::ToToday,
        label: "Move to today",
        note: "end of plan",
    },
    Decision {
        key: "b",
        action: Action::ToBacklog,
        label: "Back to backlog",
        note: "",
    },
    Decision {
        key: "m",
        action: Action::MoveToDay,
        label: "Move to a day…",
        note: "",
    },
    Decision {
        key: "x",
        action: Action::Delete,
        label: "Delete",
        note: "undo: u",
    },
];

/// What a surfaced task is answered with. A date is changed with `d` and
/// `r`; the panel names the one the step is mostly about.
const SURFACED_DECISIONS: &[Decision] = &[
    Decision {
        key: "t",
        action: Action::ToToday,
        label: "Pull onto today",
        note: "to plan",
    },
    Decision {
        key: "k",
        action: Action::Keep,
        label: "Keep in backlog",
        note: "tomorrow",
    },
    Decision {
        key: "d",
        action: Action::DueBy,
        label: "Change due date…",
        note: "",
    },
    Decision {
        key: "w",
        action: Action::Waiting,
        label: "Mark waiting",
        note: "no nag",
    },
    Decision {
        key: "space",
        action: Action::Close,
        label: "Done",
        note: "",
    },
];

/// The rows of the panel beside a step of the review.
pub fn decisions(step: ReviewStep) -> &'static [Decision] {
    match step {
        ReviewStep::Pile => PILE_DECISIONS,
        ReviewStep::Surfaced => SURFACED_DECISIONS,
    }
}

/// The in-place field on a task row. Adding keeps the field open after
/// Enter so that a list is typed in one go; renaming closes it.
const HOME_ADDING: &[Binding] = &[
    Binding {
        keys: &[],
        shown: "type",
        label: "the whole task",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "add & keep typing",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "add"),
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "stop",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

const HOME_RENAMING: &[Binding] = &[
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "save",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "cancel",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

/// The move card. Its six choices are rows of the card rather than of the
/// hint bar, which is why they are the rows the bar leaves out; the
/// application reads them back to put a date beside each one.
const MOVE_CARD: &[Binding] = &[
    Binding {
        keys: &[("t", Action::ToToday)],
        shown: "t",
        label: "Today",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("1", Action::Tomorrow)],
        shown: "1",
        label: "Tomorrow",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("2", Action::NextWorkDay)],
        shown: "2",
        label: "Next work day",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("3", Action::NextMonday)],
        shown: "3",
        label: "Next Monday",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("g", Action::GoToDate)],
        shown: "g",
        label: "Pick a date…",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("b", Action::ToBacklog)],
        shown: "b",
        label: "Backlog",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("up", Action::Up), ("down", Action::Down)],
        shown: "↑/↓",
        label: "move",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "move it there",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "move"),
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "cancel",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

/// The date card's picks, which are the rows of the card rather than of
/// the hint bar. They are on Alt because the field has the keyboard and
/// every letter and digit types there (DESIGN.md section 4); the same
/// keys work in the calendar so that a pick is one gesture wherever the
/// keyboard is.
macro_rules! date_picks {
    () => {
        [
            Binding {
                keys: &[("alt-1", Action::Tomorrow)],
                shown: "alt-1",
                label: "Tomorrow",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-2", Action::NextMonday)],
                shown: "alt-2",
                label: "Next Monday",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-3", Action::InAWeek)],
                shown: "alt-3",
                label: "In a week",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-4", Action::EndOfMonth)],
                shown: "alt-4",
                label: "End of month",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-0", Action::ClearDate)],
                shown: "alt-0",
                label: "Clear date",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            // Which date the card is setting, switched without losing
            // what has been typed.
            Binding {
                keys: &[("alt-d", Action::DueBy)],
                shown: "alt-d",
                label: "due by",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-r", Action::RemindOn)],
                shown: "alt-r",
                label: "remind on",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
        ]
    };
}

/// A date card table: its picks, then the keys of the control that has
/// the keyboard, then the two that leave.
macro_rules! date_table {
    ($($own:expr),* $(,)?) => {
        &[
            Binding {
                keys: &[("enter", Action::Confirm)],
                shown: "⏎",
                label: "set",
                bar: Bar::Left,
                narrow: Bar::Left,
            },
            Binding {
                keys: &[("esc", Action::Cancel)],
                shown: "esc",
                label: "cancel",
                bar: Bar::Left,
                narrow: Bar::Left,
            },
            $($own,)*
            date_picks!()[0],
            date_picks!()[1],
            date_picks!()[2],
            date_picks!()[3],
            date_picks!()[4],
            date_picks!()[5],
            date_picks!()[6],
        ]
    };
}

/// The date card while the field has the keyboard, which is how it opens.
const DATE_FIELD: &[Binding] = date_table![Binding {
    keys: &[("tab", Action::NextPane)],
    shown: "tab",
    label: "calendar",
    bar: Bar::Left,
    narrow: Bar::Left,
}];

/// The date card once `tab` has moved the keyboard into the calendar,
/// where single keys work again.
const DATE_CALENDAR: &[Binding] = date_table![
    Binding {
        keys: &[("tab", Action::NextPane)],
        shown: "tab",
        label: "type it",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[
            ("h", Action::Left),
            ("l", Action::Right),
            ("j", Action::Down),
            ("k", Action::Up),
            ("left", Action::Left),
            ("right", Action::Right),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "h/l/j/k",
        label: "day",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("<", Action::PrevMonth), (">", Action::NextMonth)],
        shown: "</>",
        label: "month",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
];

/// The repeat card. Its six shapes are rows of the card rather than of
/// the hint bar; the application reads them back to build a rule, and
/// `h`/`l` adjust whichever one is selected.
const REPEAT_CARD: &[Binding] = &[
    Binding {
        keys: &[("1", Action::EveryWorkDay)],
        shown: "1",
        label: "Every work day",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("2", Action::EveryDay)],
        shown: "2",
        label: "Every day",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("3", Action::EveryWeek)],
        shown: "3",
        label: "Every week on",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("4", Action::EveryMonth)],
        shown: "4",
        label: "Every month on the",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("5", Action::EveryFewWeeks)],
        shown: "5",
        label: "Every",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("0", Action::StopRepeat)],
        shown: "0",
        label: "Stop repeating",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "save",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "cancel",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[
            ("h", Action::Left),
            ("l", Action::Right),
            ("left", Action::Left),
            ("right", Action::Right),
        ],
        shown: "h/l",
        label: "adjust",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("space", Action::Pick)],
        shown: "space",
        label: "pick a day",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j/k",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

/// The one deliberate question (DESIGN.md section 8). Both answers are a
/// key of their own, because there is no default that is safe to guess.
const COPY_QUESTION: &[Binding] = &[
    Binding {
        keys: &[("1", Action::ThisCopy)],
        shown: "1",
        label: "this copy",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("2", Action::ThisAndFuture)],
        shown: "2",
        label: "this and future copies",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "and future"),
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "cancel",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

/// The question `x` asks while `confirm_delete` is on (DOMAIN.md section
/// 19). Enter is safe to be an answer here, unlike the copy question's:
/// the row is named in the card and keeping it is the key that backs out
/// of everything else.
const DELETE_QUESTION: &[Binding] = &[
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "delete",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "keep",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

/// The palette and search share a shape: a text field, a filtered list,
/// and the two keys that leave. Only what Enter does differs, and the
/// one key search has that the palette does not.
macro_rules! filter_box {
    ($($own:expr),* $(,)?) => {
        &[
            Binding {
                keys: &[],
                shown: "type",
                label: "to filter",
                bar: Bar::Left,
                narrow: Bar::Left,
            },
            Binding {
                keys: &[("up", Action::Up), ("down", Action::Down)],
                shown: "↑/↓",
                label: "move",
                bar: Bar::Left,
                narrow: Bar::Left,
            },
            $($own,)*
            Binding {
                keys: &[("esc", Action::Cancel)],
                shown: "esc",
                label: "close",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
        ]
    };
}

const PALETTE_BOX: &[Binding] = filter_box![Binding {
    keys: &[("enter", Action::Confirm)],
    shown: "⏎",
    label: "run",
    bar: Bar::Off,
    narrow: Bar::Off,
}];

/// Search finds tasks rather than commands, so Enter goes to one and
/// `alt-t` starts it again. Both are on the footer of the box rather
/// than in the bar, which the field's own two rows fill (wireframe 09).
const SEARCH_BOX: &[Binding] = filter_box![
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "go to that day",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    // The field has the keyboard, where every letter types, so the one
    // action it has of its own is on Alt (DESIGN.md section 4).
    Binding {
        keys: &[("alt-t", Action::ToToday)],
        shown: "alt-t",
        label: "re-add to today as a new task",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

const HELP_OVERLAY: &[Binding] = &[Binding {
    keys: &[("?", Action::Cancel), ("esc", Action::Cancel)],
    shown: "? or esc",
    label: "close",
    bar: Bar::Left,
    narrow: Bar::Left,
}];

/// The settings page. Its rows are settings rather than tasks, so a key
/// changes a value instead of acting on a row: `h` and `l` step it,
/// `space` and Enter change it, and on a row that holds a number or a
/// size Enter opens the field the value is typed into.
const SETTINGS_LIST: &[Binding] = &[
    Binding {
        keys: &[
            ("h", Action::Left),
            ("l", Action::Right),
            ("left", Action::Left),
            ("right", Action::Right),
        ],
        shown: "h/l",
        label: "adjust",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("space", Action::Pick), ("enter", Action::Confirm)],
        shown: "space ⏎",
        label: "change",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "change"),
    },
    // `,` is the key that opened the page, so it is also the key that
    // closes it; `esc` backs out of it the way it backs out of anything.
    Binding {
        keys: &[("esc", Action::Cancel), (",", Action::SettingsPage)],
        shown: "esc ,",
        label: "back",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j/k",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[(":", Action::Commands)],
        shown: ":",
        label: "commands",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("?", Action::Help)],
        shown: "?",
        label: "help",
        bar: Bar::Right,
        narrow: Bar::Short(Side::Right, "more"),
    },
    Binding {
        keys: &[("q", Action::Quit)],
        shown: "q",
        label: "quit",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

/// A number or a window size being typed on a settings row. Every letter
/// and digit types there, so the two keys that leave are the whole table.
const SETTINGS_FIELD: &[Binding] = &[
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "save",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "cancel",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

/// The rows of the key table for a context.
pub fn bindings(context: KeyContext) -> &'static [Binding] {
    match context {
        KeyContext::Home {
            field: Some(Field::Adding),
            ..
        } => HOME_ADDING,
        KeyContext::Home {
            field: Some(Field::Renaming),
            ..
        } => HOME_RENAMING,
        KeyContext::Home {
            pane: Pane::Day,
            day: Shown::Today,
            ..
        } => HOME_DAY,
        KeyContext::Home {
            pane: Pane::Day, ..
        } => HOME_OTHER_DAY,
        KeyContext::Home {
            pane: Pane::Backlog,
            day: Shown::Today,
            ..
        } => HOME_BACKLOG,
        KeyContext::Home {
            pane: Pane::Backlog,
            ..
        } => HOME_DAYS,
        KeyContext::Notes {
            pane: NotesPane::List,
            ..
        } => NOTES_LIST,
        KeyContext::Notes {
            pane: NotesPane::Note,
            ..
        } => NOTES_NOTE,
        // `e` on a review row opens the same field a task row opens
        // anywhere else, so Enter saves the title rather than moving the
        // review on to its next step.
        KeyContext::Review {
            text_field: true, ..
        } => HOME_RENAMING,
        // An outcome beside a row nobody is being asked about is a key
        // that would act on the wrong thing, so the step that asks
        // nothing offers none (DESIGN.md section 5).
        KeyContext::Review { asks: false, .. } => REVIEW_INFORMATION,
        KeyContext::Review {
            step: ReviewStep::Pile,
            ..
        } => REVIEW_PILE,
        KeyContext::Review {
            step: ReviewStep::Surfaced,
            ..
        } => REVIEW_SURFACED,
        KeyContext::Popup {
            kind: PopupKind::Palette,
            ..
        } => PALETTE_BOX,
        KeyContext::Popup {
            kind: PopupKind::Search,
            ..
        } => SEARCH_BOX,
        KeyContext::Popup {
            kind: PopupKind::Help,
            ..
        } => HELP_OVERLAY,
        KeyContext::Popup {
            kind: PopupKind::Move,
            ..
        } => MOVE_CARD,
        KeyContext::Popup {
            kind: PopupKind::CopyQuestion,
            ..
        } => COPY_QUESTION,
        KeyContext::Popup {
            kind: PopupKind::DeleteQuestion,
            ..
        } => DELETE_QUESTION,
        KeyContext::Popup {
            kind: PopupKind::Date,
            text_field: true,
        } => DATE_FIELD,
        KeyContext::Popup {
            kind: PopupKind::Date,
            text_field: false,
        } => DATE_CALENDAR,
        KeyContext::Popup {
            kind: PopupKind::Repeat,
            ..
        } => REPEAT_CARD,
        KeyContext::Settings { field: true } => SETTINGS_FIELD,
        KeyContext::Settings { field: false } => SETTINGS_LIST,
    }
}

/// What the hint bar calls the context it is showing.
pub fn name(context: KeyContext) -> &'static str {
    match context {
        KeyContext::Home {
            pane: Pane::Day,
            day: Shown::Today,
            ..
        } => "TODAY",
        KeyContext::Home {
            pane: Pane::Day,
            day: Shown::Past,
            ..
        } => "PAST DAY",
        KeyContext::Home {
            pane: Pane::Day,
            day: Shown::Future,
            ..
        } => "FUTURE DAY",
        KeyContext::Home {
            pane: Pane::Backlog,
            day: Shown::Today,
            ..
        } => "BACKLOG",
        KeyContext::Home {
            pane: Pane::Backlog,
            ..
        } => "DAYS",
        KeyContext::Notes {
            pane: NotesPane::List,
            ..
        } => "NOTES",
        KeyContext::Notes {
            pane: NotesPane::Note,
            ..
        } => "NOTE",
        KeyContext::Review {
            step: ReviewStep::Pile,
            ..
        } => "REVIEW",
        KeyContext::Review {
            step: ReviewStep::Surfaced,
            ..
        } => "SURFACED",
        KeyContext::Popup {
            kind: PopupKind::Palette,
            ..
        } => "COMMANDS",
        KeyContext::Popup {
            kind: PopupKind::Search,
            ..
        } => "SEARCH",
        KeyContext::Popup {
            kind: PopupKind::Help,
            ..
        } => "HELP",
        KeyContext::Popup {
            kind: PopupKind::Move,
            ..
        } => "MOVE",
        KeyContext::Popup {
            kind: PopupKind::CopyQuestion,
            ..
        } => "RENAME",
        KeyContext::Popup {
            kind: PopupKind::DeleteQuestion,
            ..
        } => "DELETE",
        KeyContext::Popup {
            kind: PopupKind::Date,
            ..
        } => "DATE",
        KeyContext::Popup {
            kind: PopupKind::Repeat,
            ..
        } => "REPEAT",
        KeyContext::Settings { .. } => "SETTINGS",
    }
}

// ---- dispatch --------------------------------------------------------

/// The keys a text field leaves alone (DESIGN.md section 4). Everything
/// else printable types.
const KEEPS_ITS_NAME: &[&str] = &["enter", "esc", "tab", "up", "down"];

/// The action a terminal event means in a context, if it means one.
pub fn action_for(event: &Event, context: KeyContext) -> Option<Action> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => key_action(key, context),
        Event::Mouse(mouse) => mouse_action(mouse),
        Event::Resize(_, _) => Some(Action::Resize),
        Event::FocusGained => Some(Action::FocusGained),
        _ => None,
    }
}

/// The one key that belongs to no context: it leaves from wherever the
/// keyboard is, a card or a field included, and puts the terminal back
/// (DESIGN.md section 4). It is a row of no table, and the help overlay
/// writes it from here so that it is not a hidden key.
pub const CTRL_C: Binding = Binding {
    keys: &[("ctrl-c", Action::Quit)],
    shown: "ctrl-c",
    label: "quit, from anywhere",
    bar: Bar::Off,
    narrow: Bar::Off,
};

fn key_action(key: &KeyEvent, context: KeyContext) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }
    if !context.text_field() {
        return bound(context, &key_name(key)?);
    }

    // The overlay: the editing keys first, then the few names a field
    // keeps, then typing.
    if let Some(action) = editing(key) {
        return Some(action);
    }
    let name = key_name(key)?;
    if KEEPS_ITS_NAME.contains(&name.as_str()) || name.starts_with("alt-") {
        return bound(context, &name);
    }
    typed(key).map(Action::Insert)
}

/// The row of the table that claims a key.
fn bound(context: KeyContext, name: &str) -> Option<Action> {
    bindings(context)
        .iter()
        .flat_map(|binding| binding.keys)
        .find(|(key, _)| *key == name)
        .map(|(_, action)| *action)
}

/// A key press as the table writes it: `a`, `J`, `space`, `alt-t`.
fn key_name(key: &KeyEvent) -> Option<String> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let name = match key.code {
        // The character already carries the shift; the modifier would say
        // it twice.
        KeyCode::Char(' ') => "space".to_owned(),
        KeyCode::Char(typed) => typed.to_string(),
        KeyCode::Enter => "enter".to_owned(),
        KeyCode::Esc => "esc".to_owned(),
        KeyCode::Tab | KeyCode::BackTab => "tab".to_owned(),
        KeyCode::Up => "up".to_owned(),
        KeyCode::Down => "down".to_owned(),
        KeyCode::Left => "left".to_owned(),
        KeyCode::Right => "right".to_owned(),
        KeyCode::Home => "home".to_owned(),
        KeyCode::End => "end".to_owned(),
        KeyCode::Backspace => "backspace".to_owned(),
        KeyCode::Delete => "delete".to_owned(),
        _ => return None,
    };
    Some(if alt { format!("alt-{name}") } else { name })
}

/// The keys that move and change a caret. They are the text field itself
/// rather than a row of the table, so they are never in the hint bar.
fn editing(key: &KeyEvent) -> Option<Action> {
    if key
        .modifiers
        .contains(KeyModifiers::ALT | KeyModifiers::CONTROL)
    {
        return None;
    }
    match key.code {
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Delete => Some(Action::DeleteForward),
        KeyCode::Left => Some(Action::Left),
        KeyCode::Right => Some(Action::Right),
        KeyCode::Home => Some(Action::LineStart),
        KeyCode::End => Some(Action::LineEnd),
        _ => None,
    }
}

/// The character a key press types, if it types one.
fn typed(key: &KeyEvent) -> Option<char> {
    if key
        .modifiers
        .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
    {
        return None;
    }
    match key.code {
        KeyCode::Char(typed) => Some(typed),
        _ => None,
    }
}

fn mouse_action(mouse: &MouseEvent) -> Option<Action> {
    let (column, row) = (mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(_) => Some(Action::MouseDown { column, row }),
        MouseEventKind::Up(_) => Some(Action::MouseUp { column, row }),
        MouseEventKind::Drag(_) => Some(Action::MouseDrag { column, row }),
        MouseEventKind::ScrollDown => Some(Action::Scroll {
            column,
            row,
            down: true,
        }),
        MouseEventKind::ScrollUp => Some(Action::Scroll {
            column,
            row,
            down: false,
        }),
        _ => None,
    }
}
