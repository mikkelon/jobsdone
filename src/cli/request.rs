//! Every command the program takes, and the service request each one is.
//!
//! One table. A command's name mirrors the `op` it sends, so `task update`
//! is `task.update` and there is nothing to look up twice; the aliases below
//! are the only place the two are allowed to differ.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::parse::{Options, present, single};
use super::render::View;
use super::{Failure, Then};

/// Where text the request needs is read from, which is the one thing the
/// parser cannot settle: a file may not exist and standard input may not be
/// there. `main.rs` reads it and `complete` puts it in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Stdin,
    File(PathBuf),
}

/// Text a request wants, and the field it belongs in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    pub field: &'static str,
    pub source: Source,
}

/// What building a request left for somebody who can read a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    pub body: Option<Body>,
}

/// One command.
pub struct Spec {
    /// The words that name it, as they are written.
    pub name: &'static str,
    /// The `op` it sends. Empty for the commands that send none.
    pub op: &'static str,
    /// Its own options, beside the global ones, and whether each takes a
    /// value.
    pub options: &'static [(&'static str, bool)],
    pub view: View,
    pub then: Then,
    /// The one line the command list gives it.
    pub summary: &'static str,
    /// The form, for its own help.
    pub usage: &'static str,
    /// The rest of its help, including the JSON fields it sends, because an
    /// agent reading `--help` is told to build a body from it.
    pub detail: &'static str,
}

const NO_OPTIONS: &[(&str, bool)] = &[];

/// The options that name a place, plus the reading `move` wants.
const MOVE_PLACE: &[(&str, bool)] = &[
    ("--to", true),
    ("--place", true),
    ("--day", true),
    ("--today", false),
    ("--tomorrow", false),
    ("--backlog", false),
];

const TASK_LIST: &[(&str, bool)] = &[
    ("--place", true),
    ("--day", true),
    ("--today", false),
    ("--tomorrow", false),
    ("--backlog", false),
    ("--state", true),
    ("--open", false),
    ("--closed", false),
    ("--all", false),
    ("--focus", false),
    ("--waiting", false),
];

const TASK_ADD: &[(&str, bool)] = &[
    ("--place", true),
    ("--day", true),
    ("--today", false),
    ("--tomorrow", false),
    ("--backlog", false),
    ("--focus", false),
    ("--waiting", false),
    ("--due", true),
    ("--remind", true),
    ("--repeat", true),
    ("--position", true),
    ("--before", true),
    ("--after", true),
    ("--top", false),
];

const TASK_UPDATE: &[(&str, bool)] = &[
    ("--title", true),
    ("--scope", true),
    ("--place", true),
    ("--day", true),
    ("--today", false),
    ("--tomorrow", false),
    ("--backlog", false),
    ("--focus", false),
    ("--no-focus", false),
    ("--waiting", false),
    ("--no-waiting", false),
    ("--due", true),
    ("--clear-due", false),
    ("--remind", true),
    ("--clear-remind", false),
    ("--position", true),
];

const REORDER: &[(&str, bool)] = &[
    ("--position", true),
    ("--before", true),
    ("--after", true),
    ("--top", false),
];

const ORDER: &[(&str, bool)] = &[("--order", true)];
const YES: &[(&str, bool)] = &[("--yes", false)];
const SCOPE: &[(&str, bool)] = &[("--scope", true)];
const NOTE_TEXT: &[(&str, bool)] = &[("--stdin", false), ("--file", true)];

const NOTE_CHECK: &[(&str, bool)] = &[
    ("--stdin", false),
    ("--file", true),
    ("--text", true),
    ("--suggestions", false),
];

