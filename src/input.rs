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

/// Where the keyboard is on the notes page: the list, the note open
/// beside it, or the filter typed at the top of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotesPane {
    List,
    Note,
    Filter,
}

/// Which list the left pane of the notes page shows. `tab` switches
/// between them (DESIGN.md section 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotesList {
    Notes,
    Archive,
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
    /// The spelling card: what the dictionary offers in place of the
    /// misspelt word the caret of an open note is in, and the offer to
    /// keep the word instead.
    Spelling,
    /// The personal dictionary: the words spell checking is told to
    /// know, opened from the notes group of the settings page.
    Dictionary,
}

/// The in-place text field on the home page, which is the only place a
/// task's title is written. Which one it is decides what Enter is called,
/// with Shift+Enter offered only when adding.
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
    /// `narrow` is the window collapsed to tabs, where `tab` steps
    /// through them and `h`/`l` mean nothing, rather than two panes side
    /// by side, where `h`/`l` switch pane and `tab` means nothing.
    Home {
        pane: Pane,
        day: Shown,
        field: Option<Field>,
        narrow: bool,
    },
    /// On the Archive, `tab` goes back to Notes in a wide window and on
    /// to the next tab in a narrow one, which is what `narrow` is for.
    Notes {
        pane: NotesPane,
        list: NotesList,
        text_field: bool,
        narrow: bool,
    },
    Review {
        step: ReviewStep,
        /// Whether this is the last step with something in it, where
        /// Enter starts the day rather than going on to the next step.
        last: bool,
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
    /// `g` has been pressed and the next key says where to go. `over` is
    /// the card it was pressed in, where only the first row, and on the
    /// move card a date, are places to go; on a page it is `None`.
    Leader {
        over: Option<PopupKind>,
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
            KeyContext::Leader { .. } => false,
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
    /// `g`, the leader: the key after it says where to go.
    Go,
    /// `gg` and `G`: the first and the last row of the list.
    First,
    Last,
    /// `ctrl-d` and `ctrl-u`: half of what the list shows, down or up.
    HalfPageDown,
    HalfPageUp,
    /// `gb`: the backlog beside today.
    BacklogPane,
    /// `ga`: the notes page on its archive.
    ArchivePage,
    PaneLeft,
    PaneRight,
    NextPane,
    /// `tab` on a page: the next tab of a narrow window, or the other
    /// list of the notes page.
    NextTab,
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
    CopyTask,
    CopyNote,
    /// `A` on the notes page: the note to the archive, or back from it.
    Archive,
    CopySelection,
    CutSelection,
    Paste,
    /// The misspelt word the caret of an open note is in, and what the
    /// dictionary offers in place of it. It acts on a word rather than
    /// on a row, which is why it is the note's key and no list's.
    FixSpelling,
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
    EndOfWeek,
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
    UndoText,
    RedoText,
    /// The two answers to the copy question.
    ThisCopy,
    ThisAndFuture,

    // Popups.
    Search,
    /// `/` on the notes page, which narrows the list shown rather than
    /// searching the tasks.
    Filter,
    Commands,
    Help,
    Confirm,
    AddAndContinue,
    Cancel,

    // Text fields.
    Insert(char),
    Backspace,
    DeleteWordBackward,
    DeleteForward,
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    SelectLeft,
    SelectRight,
    SelectUp,
    SelectDown,
    SelectWordLeft,
    SelectWordRight,
    SelectStart,
    SelectEnd,
    SelectAll,

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
    /// How the keys are written when the row is named: `J/K`, `y/alt-y`,
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
                    | Action::Archive
                    | Action::CopyTask
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
        panes: $panes:expr;
        $($own:expr),* $(,)?
    ) => {
        &[
            Binding {
                keys: &[("[", Action::PrevDay), ("]", Action::NextDay)],
                shown: "[/]",
                label: "prev/next day",
                bar: $steps,
                narrow: $steps_narrow,
            },
            $($own,)*
            $panes,
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
            GO,
            LAST_ROW,
            HALF_PAGE,
            SEARCH,
            COMMANDS,
            HELP,
            QUIT,
        ]
    };
}

