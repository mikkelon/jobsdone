//! The fake data the shell is drawn from, and the shape the screen needs
//! it in.
//!
//! Phase 6 builds the frame; phase 7 connects it to the domain. Until then
//! every row on screen comes from the tables below, which are the ones the
//! wireframes are drawn with, including their invented calendar: 5 Sep 2026
//! is a Friday there and a Saturday in the world, so the day labels are
//! written out rather than formatted from [`crate::app::App::today`].
//!
//! Phase 7 replaces `Fixture` with the domain's views, formats the dates
//! from the clock, and deletes this file. Nothing else knows the data is
//! fake: `ui` draws a [`PaneView`] the same way whichever it came from.

/// A task or a note, as the application refers to it: by id, never by
/// index.
pub type Id = u64;

/// The state a row is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Open,
    Done,
    Waiting,
    Moved,
}

/// What a chip means, which is what decides its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipKind {
    Due,
    Overdue,
    Remind,
    Repeat,
    Waiting,
    Pile,
}

/// A bracketed mark at the right of a row. `short` is what a pane too
/// narrow for the words shows instead.
#[derive(Clone, Copy, Debug)]
pub struct Chip {
    pub kind: ChipKind,
    pub text: &'static str,
    pub short: &'static str,
}

/// One line of a pane.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub id: Id,
    pub title: &'static str,
    pub state: State,
    pub focus: bool,
    pub chips: &'static [Chip],
    /// Right-hand text that is not a chip: `←backlog`, `to Mon 8 Sep`.
    pub meta: &'static str,
    /// The time a closed task was closed at.
    pub closed_at: &'static str,
}

impl Row {
    const fn open(id: Id, title: &'static str) -> Row {
        Row {
            id,
            title,
            state: State::Open,
            focus: false,
            chips: &[],
            meta: "",
            closed_at: "",
        }
    }
}

/// The `+ add` line that closes a group.
#[derive(Clone, Copy, Debug)]
pub struct Add {
    pub label: &'static str,
    pub key: &'static str,
}

/// A group of rows under a rule, or, with an empty label, the ungrouped
/// head of a pane.
#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub label: &'static str,
    pub count: Option<usize>,
    pub rows: &'static [Row],
    pub add: Option<Add>,
}

/// What a pane says when it has nothing in it: what the list is for, and
/// the one or two keys that fill it.
#[derive(Clone, Copy, Debug)]
pub struct Empty {
    pub what: &'static str,
    pub keys: &'static str,
}

/// A whole pane: its header and its groups.
#[derive(Clone, Copy, Debug)]
pub struct PaneView {
    pub title: &'static str,
    pub sub: &'static str,
    pub right: &'static str,
    pub groups: &'static [Group],
    pub empty: Empty,
}

impl PaneView {
    /// Every row, in the order they are drawn, which is the order the
    /// cursor moves in.
    pub fn rows(&self) -> impl Iterator<Item = &'static Row> {
        self.groups.iter().flat_map(|group| group.rows.iter())
    }

    pub fn is_empty(&self) -> bool {
        self.rows().next().is_none()
    }
}

/// One line of the search results.
#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub id: Id,
    pub title: &'static str,
    pub right: &'static str,
}

/// What a search found: open tasks first, because the commonest search is
/// "did I already add this".
#[derive(Clone, Debug, Default)]
pub struct Results {
    pub open: Vec<Match>,
    pub closed: Vec<Match>,
}

impl Results {
    pub fn count(&self) -> usize {
        self.open.len() + self.closed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }
}

/// Which set of fake data the shell is showing. `Empty` is how the empty
/// states are looked at without deleting anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixture {
    Filled,
    Empty,
}