/// The whole surface, in the order the command list prints it.
static COMMANDS: &[Spec] = &[
    Spec {
        name: "task list",
        op: "task.list",
        options: TASK_LIST,
        view: View::TaskList,
        then: Then::Nothing,
        summary: "list tasks, in a place or everywhere",
        usage: "jobsdone task list [--today | --backlog | --day DATE] [--open | --closed | --all]",
        detail: "\
Days in ascending order and then the backlog, each in position order.
With no place, every place. --focus and --waiting narrow it further.

JSON fields: place (place), state (\"open\"|\"closed\"|\"all\", default
\"all\"), focus (bool), waiting (bool).",
    },
    Spec {
        name: "task get",
        op: "task.get",
        options: NO_OPTIONS,
        view: View::Task,
        then: Then::Nothing,
        summary: "show one task in full",
        usage: "jobsdone task get ID",
        detail: "JSON fields: id (integer).",
    },
    Spec {
        name: "task add",
        op: "task.add",
        options: TASK_ADD,
        view: View::Task,
        then: Then::Nothing,
        summary: "add a task, with every property it should start with",
        usage: "jobsdone task add TITLE [--today | --backlog | --day DATE] [--focus] \\\n            [--waiting] [--due DATE] [--remind DATE] [--repeat RULE] \\\n            [--position N | --before ID | --after ID | --top]",
        detail: "\
Everything the task should start with goes in one invocation: it commits as
one change and takes one undo entry. Without a place the task goes to the
backlog. --focus needs a day and --waiting needs the backlog.

JSON fields: title (string), place (place), focus (bool), waiting (bool),
due (date), remind (date), repeat (rule), and one of position (integer),
before (id), after (id).",
    },
    Spec {
        name: "task update",
        op: "task.update",
        options: TASK_UPDATE,
        view: View::Task,
        then: Then::Nothing,
        summary: "change any number of a task's properties at once",
        usage: "jobsdone task update ID [--title TEXT [--scope this|future]] \\\n            [--today | --backlog | --day DATE] [--focus | --no-focus] \\\n            [--waiting | --no-waiting] [--due DATE | --clear-due] \\\n            [--remind DATE | --clear-remind] [--position N]",
        detail: "\
One task, any number of properties, one change and one undo entry. At least
one property is required. --scope is required with --title on a recurring
copy: this renames the copy, future renames the schedule as well.

JSON fields: id (integer), title (string), title_scope (\"this\"|\"future\"),
place (place), focus (bool), waiting (bool), due (date or null), remind
(date or null), position (integer). null clears a date; leaving the field
out keeps it.",
    },
    Spec {
        name: "task rename",
        op: "task.update",
        options: SCOPE,
        view: View::Task,
        then: Then::Nothing,
        summary: "rename a task (task update --title)",
        usage: "jobsdone task rename ID TITLE [--scope this|future]",
        detail: "Sends task.update with title, and title_scope where --scope is given.",
    },
    Spec {
        name: "task focus",
        op: "task.update",
        options: NO_OPTIONS,
        view: View::Task,
        then: Then::Nothing,
        summary: "mark a task on a day as a focus item",
        usage: "jobsdone task focus ID",
        detail: "Sends task.update with focus true. Focus belongs to tasks on a day.",
    },
    Spec {
        name: "task unfocus",
        op: "task.update",
        options: NO_OPTIONS,
        view: View::Task,
        then: Then::Nothing,
        summary: "take the focus mark off a task",
        usage: "jobsdone task unfocus ID",
        detail: "Sends task.update with focus false.",
    },
    Spec {
        name: "task waiting",
        op: "task.update",
        options: NO_OPTIONS,
        view: View::Task,
        then: Then::Nothing,
        summary: "flag a task as waiting on someone else",
        usage: "jobsdone task waiting ID",
        detail: "\
Sends task.update with waiting true. A task on a day moves to the backlog,
which is what the app's w does.",
    },
    Spec {
        name: "task unwait",
        op: "task.update",
        options: NO_OPTIONS,
        view: View::Task,
        then: Then::Nothing,
        summary: "clear the waiting flag",
        usage: "jobsdone task unwait ID",
        detail: "Sends task.update with waiting false.",
    },
    Spec {
        name: "task due",
        op: "task.update",
        options: NO_OPTIONS,
        view: View::Task,
        then: Then::Nothing,
        summary: "set or clear a task's deadline",
        usage: "jobsdone task due ID DATE\n       jobsdone task due ID none",
        detail: "\
Sends task.update with due. `none`, `clear` or `-` clears it. A due date is
a deadline: it surfaces the task in the morning review and does not put it
on a day.",
    },
    Spec {
        name: "task remind",
        op: "task.update",
        options: NO_OPTIONS,
        view: View::Task,
        then: Then::Nothing,
        summary: "set or clear a task's reminder",
        usage: "jobsdone task remind ID DATE\n       jobsdone task remind ID none",
        detail: "\
Sends task.update with remind. `none`, `clear` or `-` clears it. A reminder
is a nudge on one day and does not put the task on a day.",
    },
    Spec {
        name: "task close",
        op: "task.close",
        options: NO_OPTIONS,
        view: View::Tasks,
        then: Then::Nothing,
        summary: "close one or several tasks at once",
        usage: "jobsdone task close ID [ID ...]",
        detail: "\
Atomic: one change, one undo entry, and a rejection anywhere writes nothing.
Closing a backlog task moves it onto today first, as the app does.

JSON fields: ids (array of integers, distinct).",
    },
    Spec {
        name: "task reopen",
        op: "task.reopen",
        options: NO_OPTIONS,
        view: View::Tasks,
        then: Then::Nothing,
        summary: "reopen one or several closed tasks",
        usage: "jobsdone task reopen ID [ID ...]",
        detail: "JSON fields: ids (array of integers, distinct).",
    },
    Spec {
        name: "task move",
        op: "task.move",
        options: MOVE_PLACE,
        view: View::Tasks,
        then: Then::Nothing,
        summary: "move any number of tasks to a day or the backlog",
        usage: "jobsdone task move ID [ID ...] --to today|backlog|DATE",
        detail: "\
Every id in one invocation, one change and one undo entry. Moving a waiting
task onto a day clears the waiting flag.

JSON fields: ids (array of integers, distinct), place (place).",
    },
    Spec {
        name: "task reorder",
        op: "task.reorder",
        options: REORDER,
        view: View::Order,
        then: Then::Nothing,
        summary: "put one task somewhere else in its place",
        usage: "jobsdone task reorder ID --position N | --before ID | --after ID | --top",
        detail: "\
Exactly one of the four. Positions count the open tasks of the place from
one. The reference task must be open and in the same place.

JSON fields: id (integer), and exactly one of position (integer), before
(id), after (id).",
    },
    Spec {
        name: "task delete",
        op: "task.delete",
        options: YES,
        view: View::Tasks,
        then: Then::Nothing,
        summary: "delete one or several tasks",
        usage: "jobsdone task delete ID [ID ...] [--yes]",
        detail: "\
--yes confirms a deletion that was already asked for; it does not widen what
is deleted. It is required only while the confirm-before-delete setting is
on. One undo entry takes the whole thing back.

JSON fields: ids (array of integers, distinct), confirm (bool).",
    },
    Spec {
        name: "day get",
        op: "day.get",
        options: NO_OPTIONS,
        view: View::Day,
        then: Then::Nothing,
        summary: "the plan for a day: focus, plan, done and moved",
        usage: "jobsdone day get [DATE]",
        detail: "\
With no date, today, which is the configured working day rather than the
calendar date.

JSON fields: day (date).",
    },
    Spec {
        name: "day reorder",
        op: "day.reorder",
        options: ORDER,
        view: View::Order,
        then: Then::Nothing,
        summary: "give a day its whole order at once",
        usage: "jobsdone day reorder [DATE] --order ID,ID,ID",
        detail: "\
The order must be exactly the open live tasks of that day: a duplicate, a
missing id, an extra id or an id from another place is refused and nothing
is written. Read the day first.

JSON fields: place (place), ids (array of integers).",
    },
    Spec {
        name: "backlog get",
        op: "backlog.get",
        options: NO_OPTIONS,
        view: View::Backlog,
        then: Then::Nothing,
        summary: "the backlog, its waiting tasks and its schedules",
        usage: "jobsdone backlog get",
        detail: "JSON fields: none.",
    },
    Spec {
        name: "backlog reorder",
        op: "day.reorder",
        options: ORDER,
        view: View::Order,
        then: Then::Nothing,
        summary: "give the backlog its whole order at once",
        usage: "jobsdone backlog reorder --order ID,ID,ID",
        detail: "\
day.reorder with a backlog place. The order must be exactly the open live
backlog tasks.

JSON fields: place (place), ids (array of integers).",
    },
    Spec {
        name: "history list",
        op: "history.list",
        options: &[("--limit", true)],
        view: View::History,
        then: Then::Nothing,
        summary: "the days that have something on them, newest first",
        usage: "jobsdone history list [--limit N]",
        detail: "\
Broken into later, this week, last week and earlier, the way the app's day
list is.

JSON fields: limit (integer).",
    },
    Spec {
        name: "search",
        op: "search",
        options: NO_OPTIONS,
        view: View::Search,
        then: Then::Nothing,
        summary: "find live tasks whose title contains some text",
        usage: "jobsdone search TEXT",
        detail: "\
Case-insensitive, open tasks then closed ones. Notes are not searched.

JSON fields: text (string).",
    },
    Spec {
        name: "review get",
        op: "review.get",
        options: NO_OPTIONS,
        view: View::Review,
        then: Then::Nothing,
        summary: "the morning review's pile and surfaced tasks, without starting it",
        usage: "jobsdone review get",
        detail: "\
Reading never spends the once-a-day gate and never makes a recurring copy.

JSON fields: none.",
    },
    Spec {
        name: "review start",
        op: "review.start",
        options: NO_OPTIONS,
        view: View::Review,
        then: Then::Nothing,
        summary: "start the morning review, advancing its once-a-day gate",
        usage: "jobsdone review start",
        detail: "\
Use it only when starting the review is the point; `review get` reads the
same thing without touching the gate. Pushes no undo entry.

JSON fields: none.",
    },
    Spec {
        name: "refresh",
        op: "refresh",
        options: NO_OPTIONS,
        view: View::Refresh,
        then: Then::Nothing,
        summary: "make the recurring copies that are due",
        usage: "jobsdone refresh",
        detail: "\
The one read-like command that writes. Every response's
context.recurrence_pending says whether this would do anything. Pushes no
undo entry.

JSON fields: none.",
    },
    Spec {
        name: "schedule list",
        op: "schedule.list",
        options: &[("--all", false)],
        view: View::Schedules,
        then: Then::Nothing,
        summary: "the repeat schedules",
        usage: "jobsdone schedule list [--all]",
        detail: "\
Live schedules; --all includes the stopped ones.

JSON fields: include_stopped (bool).",
    },
    Spec {
        name: "schedule get",
        op: "schedule.get",
        options: NO_OPTIONS,
        view: View::Schedule,
        then: Then::Nothing,
        summary: "one schedule and the dates it falls on next",
        usage: "jobsdone schedule get ID",
        detail: "JSON fields: id (integer).",
    },
    Spec {
        name: "schedule create",
        op: "schedule.create",
        options: &[("--rule", true)],
        view: View::Schedule,
        then: Then::Nothing,
        summary: "give a task a repeat rule",
        usage: "jobsdone schedule create TASK_ID RULE",
        detail: "\
RULE is one of: workdays, daily, weekly:mon,thu, monthly:15, monthly:last,
every:2w:2026-09-15. The task becomes the schedule's first copy and stays
where it is.

JSON fields: task (integer), rule (rule).",
    },
    Spec {
        name: "schedule update",
        op: "schedule.update",
        options: &[("--rule", true), ("--title", true)],
        view: View::Schedule,
        then: Then::Nothing,
        summary: "change a schedule's rule or the title its copies get",
        usage: "jobsdone schedule update ID [--rule RULE] [--title TEXT]",
        detail: "\
Future copies only; the copies that exist stay as they are.

JSON fields: id (integer), rule (rule), title (string).",
    },
    Spec {
        name: "schedule stop",
        op: "schedule.stop",
        options: NO_OPTIONS,
        view: View::Schedule,
        then: Then::Nothing,
        summary: "stop a schedule making new copies",
        usage: "jobsdone schedule stop ID",
        detail: "The copies that exist stay where they are.\n\nJSON fields: id (integer).",
    },
    Spec {
        name: "schedule preview",
        op: "schedule.preview",
        options: &[
            ("--rule", true),
            ("--schedule", true),
            ("--after", true),
            ("--count", true),
        ],
        view: View::Preview,
        then: Then::Nothing,
        summary: "the dates a rule would fall on, without saving it",
        usage: "jobsdone schedule preview RULE [--after DATE] [--count N]\n       jobsdone schedule preview --schedule ID [--after DATE] [--count N]",
        detail: "\
Writes nothing.

JSON fields: exactly one of rule (rule) or schedule (integer); after (date),
count (integer 1..=50).",
    },
    Spec {
        name: "note list",
        op: "note.list",
        options: &[("--bodies", false), ("--archived", false)],
        view: View::Notes,
        then: Then::Nothing,
        summary: "the notes, newest first",
        usage: "jobsdone note list [--bodies] [--archived]",
        detail: "\
First lines only unless --bodies. --archived lists the archived notes
instead, the most recently archived first.

JSON fields: include_body (bool), archived (bool).",
    },
    Spec {
        name: "note get",
        op: "note.get",
        options: NO_OPTIONS,
        view: View::Note,
        then: Then::Nothing,
        summary: "one note in full",
        usage: "jobsdone note get ID",
        detail: "The body is written out exactly.\n\nJSON fields: id (integer).",
    },
    Spec {
        name: "note create",
        op: "note.create",
        options: NOTE_TEXT,
        view: View::Note,
        then: Then::Nothing,
        summary: "add a note",
        usage: "jobsdone note create [TEXT]\n       jobsdone note create --stdin\n       jobsdone note create --file PATH",
        detail: "\
--stdin and --file carry the text exactly: newlines, tabs and any Unicode.
With nothing at all the note starts empty.

JSON fields: body (string).",
    },
    Spec {
        name: "note update",
        op: "note.update",
        options: NOTE_TEXT,
        view: View::Note,
        then: Then::Nothing,
        summary: "replace a note's text",
        usage: "jobsdone note update ID TEXT\n       jobsdone note update ID --stdin\n       jobsdone note update ID --file PATH",
        detail: "\
The whole body is replaced. --stdin and --file carry it exactly.

JSON fields: id (integer), body (string).",
    },
    Spec {
        name: "note delete",
        op: "note.delete",
        options: YES,
        view: View::Note,
        then: Then::Nothing,
        summary: "delete a note",
        usage: "jobsdone note delete ID [--yes]",
        detail: "\
--yes confirms a deletion that was already asked for. It is required only
while the confirm-before-delete setting is on.

JSON fields: id (integer), confirm (bool).",
    },
    Spec {
        name: "note archive",
        op: "note.archive",
        options: NO_OPTIONS,
        view: View::Note,
        then: Then::Nothing,
        summary: "put a note out of the list without deleting it",
        usage: "jobsdone note archive ID",
        detail: "\
An archived note is kept and can still be read, changed and deleted.
note list --archived shows it.

JSON fields: id (integer).",
    },
    Spec {
        name: "note unarchive",
        op: "note.unarchive",
        options: NO_OPTIONS,
        view: View::Note,
        then: Then::Nothing,
        summary: "bring an archived note back to the list",
        usage: "jobsdone note unarchive ID",
        detail: "JSON fields: id (integer).",
    },
    Spec {
        name: "note spell-check",
        op: "note.spell_check",
        options: NO_OPTIONS,
        view: View::Note,
        then: Then::Nothing,
        summary: "take a note out of spell checking, or put it back",
        usage: "jobsdone note spell-check ID on|off",
        detail: "\
Off leaves the note unmarked in the window while spell_check_notes is on;
every other note is still checked. note check still checks it when asked.

JSON fields: id (integer), check (bool).",
    },
    Spec {
        name: "note check",
        op: "note.check",
        options: NOTE_CHECK,
        view: View::NoteCheck,
        then: Then::Nothing,
        summary: "spell-check a note or some text, changing nothing",
        usage: "jobsdone note check ID [--suggestions]\n       jobsdone note check --stdin [--suggestions]\n       jobsdone note check --text TEXT [--suggestions]",
        detail: "\
US English, offline, with the personal dictionary applied. Offsets count
grapheme clusters. Nothing is written and no text is changed.

JSON fields: exactly one of note (integer) or text (string); suggestions
(bool).",
    },
    Spec {
        name: "note copy",
        op: "note.get",
        options: NO_OPTIONS,
        view: View::NoteCopy,
        then: Then::CopyNote,
        summary: "put a note on the desktop clipboard",
        usage: "jobsdone note copy ID",
        detail: "\
Reads the note and hands its body to wl-copy on Wayland or xclip on X11.
The only command that does anything outside the database.

JSON fields: id (integer).",
    },
    Spec {
        name: "settings get",
        op: "settings.get",
        options: NO_OPTIONS,
        view: View::Settings,
        then: Then::Nothing,
        summary: "every setting and its value",
        usage: "jobsdone settings get",
        detail: "JSON fields: none.",
    },
    Spec {
        name: "settings set",
        op: "settings.set",
        options: NO_OPTIONS,
        view: View::Settings,
        then: Then::Nothing,
        summary: "change one or several settings",
        usage: "jobsdone settings set KEY=VALUE [KEY=VALUE ...]",
        detail: "\
Keys: day_starts_at 0-23, week_starts_on monday|sunday, work_days mon,tue,…,
review_opens_itself, due_ahead_days 0-365, backfill_days 0-365,
pile_horizon_days 0-3650, floating_window, window_size WxH, mouse,
message_seconds 0-60, date_style locale|day_first|month_first,
confirm_delete, spell_check_notes. Booleans take true/false, on/off, yes/no.

A value outside its range is refused rather than held to the range. Changing
the window settings does not touch the window manager: run `jobsdone
desktop` afterwards, which is what the summary says to do.

JSON fields: settings (object of the keys above, typed).",
    },
    Spec {
        name: "dictionary list",
        op: "dictionary.list",
        options: NO_OPTIONS,
        view: View::Dictionary,
        then: Then::Nothing,
        summary: "the personal spelling dictionary",
        usage: "jobsdone dictionary list",
        detail: "JSON fields: none.",
    },
    Spec {
        name: "dictionary add",
        op: "dictionary.add",
        options: NO_OPTIONS,
        view: View::Word,
        then: Then::Nothing,
        summary: "accept a word in every note",
        usage: "jobsdone dictionary add WORD",
        detail: "\
One word. Capitalisation is kept for display and ignored when matching.
Adding a word never changes a note's text.

JSON fields: word (string).",
    },
    Spec {
        name: "dictionary update",
        op: "dictionary.update",
        options: NO_OPTIONS,
        view: View::Word,
        then: Then::Nothing,
        summary: "change how a dictionary word is written",
        usage: "jobsdone dictionary update KEY WORD",
        detail: "\
KEY is the existing entry, matched whatever its capitalisation.

JSON fields: key (string), word (string).",
    },
    Spec {
        name: "dictionary delete",
        op: "dictionary.delete",
        options: NO_OPTIONS,
        view: View::Word,
        then: Then::Nothing,
        summary: "remove a word from the dictionary",
        usage: "jobsdone dictionary delete KEY",
        detail: "\
The word can be flagged again afterwards; built-in words stay accepted.

JSON fields: key (string).",
    },
    Spec {
        name: "undo get",
        op: "undo.get",
        options: NO_OPTIONS,
        view: View::Undo,
        then: Then::Nothing,
        summary: "what undo would take back next",
        usage: "jobsdone undo get",
        detail: "\
The undo history is shared with the app and with every other invocation.

JSON fields: none.",
    },
    Spec {
        name: "undo apply",
        op: "undo.apply",
        options: &[("--entry", true)],
        view: View::Undone,
        then: Then::Nothing,
        summary: "take back the last change",
        usage: "jobsdone undo apply [--entry ID]",
        detail: "\
--entry guards against taking back something newer that arrived in between:
read `undo get`, then pass its id. A mismatch is a conflict and writes
nothing.

JSON fields: expected_id (integer).",
    },
    Spec {
        name: "desktop",
        op: "",
        options: &[("--floating", false), ("--tiled", false), ("--size", true)],
        view: View::None,
        then: Then::Nothing,
        summary: "write the window rule the window manager reads",
        usage: "jobsdone desktop [--floating | --tiled] [--size WxH]",
        detail: "\
Puts the flags in the settings, writes the window rule and reloads it. A
flag left off keeps the setting as it is. Touches no other part of the
program: it makes no recurring copies and opens no review.",
    },
    Spec {
        name: "help",
        op: "",
        options: NO_OPTIONS,
        view: View::None,
        then: Then::Nothing,
        summary: "help for the program or for one command",
        usage: "jobsdone help [COMMAND]",
        detail: "",
    },
];

pub fn commands() -> &'static [Spec] {
    COMMANDS
}

/// What a written command is called in the table. The left side is only
/// ever an alias; a canonical name never appears here.
const ALIASES: &[(&str, &str)] = &[
    ("tasks", "task"),
    ("notes", "note"),
    ("dict", "dictionary"),
    ("dictionaries", "dictionary"),
    ("schedules", "schedule"),
    ("days", "day"),
    ("setting", "settings"),
    ("history", "history list"),
    ("task show", "task get"),
    ("task create", "task add"),
    ("task new", "task add"),
    ("task edit", "task update"),
    ("task set", "task update"),
    ("task ls", "task list"),
    ("task mv", "task move"),
    ("task rm", "task delete"),
    ("task del", "task delete"),
    ("task done", "task close"),
    ("task repeat", "schedule create"),
    ("day show", "day get"),
    ("day view", "day get"),
    ("day list", "history list"),
    ("backlog show", "backlog get"),
    ("backlog list", "backlog get"),
    ("history ls", "history list"),
    ("history get", "history list"),
    ("review show", "review get"),
    ("schedule show", "schedule get"),
    ("schedule new", "schedule create"),
    ("schedule add", "schedule create"),
    ("schedule edit", "schedule update"),
    ("schedule ls", "schedule list"),
    ("note show", "note get"),
    ("note add", "note create"),
    ("note new", "note create"),
    ("note edit", "note update"),
    ("note set", "note update"),
    ("note rm", "note delete"),
    ("note del", "note delete"),
    ("note ls", "note list"),
    ("settings show", "settings get"),
    ("settings list", "settings get"),
    ("dictionary ls", "dictionary list"),
    ("dictionary edit", "dictionary update"),
    ("dictionary remove", "dictionary delete"),
    ("dictionary rm", "dictionary delete"),
    ("undo show", "undo get"),
    ("undo list", "undo get"),
    ("undo apply --entry", "undo apply"),
];

/// The command those words name.
pub fn command_named(words: &[String]) -> Result<&'static Spec, Failure> {
    if words.is_empty() {
        return Err(Failure::usage("there is no command here".to_owned()));
    }
    let written = words.join(" ").to_lowercase();

    // A noun on its own may be an alias for a whole command, and a noun's
    // alias may need resolving before the verb beside it can be looked up.
    let resolved = resolve(&written);
    if let Some(spec) = COMMANDS.iter().find(|spec| spec.name == resolved) {
        return Ok(spec);
    }

    let noun = words[0].to_lowercase();
    let known = COMMANDS
        .iter()
        .any(|spec| spec.name.split(' ').next() == Some(noun.as_str()))
        || ALIASES.iter().any(|(from, _)| *from == noun);
    if words.len() == 1 && known {
        let verbs: Vec<&str> = COMMANDS
            .iter()
            .filter_map(|spec| spec.name.strip_prefix(&format!("{noun} ")))
            .collect();
        return Err(Failure::usage(format!(
            "{noun} wants one of: {}",
            verbs.join(", ")
        )));
    }
    if !known {
        return Err(Failure::usage(format!("{noun} is not a command")));
    }
    Err(Failure::usage(format!(
        "{written} is not a command; try `jobsdone help {noun}`"
    )))
}