/// Both widths of a home table: `h` and `l` switch pane when both are
/// on screen, and `tab` steps through the tabs when they have collapsed
/// (DESIGN.md section 4).
macro_rules! home_tables {
    (
        $(#[$doc:meta])*
        $wide:ident, $narrow:ident;
        steps: $steps:expr, $steps_narrow:expr;
        $($own:expr),* $(,)?
    ) => {
        $(#[$doc])*
        const $wide: &[Binding] = home_table![
            steps: $steps, $steps_narrow;
            panes: PANES;
            $($own),*
        ];
        $(#[$doc])*
        const $narrow: &[Binding] = home_table![
            steps: $steps, $steps_narrow;
            panes: TABS;
            $($own),*
        ];
    };
}

/// The leader. What can follow it is the table of `KeyContext::Leader`,
/// which the hint bar shows while it waits, so the one key a page has to
/// name for every place there is to go is this one.
const GO: Binding = Binding {
    keys: &[("g", Action::Go)],
    shown: "g",
    label: "go…",
    bar: Bar::Right,
    narrow: Bar::Off,
};

/// `G`, the partner of `gg`, on the page because it is a key pressed on
/// its own rather than a place under the leader.
const LAST_ROW: Binding = Binding {
    keys: &[("G", Action::Last)],
    shown: "G",
    label: "last row",
    bar: Bar::Off,
    narrow: Bar::Off,
};

const HALF_PAGE: Binding = Binding {
    keys: &[
        ("ctrl-d", Action::HalfPageDown),
        ("ctrl-u", Action::HalfPageUp),
    ],
    shown: "ctrl-d/u",
    label: "half a page",
    bar: Bar::Off,
    narrow: Bar::Off,
};

const SEARCH: Binding = Binding {
    keys: &[("/", Action::Search)],
    shown: "/",
    label: "search",
    bar: Bar::Off,
    narrow: Bar::Off,
};

const COMMANDS: Binding = Binding {
    keys: &[(":", Action::Commands)],
    shown: ":",
    label: "commands",
    bar: Bar::Off,
    narrow: Bar::Off,
};

const HELP: Binding = Binding {
    keys: &[("?", Action::Help)],
    shown: "?",
    label: "help",
    bar: Bar::Off,
    narrow: Bar::Short(Side::Right, "more"),
};

const QUIT: Binding = Binding {
    keys: &[("q", Action::Quit)],
    shown: "q",
    label: "quit",
    bar: Bar::Off,
    narrow: Bar::Off,
};

/// The way home from another day, which is a place under the leader and
/// so a line of the bar rather than a key of the table: stepping through
/// days is what the pane is for, and coming back is the next thing to
/// know (wireframe 08).
const BACK_TO_TODAY: Binding = Binding {
    keys: &[],
    shown: "gt",
    label: "today",
    bar: Bar::Left,
    // The narrow bar calls it "back", because `t today` is beside it and
    // means something else.
    narrow: Bar::Short(Side::Left, "back"),
};

const GO_TO_A_DATE: Binding = Binding {
    keys: &[],
    shown: "gd",
    label: "go to date",
    bar: Bar::Left,
    narrow: Bar::Off,
};

/// The leader in a card, where it leads only to the first row and the
/// card's hint bar has no room to name it.
const QUIET_GO: Binding = Binding {
    bar: Bar::Off,
    narrow: Bar::Off,
    ..GO
};

/// Where `g` leads from a page (DESIGN.md section 4). Every place the
/// program has is one of these, and none of them has a key of its own.
const AFTER_G: &[Binding] = &[
    Binding {
        keys: &[("g", Action::First)],
        shown: "g",
        label: "first row",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("t", Action::Today)],
        shown: "t",
        label: "today",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("b", Action::BacklogPane)],
        shown: "b",
        label: "backlog",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("d", Action::GoToDate)],
        shown: "d",
        label: "go to date",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "date"),
    },
    Binding {
        keys: &[("n", Action::NotesPage)],
        shown: "n",
        label: "notes",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("a", Action::ArchivePage)],
        shown: "a",
        label: "archive",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("s", Action::SettingsPage)],
        shown: "s",
        label: "settings",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("r", Action::OpenReview)],
        shown: "r",
        label: "morning review",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    GO_NOWHERE,
];