impl Fixture {
    pub fn day(self) -> &'static PaneView {
        match self {
            Fixture::Filled => &TODAY,
            Fixture::Empty => &EMPTY_DAY,
        }
    }

    pub fn backlog(self) -> &'static PaneView {
        match self {
            Fixture::Filled => &BACKLOG,
            Fixture::Empty => &EMPTY_BACKLOG,
        }
    }

    pub fn notes(self) -> &'static PaneView {
        match self {
            Fixture::Filled => &NOTES,
            Fixture::Empty => &EMPTY_NOTES,
        }
    }

    /// The header of the open note.
    pub fn open_note(self) -> &'static PaneView {
        match self {
            Fixture::Filled => &OPEN_NOTE,
            Fixture::Empty => &NO_NOTE,
        }
    }

    /// The open note, one line per line.
    pub fn note(self) -> &'static [&'static str] {
        match self {
            Fixture::Filled => NOTE_BODY,
            Fixture::Empty => &[],
        }
    }

    /// The size of the review pile, which the status line counts in red.
    pub fn review_count(self) -> usize {
        match self {
            Fixture::Filled => 2,
            Fixture::Empty => 0,
        }
    }

    /// The counts beside the three tabs of a narrow window: open tasks
    /// today, open tasks in the backlog, notes.
    pub fn tabs(self) -> [usize; 3] {
        [
            self.day().rows().filter(is_open).count(),
            self.backlog().rows().filter(is_open).count(),
            self.notes().rows().count(),
        ]
    }

    pub fn note_count(self) -> usize {
        self.notes().rows().count()
    }

    /// Titles containing the text, case-insensitively.
    pub fn search(self, text: &str) -> Results {
        if self == Fixture::Empty {
            return Results::default();
        }
        let wanted = text.trim().to_lowercase();
        let matching = |row: &&Match| row.title.to_lowercase().contains(&wanted);
        Results {
            open: OPEN_MATCHES.iter().filter(matching).copied().collect(),
            closed: CLOSED_MATCHES.iter().filter(matching).copied().collect(),
        }
    }
}

fn is_open(row: &&'static Row) -> bool {
    matches!(row.state, State::Open | State::Waiting)
}

// ---- the tables ------------------------------------------------------

/// The day the wireframes are drawn on.
pub const TODAY_LABEL: &str = "Fri 5 Sep";

const TODAY: PaneView = PaneView {
    title: "Today",
    sub: TODAY_LABEL,
    right: "6 open · 2 done · 1 moved",
    empty: EMPTY_DAY.empty,
    groups: &[
        Group {
            label: "Focus",
            count: None,
            add: None,
            rows: &[
                Row {
                    focus: true,
                    chips: &[Chip {
                        kind: ChipKind::Repeat,
                        text: "↻ every Fri",
                        short: "↻",
                    }],
                    ..Row::open(1, "Ship invoice export")
                },
                Row {
                    focus: true,
                    ..Row::open(2, "Reply to the tender questions")
                },
            ],
        },
        Group {
            label: "Plan",
            count: None,
            add: Some(Add {
                label: "add a task",
                key: "a",
            }),
            rows: &[
                Row {
                    meta: "←backlog",
                    ..Row::open(3, "Fix the flaky migration test")
                },
                Row {
                    chips: &[Chip {
                        kind: ChipKind::Remind,
                        text: "◷ today",
                        short: "◷",
                    }],
                    ..Row::open(4, "Book dentist")
                },
                Row {
                    chips: &[Chip {
                        kind: ChipKind::Repeat,
                        text: "↻ work days",
                        short: "↻",
                    }],
                    ..Row::open(5, "Write standup notes")
                },
                Row::open(6, "Review Anna's PR"),
            ],
        },
        Group {
            label: "Done",
            count: Some(2),
            add: None,
            rows: &[
                Row {
                    state: State::Done,
                    closed_at: "08:12",
                    ..Row::open(7, "Morning review")
                },
                Row {
                    state: State::Done,
                    closed_at: "08:30",
                    ..Row::open(8, "Pay electricity bill")
                },
            ],
        },
        Group {
            label: "Moved",
            count: Some(1),
            add: None,
            rows: &[Row {
                state: State::Moved,
                meta: "to Mon 8 Sep",
                ..Row::open(9, "Chase the hosting invoice")
            }],
        },
    ],
};

const BACKLOG: PaneView = PaneView {
    title: "Backlog",
    sub: "",
    right: "12 · 3 waiting",
    empty: EMPTY_BACKLOG.empty,
    groups: &[
        Group {
            label: "",
            count: None,
            add: Some(Add {
                label: "add to backlog",
                key: "a",
            }),
            rows: &[
                Row {
                    chips: &[Chip {
                        kind: ChipKind::Due,
                        text: "due 12 Sep",
                        short: "due",
                    }],
                    ..Row::open(101, "Migrate CI to the new runners")
                },
                Row {
                    chips: &[Chip {
                        kind: ChipKind::Due,
                        text: "due 30 Sep",
                        short: "due",
                    }],
                    ..Row::open(102, "Write the Q4 planning doc")
                },
                Row::open(103, "Clean out the garage"),
                Row {
                    chips: &[Chip {
                        kind: ChipKind::Remind,
                        text: "◷ 1 Oct",
                        short: "◷",
                    }],
                    ..Row::open(104, "Renew passport")
                },
                Row::open(105, "Try the new keyboard layout"),
                Row::open(106, "Read the Hyprland plugin docs"),
                Row::open(107, "Cancel unused subscriptions"),
                Row::open(108, "Sort photo backups"),
                Row::open(109, "Update the household budget"),
            ],
        },
        Group {
            label: "Waiting",
            count: Some(3),
            add: None,
            rows: &[
                Row {
                    state: State::Waiting,
                    chips: &[WAITING],
                    ..Row::open(110, "Quote from the electrician")
                },
                Row {
                    state: State::Waiting,
                    chips: &[
                        WAITING,
                        Chip {
                            kind: ChipKind::Remind,
                            text: "◷ 15 Sep",
                            short: "◷",
                        },
                    ],
                    ..Row::open(111, "Feedback on the proposal")
                },
                Row {
                    state: State::Waiting,
                    chips: &[WAITING],
                    ..Row::open(112, "Parcel from the supplier")
                },
            ],
        },
    ],
};