/// A written command with its aliases replaced, noun first and then the
/// pair, so that `dict rm` reaches `dictionary delete`.
fn resolve(written: &str) -> String {
    let direct = ALIASES
        .iter()
        .find(|(from, _)| *from == written)
        .map(|(_, to)| (*to).to_owned());
    if let Some(direct) = direct {
        return direct;
    }
    let Some((noun, verb)) = written.split_once(' ') else {
        return written.to_owned();
    };
    // Only a noun that is another noun stands in for one here. `history`
    // is an alias for a whole command, which the direct lookup above
    // answers and which would otherwise put three words in a pair.
    let noun = ALIASES
        .iter()
        .find(|(from, to)| *from == noun && !to.contains(' '))
        .map(|(_, to)| *to)
        .unwrap_or(noun);
    let pair = format!("{noun} {verb}");
    ALIASES
        .iter()
        .find(|(from, _)| *from == pair)
        .map(|(_, to)| (*to).to_owned())
        .unwrap_or(pair)
}

// ---- from a command line to a request body --------------------------

/// The fields of the request the command sends, and whatever text it still
/// needs from a file or from standard input.
///
/// `input` says whether a `--input` body is coming. A field that would
/// otherwise be required is then allowed to be missing here, because the
/// body is the other half of the same request and the service is the one
/// place that knows what a complete one looks like. A field that is
/// *written* and wrong is still wrong.
pub fn fields(
    spec: &'static Spec,
    options: &Options,
    positionals: &[String],
    input: bool,
    out: &mut Map<String, Value>,
) -> Result<Extras, Failure> {
    let mut extras = Extras::default();
    let name = spec.name;
    let want = |wanted: &str| Failure::usage(format!("{name} wants {wanted}"));

    match name {
        "task list" => {
            no_positionals(name, positionals)?;
            if let Some(place) = place_asked(options, name)? {
                out.insert("place".to_owned(), place);
            }
            if let Some(state) = state_asked(options)? {
                out.insert("state".to_owned(), Value::String(state));
            }
            if present(options, "--focus") {
                out.insert("focus".to_owned(), Value::Bool(true));
            }
            if present(options, "--waiting") {
                out.insert("waiting".to_owned(), Value::Bool(true));
            }
        }
        "task get" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a task id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
        }
        "task add" => {
            if let Some(title) = needed(joined(positionals), input, || want("a title"))? {
                out.insert("title".to_owned(), Value::String(title));
            }
            match place_asked(options, name)? {
                Some(place) => {
                    out.insert("place".to_owned(), place);
                }
                // The backlog is where a task with no day belongs, so it
                // is the default rather than a question; a body naming a
                // place is left to name it.
                None if !input => {
                    out.insert("place".to_owned(), backlog());
                }
                None => {}
            }
            if present(options, "--focus") {
                out.insert("focus".to_owned(), Value::Bool(true));
            }
            if present(options, "--waiting") {
                out.insert("waiting".to_owned(), Value::Bool(true));
            }
            date_option(options, "--due", "due", out)?;
            date_option(options, "--remind", "remind", out)?;
            if let Some(text) = single(options, "--repeat")? {
                out.insert("repeat".to_owned(), rule_value(&text)?);
            }
            where_in_place(options, out)?;
        }
        "task update" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a task id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            if let Some(title) = single(options, "--title")? {
                out.insert("title".to_owned(), Value::String(title));
            }
            if let Some(scope) = scope_asked(options)? {
                out.insert("title_scope".to_owned(), Value::String(scope));
            }
            if let Some(place) = place_asked(options, name)? {
                out.insert("place".to_owned(), place);
            }
            two_ways(options, "--focus", "--no-focus", "focus", out)?;
            two_ways(options, "--waiting", "--no-waiting", "waiting", out)?;
            date_or_cleared(options, "--due", "--clear-due", "due", out)?;
            date_or_cleared(options, "--remind", "--clear-remind", "remind", out)?;
            if let Some(text) = single(options, "--position")? {
                out.insert("position".to_owned(), position(&text)?);
            }
        }
        "task rename" => {
            let asked = at_least_two(positionals);
            if let Some((first, rest)) = needed(asked, input, || want("a task id and a title"))? {
                out.insert("id".to_owned(), id(&first)?);
                out.insert("title".to_owned(), Value::String(rest));
            }
            if let Some(scope) = scope_asked(options)? {
                out.insert("title_scope".to_owned(), Value::String(scope));
            }
        }
        "task focus" | "task unfocus" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a task id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            out.insert("focus".to_owned(), Value::Bool(name == "task focus"));
        }
        "task waiting" | "task unwait" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a task id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            out.insert("waiting".to_owned(), Value::Bool(name == "task waiting"));
        }
        "task due" | "task remind" => {
            let field = if name == "task due" { "due" } else { "remind" };
            let asked = at_least_two(positionals);
            if let Some((first, rest)) = needed(asked, input, || want("a task id and a date"))? {
                out.insert("id".to_owned(), id(&first)?);
                out.insert(field.to_owned(), a_date(&rest));
            }
        }
        "task close" | "task reopen" => {
            if let Some(ids) = needed(ids(positionals)?, input, || want("at least one task id"))? {
                out.insert("ids".to_owned(), ids);
            }
        }
        "task move" => {
            if let Some(ids) = needed(ids(positionals)?, input, || want("at least one task id"))? {
                out.insert("ids".to_owned(), ids);
            }
            let asked = place_asked(options, name)?;
            if let Some(place) = needed(asked, input, || {
                want("somewhere to move to, as in --to today")
            })? {
                out.insert("place".to_owned(), place);
            }
        }
        "task delete" => {
            if let Some(ids) = needed(ids(positionals)?, input, || want("at least one task id"))? {
                out.insert("ids".to_owned(), ids);
            }
            if present(options, "--yes") {
                out.insert("confirm".to_owned(), Value::Bool(true));
            }
        }
        "task reorder" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a task id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            if !where_in_place(options, out)? && !input {
                return Err(want("one of --position, --before, --after or --top"));
            }
        }
        "day get" => {
            if let Some(day) = one(name, positionals)? {
                out.insert("day".to_owned(), Value::String(day));
            }
        }
        "day reorder" => {
            match one(name, positionals)? {
                Some(day) => {
                    out.insert("place".to_owned(), day_place(&day));
                }
                None if !input => {
                    out.insert("place".to_owned(), day_place("today"));
                }
                None => {}
            }
            let asked = order_asked(options)?;
            if let Some(order) = needed(asked, input, || {
                want("the whole order, as in --order 3,1,2")
            })? {
                out.insert("ids".to_owned(), order);
            }
        }
        "backlog get" => {
            no_positionals(name, positionals)?;
        }
        "backlog reorder" => {
            no_positionals(name, positionals)?;
            if !input {
                out.insert("place".to_owned(), backlog());
            }
            let asked = order_asked(options)?;
            if let Some(order) = needed(asked, input, || {
                want("the whole order, as in --order 3,1,2")
            })? {
                out.insert("ids".to_owned(), order);
                out.insert("place".to_owned(), backlog());
            }
        }
        "history list" => {
            no_positionals(name, positionals)?;
            if let Some(limit) = single(options, "--limit")? {
                out.insert("limit".to_owned(), position(&limit)?);
            }
        }
        "search" => {
            let asked = joined(positionals);
            if let Some(text) = needed(asked, input, || want("something to look for"))? {
                out.insert("text".to_owned(), Value::String(text));
            }
        }
        "review get" | "review start" | "refresh" | "settings get" | "dictionary list"
        | "undo get" => {
            no_positionals(name, positionals)?;
        }
        "schedule list" => {
            no_positionals(name, positionals)?;
            if present(options, "--all") {
                out.insert("include_stopped".to_owned(), Value::Bool(true));
            }
        }
        "schedule get" | "schedule stop" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a schedule id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
        }
        "schedule create" => {
            let asked = match single(options, "--rule")? {
                Some(rule) => one(name, positionals)?.map(|task| (task, rule)),
                None => at_least_two(positionals),
            };
            if let Some((task, rule)) = needed(asked, input, || want("a task id and a rule"))? {
                out.insert("task".to_owned(), id(&task)?);
                out.insert("rule".to_owned(), rule_value(&rule)?);
            }
        }
        "schedule update" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a schedule id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            if let Some(text) = single(options, "--rule")? {
                out.insert("rule".to_owned(), rule_value(&text)?);
            }
            if let Some(title) = single(options, "--title")? {
                out.insert("title".to_owned(), Value::String(title));
            }
        }
        "schedule preview" => {
            let named = single(options, "--schedule")?;
            let rule = single(options, "--rule")?.or(one(name, positionals)?);
            match (named, rule) {
                (Some(_), Some(_)) => {
                    return Err(Failure::usage(
                        "schedule preview takes a rule or --schedule, not both".to_owned(),
                    ));
                }
                (Some(named), None) => {
                    out.insert("schedule".to_owned(), id(&named)?);
                }
                (None, Some(rule)) => {
                    out.insert("rule".to_owned(), rule_value(&rule)?);
                }
                (None, None) if input => {}
                (None, None) => {
                    return Err(want("a rule, as in `jobsdone schedule preview weekly:mon`"));
                }
            }
            if let Some(after) = single(options, "--after")? {
                out.insert("after".to_owned(), Value::String(after));
            }
            if let Some(n) = single(options, "--count")? {
                out.insert("count".to_owned(), position(&n)?);
            }
        }
        "note list" => {
            no_positionals(name, positionals)?;
            if present(options, "--bodies") {
                out.insert("include_body".to_owned(), Value::Bool(true));
            }
            if present(options, "--archived") {
                out.insert("archived".to_owned(), Value::Bool(true));
            }
        }
        "note get" | "note copy" | "note archive" | "note unarchive" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a note id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
        }
        "note create" => {
            let written = one(name, positionals)?;
            match note_text(options, written, "body", &mut extras)? {
                Some(body) => {
                    out.insert("body".to_owned(), Value::String(body));
                }
                // An empty note is a real thing to make, so nothing at all
                // is a body rather than a question. A body coming in is
                // left to be the body.
                None if extras.body.is_none() && !input => {
                    out.insert("body".to_owned(), Value::String(String::new()));
                }
                None => {}
            }
        }
        "note update" => {
            let (first, written) = match positionals {
                [] => (None, None),
                [only] => (Some(only.clone()), None),
                [first, second] => (Some(first.clone()), Some(second.clone())),
                [_, _, _, ..] => {
                    return Err(Failure::usage(
                        "note update wants one body; quote it, or use --stdin or --file".to_owned(),
                    ));
                }
            };
            if let Some(only) = needed(first, input, || want("a note id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            match note_text(options, written, "body", &mut extras)? {
                Some(body) => {
                    out.insert("body".to_owned(), Value::String(body));
                }
                None if extras.body.is_none() && !input => {
                    return Err(want("the new body, or --stdin, or --file"));
                }
                None => {}
            }
        }
        "note spell-check" => {
            let (first, state) = match positionals {
                [] => (None, None),
                [only] => (Some(only.clone()), None),
                [first, second] => (Some(first.clone()), Some(second.clone())),
                [_, _, _, ..] => {
                    return Err(Failure::usage(
                        "note spell-check wants a note id and on or off".to_owned(),
                    ));
                }
            };
            if let Some(only) = needed(first, input, || want("a note id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            match state {
                Some(state) => {
                    out.insert(
                        "check".to_owned(),
                        setting_value("spell-check", &state, Shape::Flag)?,
                    );
                }
                None if !input => return Err(want("on or off")),
                None => {}
            }
        }
        "note delete" => {
            if let Some(only) = needed(one(name, positionals)?, input, || want("a note id"))? {
                out.insert("id".to_owned(), id(&only)?);
            }
            if present(options, "--yes") {
                out.insert("confirm".to_owned(), Value::Bool(true));
            }
        }
        "note check" => {
            let written = one(name, positionals)?;
            let raw = note_text(options, None, "text", &mut extras)?;
            match (written, raw, &extras.body) {
                (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
                    return Err(Failure::usage(
                        "note check takes a note id or some text, not both".to_owned(),
                    ));
                }
                (Some(note), None, None) => {
                    out.insert("note".to_owned(), id(&note)?);
                }
                (None, Some(text), _) => {
                    out.insert("text".to_owned(), Value::String(text));
                }
                (None, None, Some(_)) => {}
                (None, None, None) if input => {}
                (None, None, None) => {
                    return Err(want("a note id, --text, --stdin or --file"));
                }
            }
            if present(options, "--suggestions") {
                out.insert("suggestions".to_owned(), Value::Bool(true));
            }
        }
        "settings set" => {
            let asked = settings_value(positionals)?;
            if let Some(settings) = needed(asked, input, || {
                want("at least one KEY=VALUE, as in `settings set day_starts_at=6`")
            })? {
                out.insert("settings".to_owned(), settings);
            }
        }
        "dictionary add" => {
            if let Some(word) = needed(one(name, positionals)?, input, || want("a word"))? {
                out.insert("word".to_owned(), Value::String(word));
            }
        }
        "dictionary update" => {
            let asked = at_least_two(positionals);
            if let Some((key, word)) =
                needed(asked, input, || want("the word to change and its new form"))?
            {
                out.insert("key".to_owned(), Value::String(key));
                out.insert("word".to_owned(), Value::String(word));
            }
        }
        "dictionary delete" => {
            if let Some(key) = needed(one(name, positionals)?, input, || want("a word"))? {
                out.insert("key".to_owned(), Value::String(key));
            }
        }
        "undo apply" => {
            no_positionals(name, positionals)?;
            if let Some(entry) = single(options, "--entry")? {
                out.insert("expected_id".to_owned(), id(&entry)?);
            }
        }
        other => {
            return Err(Failure::usage(format!("{other} has nothing to send")));
        }
    }
    Ok(extras)
}