/// Where `g` leads in the move card: its first row, or a date typed or
/// walked to in the calendar.
const AFTER_G_ON_THE_MOVE_CARD: &[Binding] = &[AFTER_G[0], AFTER_G[3], GO_NOWHERE];

/// Where `g` leads in a card that is a list and nothing else.
const AFTER_G_IN_A_LIST: &[Binding] = &[AFTER_G[0], GO_NOWHERE];

const GO_NOWHERE: Binding = Binding {
    keys: &[("esc", Action::Cancel)],
    shown: "esc",
    label: "cancel",
    bar: Bar::Right,
    narrow: Bar::Right,
};

/// Two panes side by side.
const PANES: Binding = Binding {
    keys: &[("h", Action::PaneLeft), ("l", Action::PaneRight)],
    shown: "h/l",
    label: "pane",
    bar: Bar::Right,
    narrow: Bar::Off,
};

/// The tabs a narrow window collapses to, which go round: Today,
/// Backlog, Notes, Archive and Today again.
const TABS: Binding = Binding {
    keys: &[("tab", Action::NextTab)],
    shown: "tab",
    label: "next tab",
    bar: Bar::Off,
    narrow: Bar::Off,
};

home_tables![
    HOME_DAY, HOME_DAY_NARROW;
    steps: Bar::Off, Bar::Off;
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
    Binding {
        keys: &[("y", Action::CopyTask)],
        shown: "y",
        label: "copy task",
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

home_tables![
    HOME_BACKLOG, HOME_BACKLOG_NARROW;
    steps: Bar::Off, Bar::Off;
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
        label: "move to day…",
        bar: Bar::Short(Side::Left, "move…"),
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
    Binding {
        keys: &[("y", Action::CopyTask)],
        shown: "y",
        label: "copy task",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

home_tables![
    /// A day that is not today. Stepping is what the pane is for, so the
    /// day keys lead the bar; `t` puts a task from a day that has passed
    /// onto today, which on today itself would mean nothing (wireframe
    /// 08).
    HOME_OTHER_DAY, HOME_OTHER_DAY_NARROW;
    steps: Bar::Short(Side::Left, "day"), Bar::Short(Side::Left, "day");
    BACK_TO_TODAY,
    GO_TO_A_DATE,
    Binding {
        keys: &[("space", Action::Close)],
        shown: "space",
        label: "done",
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
        label: "move to day…",
        bar: Bar::Short(Side::Left, "move…"),
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
        label: "add",
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
    Binding {
        keys: &[("y", Action::CopyTask)],
        shown: "y",
        label: "copy task",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
];

home_tables![
    /// The list of days the backlog pane becomes while history is
    /// browsed. Its rows are days, so nothing that acts on a task is
    /// bound here.
    HOME_DAYS, HOME_DAYS_NARROW;
    steps: Bar::Short(Side::Left, "day"), Bar::Short(Side::Left, "day");
    BACK_TO_TODAY,
    GO_TO_A_DATE,
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "go to that day",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

/// A notes list table: the rows the two lists share around the one key
/// that differs between them, `A`, and what `tab` goes on to.
macro_rules! notes_table {
    ($archive:expr, $tab:expr) => {
        &[
            Binding {
                keys: &[("y", Action::CopyNote), ("alt-y", Action::CopyNote)],
                shown: "y/alt-y",
                label: "copy note",
                bar: Bar::Left,
                narrow: Bar::Left,
            },
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
                label: "add",
                bar: Bar::Left,
                narrow: Bar::Left,
            },
            $archive,
            Binding {
                keys: &[("x", Action::Delete)],
                shown: "x",
                label: "delete",
                bar: Bar::Left,
                narrow: Bar::Short(Side::Left, "del"),
            },
            // Escape backs out of the page, one level towards today.
            Binding {
                keys: &[("esc", Action::Cancel)],
                shown: "esc",
                label: "back to tasks",
                bar: Bar::Left,
                narrow: Bar::Short(Side::Left, "back"),
            },
            $tab,
            Binding {
                keys: &[("h", Action::PaneLeft), ("l", Action::PaneRight)],
                shown: "h/l",
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
            GO,
            LAST_ROW,
            HALF_PAGE,
            // The notes page filters its own list rather than searching
            // the tasks (DESIGN.md section 9).
            Binding {
                keys: &[("/", Action::Filter)],
                shown: "/",
                label: "filter",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            COMMANDS,
            HELP,
            QUIT,
        ]
    };
}

const ARCHIVE_NOTE: Binding = Binding {
    keys: &[("A", Action::Archive)],
    shown: "A",
    label: "archive note",
    bar: Bar::Short(Side::Left, "archive"),
    narrow: Bar::Off,
};

const UNARCHIVE_NOTE: Binding = Binding {
    keys: &[("A", Action::Archive)],
    shown: "A",
    label: "unarchive note",
    bar: Bar::Short(Side::Left, "unarchive"),
    narrow: Bar::Off,
};

/// `tab` from Notes, at either width, is the Archive.
const TO_THE_ARCHIVE: Binding = Binding {
    keys: &[("tab", Action::NextTab)],
    shown: "tab",
    label: "archive",
    bar: Bar::Right,
    narrow: Bar::Off,
};

/// `tab` from the Archive is Notes when the list is a pane of its own,
/// and the next tab, Today, when the window has collapsed to tabs.
const BACK_TO_THE_NOTES: Binding = Binding {
    keys: &[("tab", Action::NextTab)],
    shown: "tab",
    label: "notes",
    bar: Bar::Right,
    narrow: Bar::Off,
};

const ON_TO_TODAY: Binding = Binding {
    keys: &[("tab", Action::NextTab)],
    shown: "tab",
    label: "next tab",
    bar: Bar::Off,
    narrow: Bar::Off,
};

const NOTES_LIST: &[Binding] = notes_table!(ARCHIVE_NOTE, TO_THE_ARCHIVE);
const ARCHIVE_LIST: &[Binding] = notes_table!(UNARCHIVE_NOTE, BACK_TO_THE_NOTES);
const ARCHIVE_LIST_NARROW: &[Binding] = notes_table!(UNARCHIVE_NOTE, ON_TO_TODAY);

/// The filter at the top of the notes list while it has the keyboard.
/// Every letter types into it; the list under it still moves and opens,
/// and `tab` hands the keyboard to the list, where single keys work
/// again with the filter still applied (DESIGN.md section 4).
const NOTES_FILTER: &[Binding] = &[
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
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "open",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("tab", Action::NextPane)],
        shown: "tab",
        label: "to the list",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "list"),
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "clear",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
];

const NOTES_NOTE: &[Binding] = &[
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "back to the list",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "back"),
    },
    Binding {
        keys: &[("ctrl-z", Action::UndoText)],
        shown: "ctrl-z",
        label: "undo edit",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("ctrl-y", Action::RedoText)],
        shown: "ctrl-y",
        label: "redo edit",
        bar: Bar::Left,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("alt-h", Action::Help)],
        shown: "alt-h",
        label: "help",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("alt-y", Action::CopyNote)],
        shown: "alt-y",
        label: "copy note",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    // Every letter types here, so the one key that acts on a word is on
    // Alt, the way search's `alt-t` and the date card's picks are
    // (DESIGN.md section 4).
    Binding {
        keys: &[("alt-s", Action::FixSpelling)],
        shown: "alt-s",
        label: "fix spelling…",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "spelling"),
    },
    Binding {
        keys: &[],
        shown: "type",
        label: "to edit",
        bar: Bar::Left,
        narrow: Bar::Left,
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
];

/// The keys every page has, in the review, where the bar has room only
/// for the decisions and the one thing to press, so none of them is in
/// it; the palette and the help overlay teach them.
const REVIEW_GLOBALS: [Binding; 7] = [
    Binding {
        bar: Bar::Off,
        ..GO
    },
    LAST_ROW,
    HALF_PAGE,
    SEARCH,
    COMMANDS,
    Binding {
        narrow: Bar::Off,
        ..HELP
    },
    QUIT,
];

/// The pile, with what Enter is called there: the next step while one
/// is to come, and the start of the day on the last.
macro_rules! review_pile {
    ($enter:expr) => {
        &[
            Binding {
                keys: &[("space", Action::Close)],
                shown: "space",
                label: "done",
                bar: Bar::Left,
                narrow: Bar::Left,
            },
            Binding {
                keys: &[("t", Action::ToToday)],
                shown: "t",
                label: "to today",
                bar: Bar::Short(Side::Left, "today"),
                narrow: Bar::Short(Side::Left, "today"),
            },
            Binding {
                keys: &[("b", Action::ToBacklog)],
                shown: "b",
                label: "to backlog",
                bar: Bar::Short(Side::Left, "backlog"),
                narrow: Bar::Short(Side::Left, "backlog"),
            },
            Binding {
                keys: &[("m", Action::MoveToDay)],
                shown: "m",
                label: "move to day…",
                bar: Bar::Short(Side::Left, "move…"),
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
                label: $enter,
                bar: Bar::Right,
                narrow: Bar::Right,
            },
            Binding {
                keys: &[("esc", Action::Cancel)],
                shown: "esc",
                label: "skip for now",
                bar: Bar::Short(Side::Right, "skip"),
                narrow: Bar::Short(Side::Right, "skip"),
            },
            // Review navigation uses the same keys as other lists.
            Binding {
                keys: &[
                    ("j", Action::Down),
                    ("k", Action::Up),
                    ("down", Action::Down),
                    ("up", Action::Up),
                ],
                shown: "j/k ↑/↓",
                label: "move",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            REVIEW_GLOBALS[0],
            REVIEW_GLOBALS[1],
            REVIEW_GLOBALS[2],
            REVIEW_GLOBALS[3],
            REVIEW_GLOBALS[4],
            REVIEW_GLOBALS[5],
            REVIEW_GLOBALS[6],
        ]
    };
}

const REVIEW_PILE: &[Binding] = review_pile!("next step");
const REVIEW_PILE_LAST: &[Binding] = review_pile!("start the day");

const REVIEW_SURFACED: &[Binding] = &[
    Binding {
        keys: &[("t", Action::ToToday)],
        shown: "t",
        label: "to today",
        bar: Bar::Short(Side::Left, "today"),
        narrow: Bar::Short(Side::Left, "today"),
    },
    Binding {
        keys: &[("s", Action::Keep)],
        shown: "s",
        label: "leave in backlog",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("d", Action::DueBy)],
        shown: "d",
        label: "due by",
        bar: Bar::Short(Side::Left, "due…"),
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("r", Action::RemindOn)],
        shown: "r",
        label: "remind on",
        bar: Bar::Short(Side::Left, "remind…"),
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
        label: "skip for now",
        bar: Bar::Short(Side::Right, "skip"),
        narrow: Bar::Short(Side::Right, "skip"),
    },
    Binding {
        keys: &[
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j/k ↑/↓",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    REVIEW_GLOBALS[0],
    REVIEW_GLOBALS[1],
    REVIEW_GLOBALS[2],
    REVIEW_GLOBALS[3],
    REVIEW_GLOBALS[4],
    REVIEW_GLOBALS[5],
    REVIEW_GLOBALS[6],
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
        label: "skip for now",
        bar: Bar::Short(Side::Right, "skip"),
        narrow: Bar::Short(Side::Right, "skip"),
    },
    // Navigation stays available even when there is nothing to decide.
    Binding {
        keys: &[
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j/k ↑/↓",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    REVIEW_GLOBALS[0],
    REVIEW_GLOBALS[1],
    REVIEW_GLOBALS[2],
    REVIEW_GLOBALS[3],
    REVIEW_GLOBALS[4],
    REVIEW_GLOBALS[5],
    REVIEW_GLOBALS[6],
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
        key: "space",
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
        key: "s",
        action: Action::Keep,
        label: "Leave in backlog",
        note: "unchanged",
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

/// The in-place field on a task row. Shift+Enter keeps adding; Enter closes it.
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
        label: "add",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "add"),
    },
    Binding {
        keys: &[("shift-enter", Action::AddAndContinue)],
        shown: "shift+⏎",
        label: "add & keep typing",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "add more"),
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
    // The digits are the date card's picks, so a digit is the same day on
    // both cards; the next work day, which only a move asks for, is on
    // its own letter.
    Binding {
        keys: &[("1", Action::Tomorrow)],
        shown: "1",
        label: "Tomorrow",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("w", Action::NextWorkDay)],
        shown: "w",
        label: "Next work day",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("2", Action::EndOfWeek)],
        shown: "2",
        label: "End of week",
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
        keys: &[("4", Action::InAWeek)],
        shown: "4",
        label: "In a week",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    Binding {
        keys: &[("5", Action::EndOfMonth)],
        shown: "5",
        label: "End of month",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    // A date is `gd` wherever one is gone to, so the row is the leader,
    // written as the whole chord.
    Binding {
        keys: &[("g", Action::Go)],
        shown: "gd",
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
        keys: &[
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j/k",
        label: "move",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    LAST_ROW,
    HALF_PAGE,
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
                keys: &[("alt-2", Action::EndOfWeek)],
                shown: "alt-2",
                label: "End of week",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-3", Action::NextMonday)],
                shown: "alt-3",
                label: "Next Monday",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-4", Action::InAWeek)],
                shown: "alt-4",
                label: "In a week",
                bar: Bar::Off,
                narrow: Bar::Off,
            },
            Binding {
                keys: &[("alt-5", Action::EndOfMonth)],
                shown: "alt-5",
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
    QUIET_GO,
    LAST_ROW,
    HALF_PAGE,
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
            HALF_PAGE,
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

/// The spelling card: the words the dictionary offers in place of a
/// misspelt one, and under them the offer to keep the word instead. It
/// is a list that is chosen from and answered, so its keys are the keys
/// every card's list has; nothing here is on Alt, because the note under
/// it has given the keyboard up while it stands.
///
/// Enter is called "choose" rather than "replace" because the last row
/// replaces nothing: it adds the word to the personal dictionary. Each
/// row says which of the two it is.
const SPELLING_CARD: &[Binding] = &[
    Binding {
        keys: &[
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ],
        shown: "j/k",
        label: "move",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("enter", Action::Confirm)],
        shown: "⏎",
        label: "choose",
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
    QUIET_GO,
    LAST_ROW,
    HALF_PAGE,
];

/// The personal dictionary: the words the checker is told to know, as a
/// list with the three keys a list of rows anywhere else has. `a`, `e`
/// and `x` mean here what they mean on the notes list, so there is one
/// idea of adding, changing and removing a row to learn.
const DICTIONARY_LIST: &[Binding] = &[
    Binding {
        keys: &[("up", Action::Up), ("down", Action::Down)],
        shown: "↑/↓",
        label: "move",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("a", Action::Add)],
        shown: "a",
        label: "add a word",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "add"),
    },
    // Enter on a row of words is the row itself, which is the word to
    // write again; there is nothing else for it to open.
    Binding {
        keys: &[("e", Action::Edit), ("enter", Action::Confirm)],
        shown: "e ⏎",
        label: "change",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("x", Action::Delete)],
        shown: "x",
        label: "remove",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "del"),
    },
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
        label: "back to settings",
        bar: Bar::Left,
        narrow: Bar::Short(Side::Left, "back"),
    },
    Binding {
        keys: &[("j", Action::Down), ("k", Action::Up)],
        shown: "j/k",
        label: "move",
        bar: Bar::Off,
        narrow: Bar::Off,
    },
    QUIET_GO,
    LAST_ROW,
    HALF_PAGE,
];

