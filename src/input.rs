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
    /// The one deliberate question: whether a recurring copy's new title
    /// is for this copy or for this and future copies.
    CopyQuestion,
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
    Home { pane: Pane, field: Option<Field> },
    Notes { pane: NotesPane, text_field: bool },
    Review { step: ReviewStep, text_field: bool },
    Popup { kind: PopupKind, text_field: bool },
}

impl KeyContext {
    /// Whether a text field has the keyboard.
    pub fn text_field(self) -> bool {
        match self {
            KeyContext::Home { field, .. } => field.is_some(),
            KeyContext::Notes { text_field, .. }
            | KeyContext::Review { text_field, .. }
            | KeyContext::Popup { text_field, .. } => text_field,
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
    Waiting,
    Repeat,
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
    MouseDown { column: u16, row: u16 },
    MouseUp { column: u16, row: u16 },
    MouseDrag { column: u16, row: u16 },
    Scroll { column: u16, row: u16, down: bool },
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
    /// overlay.
    pub label: &'static str,
    /// Where the hint bar puts it when both panes are on screen.
    pub bar: Bar,
    /// Where it puts it when the window has collapsed to tabs and the bar
    /// has room for about five keys.
    pub narrow: Bar,
}

// ---- the key table ---------------------------------------------------
//
// One table per context. The order of the rows is the order of the hint
// bar; a row the bar leaves out still teaches its key in the palette and
// the help overlay.

/// A home table: its own rows, then switching pane, then the day keys and
/// the rows every page shares. One macro rather than one const per group,
/// because [`bindings`] hands out a single slice and the order of the rows
/// is the order of the hint bar.
macro_rules! home_table {
    ($($own:expr),* $(,)?) => {
        &[
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
                keys: &[("[", Action::PrevDay), ("]", Action::NextDay)],
                shown: "[ ]",
                label: "prev/next day",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[(".", Action::Today)],
                shown: ".",
                label: "today",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("g", Action::GoToDate)],
                shown: "g",
                label: "go to date",
                bar: Bar::Off,
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
];

const HOME_BACKLOG: &[Binding] = home_table![
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
    Binding {
        keys: &[("n", Action::NotesPage)],
        shown: "n",
        label: "back to today",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "back"),
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
    Binding {
        keys: &[("tab", Action::NextPane)],
        shown: "tab",
        label: "pane",
        bar: Bar::Off,
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
        keys: &[("k", Action::Keep)],
        shown: "k",
        label: "keep",
        bar: Bar::Off,
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
        keys: &[("q", Action::Quit)],
        shown: "q",
        label: "quit",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

/// The in-place field on a task row. Adding keeps the field open after
/// Enter so that a list is typed in one go; renaming closes it.
const HOME_ADDING: &[Binding] = &[
    Binding {
        keys: &[],
        shown: "type",
        label: "the text is the whole task",
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
        keys: &[],
        shown: "type",
        label: "the text is the whole task",
        bar: Bar::Left,
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

/// The palette and search share a shape: a text field, a filtered list,
/// and the two keys that leave.
const FILTER_BOX: &[Binding] = &[
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
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "run",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "close",
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
            pane: Pane::Day, ..
        } => HOME_DAY,
        KeyContext::Home {
            pane: Pane::Backlog,
            ..
        } => HOME_BACKLOG,
        KeyContext::Notes {
            pane: NotesPane::List,
            ..
        } => NOTES_LIST,
        KeyContext::Notes {
            pane: NotesPane::Note,
            ..
        } => NOTES_NOTE,
        KeyContext::Review {
            step: ReviewStep::Pile,
            ..
        } => REVIEW_PILE,
        KeyContext::Review {
            step: ReviewStep::Surfaced,
            ..
        } => REVIEW_SURFACED,
        KeyContext::Popup {
            kind: PopupKind::Palette | PopupKind::Search,
            ..
        } => FILTER_BOX,
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
    }
}

/// What the hint bar calls the context it is showing.
pub fn name(context: KeyContext) -> &'static str {
    match context {
        KeyContext::Home {
            pane: Pane::Day, ..
        } => "TODAY",
        KeyContext::Home {
            pane: Pane::Backlog,
            ..
        } => "BACKLOG",
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

fn key_action(key: &KeyEvent, context: KeyContext) -> Option<Action> {
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