/// A piece the command needs: here, coming in a `--input` body, or missing
/// and said so.
fn needed<T>(
    got: Option<T>,
    input: bool,
    missing: impl Fn() -> Failure,
) -> Result<Option<T>, Failure> {
    match got {
        Some(value) => Ok(Some(value)),
        None if input => Ok(None),
        None => Err(missing()),
    }
}

// ---- the pieces a request is made of --------------------------------

fn backlog() -> Value {
    serde_json::json!({ "kind": "backlog" })
}

fn day_place(day: &str) -> Value {
    serde_json::json!({ "kind": "day", "day": day })
}

/// A date, or `null` for the words that clear one.
fn a_date(text: &str) -> Value {
    match text.to_lowercase().as_str() {
        "none" | "clear" | "-" | "never" => Value::Null,
        _ => Value::String(text.to_owned()),
    }
}

/// The place the options name, from whichever of the five ways was used.
fn place_asked(options: &Options, command: &str) -> Result<Option<Value>, Failure> {
    let mut asked: Vec<(&str, Value)> = Vec::new();
    for name in ["--place", "--day", "--to"] {
        if let Some(text) = single(options, name)? {
            let place = if name == "--day" {
                day_place(&text)
            } else {
                written_place(&text)
            };
            asked.push((name, place));
        }
    }
    if present(options, "--today") {
        asked.push(("--today", day_place("today")));
    }
    if present(options, "--tomorrow") {
        asked.push(("--tomorrow", day_place("tomorrow")));
    }
    if present(options, "--backlog") {
        asked.push(("--backlog", backlog()));
    }

    let mut asked = asked.into_iter();
    let Some((first, place)) = asked.next() else {
        return Ok(None);
    };
    for (second, other) in asked {
        if other != place {
            return Err(Failure::usage(format!(
                "{first} and {second} name different places for {command}"
            )));
        }
    }
    Ok(Some(place))
}