/// A word being typed into the dictionary. Every letter types there, so
/// the two keys that leave are the whole table: `x` in a field is an
/// `x`, and no word is removed while one is being written.
const DICTIONARY_FIELD: &[Binding] = &[
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

const HELP_OVERLAY: &[Binding] = &[
    Binding {
        keys: &[("?", Action::Cancel), ("esc", Action::Cancel)],
        shown: "? or esc",
        label: "close",
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
        shown: "↑/↓",
        label: "scroll",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    Binding {
        keys: &[("tab", Action::NextPane)],
        shown: "tab",
        label: "current/all keys",
        bar: Bar::Left,
        narrow: Bar::Left,
    },
    QUIET_GO,
    LAST_ROW,
    HALF_PAGE,
];

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
    // Escape backs out of the page the way it backs out of anything.
    Binding {
        keys: &[("esc", Action::Cancel)],
        shown: "esc",
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
    GO,
    LAST_ROW,
    HALF_PAGE,
    SEARCH,
    COMMANDS,
    Binding {
        bar: Bar::Right,
        ..HELP
    },
    QUIT,
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
            pane, day, narrow, ..
        } => match (pane, day == Shown::Today, narrow) {
            (Pane::Day, true, false) => HOME_DAY,
            (Pane::Day, true, true) => HOME_DAY_NARROW,
            (Pane::Day, false, false) => HOME_OTHER_DAY,
            (Pane::Day, false, true) => HOME_OTHER_DAY_NARROW,
            (Pane::Backlog, true, false) => HOME_BACKLOG,
            (Pane::Backlog, true, true) => HOME_BACKLOG_NARROW,
            (Pane::Backlog, false, false) => HOME_DAYS,
            (Pane::Backlog, false, true) => HOME_DAYS_NARROW,
        },
        KeyContext::Notes {
            pane: NotesPane::List,
            list,
            narrow,
            ..
        } => match (list, narrow) {
            (NotesList::Notes, _) => NOTES_LIST,
            (NotesList::Archive, false) => ARCHIVE_LIST,
            (NotesList::Archive, true) => ARCHIVE_LIST_NARROW,
        },
        KeyContext::Notes {
            pane: NotesPane::Note,
            ..
        } => NOTES_NOTE,
        KeyContext::Notes {
            pane: NotesPane::Filter,
            ..
        } => NOTES_FILTER,
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
            last: false,
            ..
        } => REVIEW_PILE,
        KeyContext::Review {
            step: ReviewStep::Pile,
            ..
        } => REVIEW_PILE_LAST,
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
        KeyContext::Popup {
            kind: PopupKind::Spelling,
            ..
        } => SPELLING_CARD,
        KeyContext::Popup {
            kind: PopupKind::Dictionary,
            text_field: true,
        } => DICTIONARY_FIELD,
        KeyContext::Popup {
            kind: PopupKind::Dictionary,
            text_field: false,
        } => DICTIONARY_LIST,
        KeyContext::Settings { field: true } => SETTINGS_FIELD,
        KeyContext::Settings { field: false } => SETTINGS_LIST,
        KeyContext::Leader { over: None } => AFTER_G,
        KeyContext::Leader {
            over: Some(PopupKind::Move),
        } => AFTER_G_ON_THE_MOVE_CARD,
        KeyContext::Leader { over: Some(_) } => AFTER_G_IN_A_LIST,
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
            list: NotesList::Notes,
            ..
        } => "NOTES",
        KeyContext::Notes {
            pane: NotesPane::List,
            list: NotesList::Archive,
            ..
        } => "ARCHIVE",
        KeyContext::Notes {
            pane: NotesPane::Note,
            ..
        } => "NOTE",
        KeyContext::Notes {
            pane: NotesPane::Filter,
            ..
        } => "FILTER",
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
        KeyContext::Popup {
            kind: PopupKind::Spelling,
            ..
        } => "SPELLING",
        KeyContext::Popup {
            kind: PopupKind::Dictionary,
            ..
        } => "DICTIONARY",
        KeyContext::Settings { .. } => "SETTINGS",
        KeyContext::Leader { .. } => "GO",
    }
}