const WAITING: Chip = Chip {
    kind: ChipKind::Waiting,
    text: "waiting",
    short: "w",
};

const NOTES: PaneView = PaneView {
    title: "Notes",
    sub: "",
    right: "a new",
    empty: EMPTY_NOTES.empty,
    groups: &[Group {
        label: "",
        count: None,
        add: Some(Add {
            label: "new note",
            key: "a",
        }),
        rows: &[
            Row {
                meta: "yesterday",
                ..Row::open(201, "Mention to Anna: CI runner budget,")
            },
            Row {
                meta: "2 days",
                ..Row::open(202, "Draft reply to tender Q3: \"We can")
            },
            Row {
                meta: "2 days",
                ..Row::open(203, "nordic ltd PO 4471, due 30 days")
            },
            Row {
                meta: "last week",
                ..Row::open(204, "rsync -av --delete ~/work nas:/bk")
            },
        ],
    }],
};

const OPEN_NOTE: PaneView = PaneView {
    title: "Note",
    sub: "Thu 4 Sep 16:40",
    right: "esc back",
    groups: &[],
    empty: Empty { what: "", keys: "" },
};

const NO_NOTE: PaneView = PaneView {
    title: "Note",
    sub: "",
    right: "",
    groups: &[],
    empty: Empty { what: "", keys: "" },
};

const NOTE_BODY: &[&str] = &[
    "Mention to Anna:",
    "- CI runner budget",
    "- Friday demo slot",
    "- ask about the retro format",
    "",
    "Also: the tender deadline moved to the 12th, check with legal first.",
    "",
    "Draft:",
    "Hi Anna, two things before Friday. The CI runner budget needs a decision",
    "this week, and I would like the demo slot after lunch rather than before.",
];

const EMPTY_DAY: PaneView = PaneView {
    title: "Today",
    sub: TODAY_LABEL,
    right: "nothing planned",
    groups: &[],
    empty: Empty {
        what: "Nothing planned.",
        keys: "a add a task · l then t pull from the backlog",
    },
};

const EMPTY_BACKLOG: PaneView = PaneView {
    title: "Backlog",
    sub: "",
    right: "0",
    groups: &[],
    empty: Empty {
        what: "Backlog is empty.",
        keys: "a add · b on a day task sends it here",
    },
};

const EMPTY_NOTES: PaneView = PaneView {
    title: "Notes",
    sub: "",
    right: "0",
    empty: Empty { what: "", keys: "" },
    // The new-note row is the whole empty state (DESIGN.md section 10).
    groups: &[Group {
        label: "",
        count: None,
        rows: &[],
        add: Some(Add {
            label: "new note",
            key: "a",
        }),
    }],
};

const OPEN_MATCHES: &[Match] = &[
    Match {
        id: 1,
        title: "Ship invoice export",
        right: "today · focus",
    },
    Match {
        id: 113,
        title: "Chase the unpaid invoices",
        right: "backlog · waiting",
    },
];

const CLOSED_MATCHES: &[Match] = &[
    Match {
        id: 301,
        title: "Send the invoice to Nordic Ltd",
        right: "Thu 4 Sep",
    },
    Match {
        id: 302,
        title: "Ship invoice export",
        right: "Fri 29 Aug · ↻",
    },
    Match {
        id: 303,
        title: "Ship invoice export",
        right: "Fri 22 Aug · ↻",
    },
    Match {
        id: 304,
        title: "Fix invoice PDF font",
        right: "Tue 19 Aug",
    },
    Match {
        id: 305,
        title: "Set up invoice numbering",
        right: "Mon 4 Aug",
    },
    Match {
        id: 306,
        title: "Invoice template v2",
        right: "Fri 18 Jul",
    },
    Match {
        id: 307,
        title: "Ask Anna about invoice terms",
        right: "Wed 2 Jul",
    },
];