/// `backlog`, or a day. The one word that is not a date is the backlog.
fn written_place(text: &str) -> Value {
    if text.eq_ignore_ascii_case("backlog") {
        backlog()
    } else {
        day_place(text)
    }
}

fn state_asked(options: &Options) -> Result<Option<String>, Failure> {
    let mut asked: Vec<String> = Vec::new();
    if let Some(state) = single(options, "--state")? {
        match state.as_str() {
            "open" | "closed" | "all" => asked.push(state),
            other => {
                return Err(Failure::usage(format!(
                    "{other} is not a state; the states are open, closed and all"
                )));
            }
        }
    }
    for (option, state) in [("--open", "open"), ("--closed", "closed"), ("--all", "all")] {
        if present(options, option) {
            asked.push(state.to_owned());
        }
    }
    asked.dedup();
    match asked.len() {
        0 => Ok(None),
        1 => Ok(asked.pop()),
        _ => Err(Failure::usage(format!(
            "{} ask for different tasks",
            asked.join(" and ")
        ))),
    }
}

fn scope_asked(options: &Options) -> Result<Option<String>, Failure> {
    match single(options, "--scope")?.as_deref() {
        None => Ok(None),
        Some("this") => Ok(Some("this".to_owned())),
        Some("future") => Ok(Some("future".to_owned())),
        Some(other) => Err(Failure::usage(format!(
            "{other} is not a scope; the scopes are this and future"
        ))),
    }
}