// ---- dispatch --------------------------------------------------------

/// The keys a text field leaves alone (DESIGN.md section 4). Everything
/// else printable types.
const KEEPS_ITS_NAME: &[&str] = &["enter", "esc", "tab", "up", "down"];

/// How the key tables write a key press, for a press that means nothing
/// where it was made; anything that is not a key press has no name.
pub fn pressed(event: &Event) -> Option<String> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => key_name(key),
        _ => None,
    }
}

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
    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }
    if key.code == KeyCode::Enter
        && key.modifiers == KeyModifiers::SHIFT
        && let Some(action) = bound(context, "shift-enter")
    {
        return Some(action);
    }
    if !context.text_field() {
        return bound(context, &key_name(key)?);
    }

    if key.modifiers == KeyModifiers::CONTROL {
        match key.code {
            KeyCode::Char('z') => return bound(context, "ctrl-z"),
            KeyCode::Char('y') => return bound(context, "ctrl-y"),
            _ => {}
        }
    }
    // The overlay: the editing keys first, then the few names a field
    // keeps, then typing.
    if let Some(action) = editing(key) {
        return Some(action);
    }
    let name = key_name(key)?;
    if KEEPS_ITS_NAME.contains(&name.as_str())
        || name.starts_with("alt-")
        || name.starts_with("ctrl-")
    {
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
    // The two page motions are the only control keys a table names; the
    // rest belong to the text fields or to nobody.
    if key.modifiers == KeyModifiers::CONTROL {
        return match key.code {
            KeyCode::Char('d') => Some("ctrl-d".to_owned()),
            KeyCode::Char('u') => Some("ctrl-u".to_owned()),
            _ => None,
        };
    }
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
    if key.modifiers == KeyModifiers::ALT && key.code == KeyCode::Char('y') {
        return Some(Action::CopyNote);
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    if key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT
        && let Some(action) = match key.code {
            KeyCode::Char('c' | 'C') | KeyCode::Insert => Some(Action::CopySelection),
            KeyCode::Char('x' | 'X') => Some(Action::CutSelection),
            KeyCode::Char('v' | 'V') => Some(Action::Paste),
            _ => None,
        }
    {
        return Some(action);
    }
    if key.modifiers == KeyModifiers::SHIFT && key.code == KeyCode::Insert {
        return Some(Action::Paste);
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        return match (key.code, key.modifiers.contains(KeyModifiers::CONTROL)) {
            (KeyCode::Left, true) => Some(Action::SelectWordLeft),
            (KeyCode::Right, true) => Some(Action::SelectWordRight),
            (KeyCode::Left, false) => Some(Action::SelectLeft),
            (KeyCode::Right, false) => Some(Action::SelectRight),
            (KeyCode::Up, false) => Some(Action::SelectUp),
            (KeyCode::Down, false) => Some(Action::SelectDown),
            (KeyCode::Home, false) => Some(Action::SelectStart),
            (KeyCode::End, false) => Some(Action::SelectEnd),
            _ => None,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('a') => Some(Action::SelectAll),
            KeyCode::Char('x') => Some(Action::CutSelection),
            KeyCode::Char('v') => Some(Action::Paste),
            KeyCode::Insert => Some(Action::CopySelection),
            KeyCode::Left => Some(Action::WordLeft),
            KeyCode::Right => Some(Action::WordRight),
            // Legacy terminals encode Ctrl+Backspace as BS (Ctrl+H).
            KeyCode::Backspace | KeyCode::Char('h') => Some(Action::DeleteWordBackward),
            _ => None,
        };
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