/// `--focus` and `--no-focus`, which cannot both be asked for.
fn two_ways(
    options: &Options,
    on: &str,
    off: &str,
    field: &str,
    out: &mut Map<String, Value>,
) -> Result<(), Failure> {
    match (present(options, on), present(options, off)) {
        (true, true) => Err(Failure::usage(format!("{on} and {off} ask for opposites"))),
        (true, false) => {
            out.insert(field.to_owned(), Value::Bool(true));
            Ok(())
        }
        (false, true) => {
            out.insert(field.to_owned(), Value::Bool(false));
            Ok(())
        }
        (false, false) => Ok(()),
    }
}

fn date_option(
    options: &Options,
    name: &str,
    field: &str,
    out: &mut Map<String, Value>,
) -> Result<(), Failure> {
    if let Some(text) = single(options, name)? {
        out.insert(field.to_owned(), a_date(&text));
    }
    Ok(())
}

fn date_or_cleared(
    options: &Options,
    set: &str,
    clear: &str,
    field: &str,
    out: &mut Map<String, Value>,
) -> Result<(), Failure> {
    let given = single(options, set)?;
    let cleared = present(options, clear);
    if given.is_some() && cleared {
        return Err(Failure::usage(format!(
            "{set} and {clear} ask for opposites"
        )));
    }
    if cleared {
        out.insert(field.to_owned(), Value::Null);
    } else if let Some(text) = given {
        out.insert(field.to_owned(), a_date(&text));
    }
    Ok(())
}

/// `--position`, `--before`, `--after` or `--top`, of which one at most.
/// Answers whether any of them was given.
fn where_in_place(options: &Options, out: &mut Map<String, Value>) -> Result<bool, Failure> {
    let mut asked: Vec<(&str, Value)> = Vec::new();
    if let Some(text) = single(options, "--position")? {
        asked.push(("position", position(&text)?));
    }
    if present(options, "--top") {
        asked.push(("position", Value::from(1)));
    }
    if let Some(text) = single(options, "--before")? {
        asked.push(("before", id(&text)?));
    }
    if let Some(text) = single(options, "--after")? {
        asked.push(("after", id(&text)?));
    }
    match asked.len() {
        0 => Ok(false),
        1 => {
            let (field, value) = asked.remove(0);
            out.insert(field.to_owned(), value);
            Ok(true)
        }
        _ => Err(Failure::usage(
            "--position, --before, --after and --top are four ways of saying one thing; use one"
                .to_owned(),
        )),
    }
}

fn order_asked(options: &Options) -> Result<Option<Value>, Failure> {
    let Some(text) = single(options, "--order")? else {
        return Ok(None);
    };
    let mut order = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for part in text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let one = number(part)?;
        if seen.contains(&one) {
            return Err(Failure::usage(format!("{one} is given twice")));
        }
        seen.push(one);
        order.push(Value::from(one));
    }
    if order.is_empty() {
        return Err(Failure::usage(
            "--order wants the task ids of the whole place".to_owned(),
        ));
    }
    Ok(Some(Value::Array(order)))
}

/// Where a note's text comes from: written, standard input or a file, of
/// which one at most.
fn note_text(
    options: &Options,
    written: Option<String>,
    field: &'static str,
    extras: &mut Extras,
) -> Result<Option<String>, Failure> {
    let from_stdin = present(options, "--stdin");
    let from_file = single(options, "--file")?;
    let told = single(options, "--text")?;

    let ways = usize::from(written.is_some())
        + usize::from(from_stdin)
        + usize::from(from_file.is_some())
        + usize::from(told.is_some());
    if ways > 1 {
        return Err(Failure::usage(
            "the text can come from one place: written out, --text, --stdin or --file".to_owned(),
        ));
    }
    if from_stdin {
        extras.body = Some(Body {
            field,
            source: Source::Stdin,
        });
        return Ok(None);
    }
    if let Some(path) = from_file {
        extras.body = Some(Body {
            field,
            source: Source::File(PathBuf::from(path)),
        });
        return Ok(None);
    }
    Ok(written.or(told))
}

// ---- settings -------------------------------------------------------

/// How each setting is written on the command line and what it is in the
/// request. The service refuses a value outside its range; this only says
/// which of the five shapes the text is read as, because a key it does not
/// know is a key whose shape nobody can guess.
const SETTINGS: &[(&str, Shape)] = &[
    ("day_starts_at", Shape::Number),
    ("week_starts_on", Shape::Word),
    ("work_days", Shape::Words),
    ("review_opens_itself", Shape::Flag),
    ("due_ahead_days", Shape::Number),
    ("backfill_days", Shape::Number),
    ("pile_horizon_days", Shape::Number),
    ("floating_window", Shape::Flag),
    ("window_size", Shape::Size),
    ("mouse", Shape::Flag),
    ("message_seconds", Shape::Number),
    ("date_style", Shape::Word),
    ("confirm_delete", Shape::Flag),
    ("spell_check_notes", Shape::Flag),
];

#[derive(Clone, Copy)]
enum Shape {
    Number,
    Flag,
    Word,
    /// A comma-separated list, which is only ever the work days.
    Words,
    /// `WxH`.
    Size,
}

fn settings_value(positionals: &[String]) -> Result<Option<Value>, Failure> {
    if positionals.is_empty() {
        return Ok(None);
    }
    let mut settings = Map::new();
    for pair in positionals {
        let (key, text) = pair
            .split_once('=')
            .ok_or_else(|| Failure::usage(format!("{pair} is not a KEY=VALUE")))?;
        let shape = SETTINGS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, shape)| *shape)
            .ok_or_else(|| {
                Failure::usage(format!(
                    "{key} is not a setting; the settings are {}",
                    SETTINGS
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<&str>>()
                        .join(", ")
                ))
            })?;
        if settings.contains_key(key) {
            return Err(Failure::usage(format!("{key} is set twice")));
        }
        settings.insert(key.to_owned(), setting_value(key, text, shape)?);
    }
    Ok(Some(Value::Object(settings)))
}

fn setting_value(key: &str, text: &str, shape: Shape) -> Result<Value, Failure> {
    match shape {
        Shape::Number => text
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| Failure::usage(format!("{key} wants a whole number, not {text}"))),
        Shape::Flag => match text.trim().to_lowercase().as_str() {
            "true" | "on" | "yes" | "1" => Ok(Value::Bool(true)),
            "false" | "off" | "no" | "0" => Ok(Value::Bool(false)),
            _ => Err(Failure::usage(format!("{key} wants on or off, not {text}"))),
        },
        Shape::Word => Ok(Value::String(text.trim().to_lowercase())),
        Shape::Words => {
            let words: Vec<Value> = text
                .split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(|word| Value::String(word.to_lowercase()))
                .collect();
            if words.is_empty() {
                return Err(Failure::usage(format!("{key} wants at least one day")));
            }
            Ok(Value::Array(words))
        }
        Shape::Size => {
            let (width, height) = text.split_once('x').ok_or_else(|| {
                Failure::usage(format!("{key} wants a size, as in 870x650, not {text}"))
            })?;
            let read = |part: &str| -> Result<Value, Failure> {
                part.trim()
                    .parse::<i64>()
                    .map(Value::from)
                    .map_err(|_| Failure::usage(format!("{key} wants a size, as in 870x650")))
            };
            Ok(serde_json::json!({ "width": read(width)?, "height": read(height)? }))
        }
    }
}

// ---- rules ----------------------------------------------------------

/// A repeat rule, written the short way a command line writes one.
pub fn rule_value(text: &str) -> Result<Value, Failure> {
    let text = text.trim();
    let (kind, rest) = match text.split_once(':') {
        Some((kind, rest)) => (kind.trim().to_lowercase(), rest.trim()),
        None => (text.to_lowercase(), ""),
    };
    let wrong = |what: &str| Failure::usage(format!("{text} is not a repeat rule: {what}"));

    match kind.as_str() {
        "workdays" | "work-days" | "work_days" | "weekdays" => {
            Ok(serde_json::json!({ "kind": "workdays" }))
        }
        "daily" | "every-day" | "everyday" => Ok(serde_json::json!({ "kind": "daily" })),
        "weekly" => {
            let days: Vec<Value> = rest
                .split(',')
                .map(str::trim)
                .filter(|day| !day.is_empty())
                .map(|day| Value::String(day.to_lowercase()))
                .collect();
            if days.is_empty() {
                return Err(wrong("weekly wants weekdays, as in weekly:mon,thu"));
            }
            Ok(serde_json::json!({ "kind": "weekly", "weekdays": days }))
        }
        "monthly" => {
            if rest.eq_ignore_ascii_case("last") {
                return Ok(serde_json::json!({ "kind": "monthly", "day": "last" }));
            }
            let day: i64 = rest
                .parse()
                .map_err(|_| wrong("monthly wants a day of the month, as in monthly:15"))?;
            Ok(serde_json::json!({ "kind": "monthly", "day": day }))
        }
        "every" => {
            let (weeks, from) = rest.split_once(':').ok_or_else(|| {
                wrong("every wants weeks and a day to count from, as in every:2w:2026-09-15")
            })?;
            let n: i64 = weeks
                .trim()
                .trim_end_matches(['w', 'W'])
                .parse()
                .map_err(|_| wrong("every wants a number of weeks, as in every:2w:2026-09-15"))?;
            Ok(serde_json::json!({
                "kind": "every_n_weeks",
                "n": n,
                "from": from.trim(),
            }))
        }
        _ => Err(wrong(
            "the rules are workdays, daily, weekly:mon,thu, monthly:15, monthly:last and every:2w:DATE",
        )),
    }
}

// ---- positionals ----------------------------------------------------

fn no_positionals(command: &str, positionals: &[String]) -> Result<(), Failure> {
    match positionals.first() {
        None => Ok(()),
        Some(first) => Err(Failure::usage(format!(
            "{command} takes no arguments, and {first} is one"
        ))),
    }
}

/// The one argument the command takes, where it was given.
fn one(command: &str, positionals: &[String]) -> Result<Option<String>, Failure> {
    match positionals {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        [_, second, ..] => Err(Failure::usage(format!(
            "{command} takes one argument, and {second} is a second"
        ))),
    }
}

/// The first argument and everything after it as one line, which is how a
/// title typed without quotes still arrives as a title.
fn at_least_two(positionals: &[String]) -> Option<(String, String)> {
    match positionals {
        [] | [_] => None,
        [first, rest @ ..] => Some((first.clone(), rest.join(" "))),
    }
}

fn joined(positionals: &[String]) -> Option<String> {
    if positionals.is_empty() {
        return None;
    }
    Some(positionals.join(" "))
}

/// Every id the arguments name, by space or by comma, distinct.
fn ids(positionals: &[String]) -> Result<Option<Value>, Failure> {
    let mut found: Vec<Value> = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for part in positionals
        .iter()
        .flat_map(|argument| argument.split(','))
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let number = number(part)?;
        if seen.contains(&number) {
            return Err(Failure::usage(format!("{number} is given twice")));
        }
        seen.push(number);
        found.push(Value::from(number));
    }
    Ok((!found.is_empty()).then_some(Value::Array(found)))
}

fn id(text: &str) -> Result<Value, Failure> {
    Ok(Value::from(number(text)?))
}

fn number(text: &str) -> Result<u64, Failure> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|number| *number >= 1)
        .ok_or_else(|| Failure::usage(format!("{text} is not an id")))
}

fn position(text: &str) -> Result<Value, Failure> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|number| *number >= 1)
        .map(Value::from)
        .ok_or_else(|| Failure::usage(format!("{text} is not a position; positions start at 1")))
}
