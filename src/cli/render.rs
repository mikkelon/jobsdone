//! The service's answer, read out.
//!
//! Text is the default because the command line is read by people as often
//! as by programs, and a pretty-printed envelope is not a list: `--json`
//! gives the machine response and everything here gives the reading of it.
//! Nothing here is coloured, boxed or padded to a width; a list is lines.

use std::fmt::Write as _;

use jiff::civil::Date;
use serde_json::Value;

use crate::domain::{DateOrder, day_label};

use super::{Failure, Operation};

/// Which reading of an answer the text format uses. One per shape of
/// `data`, not one per operation: the four multi-task mutations answer
/// alike and so are read alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// Not an operation at all.
    None,
    TaskList,
    /// One task, with the entry that would take the change back.
    Task,
    /// Several tasks changed at once.
    Tasks,
    /// A place in its new order.
    Order,
    Day,
    Backlog,
    History,
    Search,
    Review,
    Schedules,
    Schedule,
    Preview,
    Notes,
    Note,
    NoteCheck,
    NoteCopy,
    Settings,
    Dictionary,
    Word,
    Undo,
    Undone,
    Refresh,
}

/// The answer, read out.
pub fn text(operation: &Operation, envelope: &Value, dates: DateOrder) -> Result<String, Failure> {
    let data = envelope.get("data").ok_or_else(|| {
        Failure::runtime(
            "unreadable_response",
            "the service answered without any data in it",
        )
    })?;
    let today = envelope
        .get("context")
        .and_then(|context| context.get("today"))
        .and_then(Value::as_str);
    let out = Reading {
        data,
        today,
        dates,
        op: operation.op(),
    };

    Ok(match operation.view {
        View::None => String::new(),
        View::TaskList => out.task_list(),
        View::Task => out.task(),
        View::Tasks => out.tasks(),
        View::Order => out.order(),
        View::Day => out.day(),
        View::Backlog => out.backlog(),
        View::History => out.history(),
        View::Search => out.search(),
        View::Review => out.review(),
        View::Schedules => out.schedules(),
        View::Schedule => out.schedule(),
        View::Preview => out.preview(),
        View::Notes => out.notes(),
        View::Note => out.note(),
        View::NoteCheck => out.note_check(),
        View::NoteCopy => out.note_copied(),
        View::Settings => out.settings(),
        View::Dictionary => out.dictionary(),
        View::Word => out.word(),
        View::Undo => out.undo(),
        View::Undone => out.undone(),
        View::Refresh => out.refresh(),
    })
}

/// One answer being read.
struct Reading<'a> {
    data: &'a Value,
    today: Option<&'a str>,
    dates: DateOrder,
    op: &'a str,
}

/// A field of an object, or nothing.
fn at<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key).filter(|found| !found.is_null())
}

fn text_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    at(value, key).and_then(Value::as_str)
}

fn number_at(value: &Value, key: &str) -> Option<i64> {
    at(value, key).and_then(Value::as_i64)
}

fn flag_at(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn rows<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    at(value, key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// `1 entry` or `4 entries`. Both forms are written out: the English
/// plural is not a rule a summary line should be guessing at.
fn count(n: i64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

impl Reading<'_> {
    /// An ISO date, written the way the settings write one. Text that is
    /// not a date is passed through: the service owns what a date is, and
    /// a reading of an answer is no place to start refusing one.
    fn date(&self, iso: &str) -> String {
        match iso.parse::<Date>() {
            Ok(date) => {
                let label = day_label(date, self.dates);
                if Some(iso) == self.today {
                    format!("{label} (today)")
                } else {
                    label
                }
            }
            Err(_) => iso.to_owned(),
        }
    }

    /// An instant, as the day and the time of it. The day is written
    /// plainly: "closed today 14:35" would be saying the same thing twice
    /// on a line that already names the day the task is on.
    fn stamp(&self, text: &str) -> String {
        match text.split_once('T') {
            Some((day, rest)) => {
                let time = rest.get(..5).unwrap_or_default();
                let day = match day.parse::<Date>() {
                    Ok(day) => day_label(day, self.dates),
                    Err(_) => day.to_owned(),
                };
                format!("{day} {time}")
            }
            None => text.to_owned(),
        }
    }

    /// `today`, `Fri 5 Sep` or `backlog`.
    fn place(&self, place: &Value) -> String {
        match text_at(place, "kind") {
            Some("backlog") => "backlog".to_owned(),
            Some("day") => text_at(place, "day")
                .map(|day| self.date(day))
                .unwrap_or_else(|| "a day".to_owned()),
            _ => "somewhere".to_owned(),
        }
    }

    /// The one line a task row is.
    ///
    /// `[ ]` open, `[*]` a focus item, `[x]` closed, and then whatever the
    /// row says about itself, each thing after a `·` so that the line reads
    /// as a sentence and greps as fields.
    fn row(&self, row: &Value, place: bool) -> String {
        let id = number_at(row, "id").unwrap_or_default();
        let title = text_at(row, "title").unwrap_or_default();
        let closed = at(row, "closed_at").is_some()
            || row.get("open").and_then(Value::as_bool) == Some(false);
        let mark = if closed {
            "[x]"
        } else if flag_at(row, "focus") {
            "[*]"
        } else {
            "[ ]"
        };

        let mut line = format!("  {id:>4}  {mark} {title}");
        let mut note = |text: String| {
            let _ = write!(line, " · {text}");
        };

        if place && let Some(place) = at(row, "place") {
            note(self.place(place));
        }
        if flag_at(row, "waiting") {
            note("waiting".to_owned());
        }
        if flag_at(row, "was_focus") {
            note("was focus".to_owned());
        }
        // A view row carries its chips as objects; a task row carries the
        // bare dates. Both are read, because both shapes reach this line.
        match at(row, "due") {
            Some(due) if due.is_object() => {
                let on = text_at(due, "on").unwrap_or_default();
                let overdue = if flag_at(due, "overdue") {
                    ", overdue"
                } else {
                    ""
                };
                note(format!("due {}{overdue}", self.date(on)));
            }
            _ => {
                if let Some(on) = text_at(row, "due_on") {
                    note(format!("due {}", self.date(on)));
                }
            }
        }
        match at(row, "remind") {
            Some(remind) if remind.is_object() => {
                if let Some(on) = text_at(remind, "on") {
                    note(format!("remind {}", self.date(on)));
                }
            }
            Some(remind) if remind.is_string() => {
                note(format!(
                    "remind {}",
                    self.date(remind.as_str().unwrap_or_default())
                ));
            }
            _ => {
                if let Some(on) = text_at(row, "remind_on") {
                    note(format!("remind {}", self.date(on)));
                }
            }
        }
        if let Some(repeat) = at(row, "repeat") {
            note(format!("repeats {}", rule(repeat)));
        } else if let Some(schedule) = at(row, "schedule") {
            note(format!(
                "repeats {}",
                at(schedule, "rule")
                    .map(rule)
                    .unwrap_or_else(|| "on a schedule".to_owned())
            ));
        }
        if flag_at(row, "on_the_pile") {
            note("on the pile".to_owned());
        }
        if flag_at(row, "still_open") {
            note("still open".to_owned());
        }
        if flag_at(row, "from_backlog") {
            note("from the backlog".to_owned());
        }
        if let Some(closed_at) = text_at(row, "closed_at") {
            note(format!("closed {}", self.stamp(closed_at)));
        }
        line
    }

    /// A group of rows under its name, or nothing where the group is empty.
    fn group(&self, out: &mut String, name: &str, rows: &[Value], place: bool) {
        if rows.is_empty() {
            return;
        }
        let _ = writeln!(out, "{name}");
        for row in rows {
            let _ = writeln!(out, "{}", self.row(row, place));
        }
    }

    /// The undo entry a mutation pushed, as the line that says how to take
    /// it back. A mutation that pushed none says nothing.
    fn undo_line(&self, out: &mut String) {
        let Some(entry) = at(self.data, "undo") else {
            return;
        };
        let Some(id) = number_at(entry, "id") else {
            return;
        };
        let label = text_at(entry, "label").unwrap_or("that change");
        let _ = writeln!(out, "undo: {label} · jobsdone undo apply --entry {id}");
    }

    // ---- reads ------------------------------------------------------

    fn task_list(&self) -> String {
        let tasks = rows(self.data, "tasks");
        if tasks.is_empty() {
            return "No tasks.\n".to_owned();
        }
        let mut out = String::new();
        let mut place = None;
        for task in tasks {
            let here = at(task, "place").map(|place| self.place(place));
            if here != place {
                if place.is_some() {
                    out.push('\n');
                }
                let _ = writeln!(out, "{}", here.clone().unwrap_or_default());
                place = here;
            }
            let _ = writeln!(out, "{}", self.row(task, false));
        }
        let total = number_at(self.data, "total").unwrap_or(tasks.len() as i64);
        let _ = writeln!(out, "\n{}", count(total, "task", "tasks"));
        out
    }

    fn day(&self) -> String {
        let mut out = String::new();
        let day = text_at(self.data, "day").unwrap_or_default();
        let counts = self.data.get("counts").cloned().unwrap_or(Value::Null);
        let mut parts = Vec::new();
        for (key, name) in [
            ("planned", "planned"),
            ("open", "open"),
            ("done", "done"),
            ("moved", "moved"),
        ] {
            if let Some(n) = number_at(&counts, key) {
                parts.push(format!("{n} {name}"));
            }
        }
        let _ = writeln!(out, "{} · {}", self.date(day), parts.join(" · "));
        out.push('\n');
        self.group(&mut out, "Focus", rows(self.data, "focus"), false);
        self.group(&mut out, "Plan", rows(self.data, "plan"), false);
        self.group(&mut out, "Done", rows(self.data, "done"), false);
        self.group(&mut out, "Moved", rows(self.data, "moved"), true);
        if out.trim_end().lines().count() <= 1 {
            out.push_str("Nothing planned.\n");
        }
        out
    }

    fn backlog(&self) -> String {
        let mut out = String::new();
        let open = number_at(self.data, "open").unwrap_or_default();
        let waiting = number_at(self.data, "waiting_count").unwrap_or_default();
        let _ = writeln!(
            out,
            "Backlog · {} · {} waiting",
            count(open, "open task", "open tasks"),
            waiting
        );
        out.push('\n');
        // The ordinary group has no name of its own: the header above it
        // has just said "Backlog", and a group called that under it would
        // be the same word twice.
        for row in rows(self.data, "ordinary") {
            let _ = writeln!(out, "{}", self.row(row, false));
        }
        self.group(&mut out, "Waiting", rows(self.data, "waiting"), false);

        let schedules = rows(self.data, "schedules");
        if !schedules.is_empty() {
            let _ = writeln!(out, "Schedules");
            for schedule in schedules {
                let _ = writeln!(
                    out,
                    "  {:>4}  {} · {}",
                    number_at(schedule, "id").unwrap_or_default(),
                    text_at(schedule, "title").unwrap_or_default(),
                    at(schedule, "rule").map(rule).unwrap_or_default()
                );
            }
        }
        out
    }

    fn history(&self) -> String {
        let mut out = String::new();
        for stretch in rows(self.data, "stretches") {
            let name = match text_at(stretch, "stretch") {
                Some("later") => "Later",
                Some("this_week") => "This week",
                Some("last_week") => "Last week",
                _ => "Earlier",
            };
            let _ = writeln!(out, "{name}");
            for day in rows(stretch, "days") {
                let done = number_at(day, "done").unwrap_or_default();
                let kept = number_at(day, "kept").unwrap_or_default();
                let open = number_at(day, "open").unwrap_or_default();
                let open = if open > 0 {
                    format!(" · {open} open")
                } else {
                    String::new()
                };
                let _ = writeln!(
                    out,
                    "  {}  {done} of {kept} done{open}",
                    self.date(text_at(day, "day").unwrap_or_default())
                );
            }
        }
        if out.is_empty() {
            return "No days with anything on them.\n".to_owned();
        }
        let _ = writeln!(
            out,
            "\n{}",
            count(
                number_at(self.data, "total_days").unwrap_or_default(),
                "day",
                "days"
            )
        );
        out
    }

    fn search(&self) -> String {
        let mut out = String::new();
        self.group(&mut out, "Open", rows(self.data, "open"), true);
        self.group(&mut out, "Closed", rows(self.data, "closed"), true);
        if out.is_empty() {
            return "Nothing found.\n".to_owned();
        }
        let _ = writeln!(
            out,
            "\n{}",
            count(
                number_at(self.data, "total").unwrap_or_default(),
                "result",
                "results"
            )
        );
        out
    }

    fn review(&self) -> String {
        let mut out = String::new();
        let pile = self.data.get("pile").cloned().unwrap_or(Value::Null);
        let surfaced = self.data.get("surfaced").cloned().unwrap_or(Value::Null);
        let gate = self.data.get("gate").cloned().unwrap_or(Value::Null);

        if let Some(started) = self.data.get("started").and_then(Value::as_bool) {
            let _ = writeln!(
                out,
                "{}",
                if started {
                    "The review has been started."
                } else {
                    "The review had already been started today."
                }
            );
        }
        let _ = writeln!(
            out,
            "Review · {} on the pile · {} surfaced",
            number_at(&pile, "total").unwrap_or_default(),
            number_at(&surfaced, "total").unwrap_or_default()
        );
        if !gate.is_null() {
            let _ = writeln!(
                out,
                "started today: {} · opens itself: {} · would open: {}",
                yes_no(flag_at(&gate, "started_today")),
                yes_no(flag_at(&gate, "opens_itself")),
                yes_no(flag_at(&gate, "would_open"))
            );
        }
        out.push('\n');

        let days = rows(&pile, "days");
        if !days.is_empty() {
            let _ = writeln!(out, "Pile");
            for day in days {
                let age = number_at(day, "age").unwrap_or_default();
                let _ = writeln!(
                    out,
                    "  {} · {}",
                    self.date(text_at(day, "day").unwrap_or_default()),
                    count(age, "day ago", "days ago")
                );
                for row in rows(day, "rows") {
                    let _ = writeln!(out, "  {}", self.row(row, false));
                }
            }
        }
        self.group(&mut out, "Due", rows(&surfaced, "due"), false);
        self.group(&mut out, "Reminders", rows(&surfaced, "reminders"), false);
        self.group(
            &mut out,
            "Also starting today",
            rows(&surfaced, "also_starting_today"),
            false,
        );
        out
    }

    fn schedules(&self) -> String {
        let schedules = rows(self.data, "schedules");
        if schedules.is_empty() {
            return "No schedules.\n".to_owned();
        }
        let mut out = String::new();
        for schedule in schedules {
            let _ = writeln!(out, "{}", self.schedule_line(schedule));
        }
        let _ = writeln!(
            out,
            "\n{}",
            count(
                number_at(self.data, "total").unwrap_or(schedules.len() as i64),
                "schedule",
                "schedules"
            )
        );
        out
    }

    fn schedule_line(&self, schedule: &Value) -> String {
        let mut line = format!(
            "  {:>4}  {} · {}",
            number_at(schedule, "id").unwrap_or_default(),
            text_at(schedule, "title").unwrap_or_default(),
            at(schedule, "rule").map(rule).unwrap_or_default()
        );
        if let Some(copies) = number_at(schedule, "copies") {
            let _ = write!(line, " · {}", count(copies, "copy", "copies"));
        }
        if let Some(stopped) = text_at(schedule, "stopped_on") {
            let _ = write!(line, " · stopped {}", self.date(stopped));
        }
        line
    }

    fn schedule(&self) -> String {
        let mut out = String::new();
        if !self.op.starts_with("schedule.get") {
            let _ = writeln!(out, "{}", self.said());
        }
        if let Some(schedule) = at(self.data, "schedule") {
            let _ = writeln!(out, "{}", self.schedule_line(schedule));
            if let Some(through) = text_at(schedule, "generated_through") {
                let _ = writeln!(out, "        copies made through {}", self.date(through));
            }
        }
        if let Some(task) = at(self.data, "task") {
            let _ = writeln!(out, "{}", self.row(task, true));
        }
        let next = rows(self.data, "next");
        if !next.is_empty() {
            let dates: Vec<String> = next
                .iter()
                .filter_map(Value::as_str)
                .map(|date| self.date(date))
                .collect();
            let _ = writeln!(out, "        next {}", dates.join(", "));
        }
        self.undo_line(&mut out);
        out
    }

    fn preview(&self) -> String {
        let dates = rows(self.data, "dates");
        if dates.is_empty() {
            return "The rule falls on no day in range.\n".to_owned();
        }
        let mut out = String::new();
        for date in dates.iter().filter_map(Value::as_str) {
            let _ = writeln!(out, "  {}", self.date(date));
        }
        out
    }

    fn notes(&self) -> String {
        let notes = rows(self.data, "notes");
        if notes.is_empty() {
            return "No notes.\n".to_owned();
        }
        let mut out = String::new();
        for note in notes {
            let first = text_at(note, "first_line")
                .map(str::to_owned)
                .or_else(|| {
                    text_at(note, "body")
                        .map(|body| body.lines().next().unwrap_or_default().to_owned())
                })
                .unwrap_or_default();
            // An archived note is dated by when it was archived, which is
            // the order the archive is in.
            let stamp = match text_at(note, "archived_at") {
                Some(archived) => format!("archived {}", self.stamp(archived)),
                None => self.stamp(text_at(note, "created_at").unwrap_or_default()),
            };
            let _ = writeln!(
                out,
                "  {:>4}  {first} · {stamp}",
                number_at(note, "id").unwrap_or_default(),
            );
        }
        let _ = writeln!(
            out,
            "\n{}",
            count(
                number_at(self.data, "count").unwrap_or(notes.len() as i64),
                "note",
                "notes"
            )
        );
        out
    }

    fn note(&self) -> String {
        let Some(note) = at(self.data, "note") else {
            return format!("{}\n", self.said());
        };
        let mut out = String::new();
        let id = number_at(note, "id").unwrap_or_default();
        if self.op == "note.get" {
            let _ = write!(
                out,
                "Note {id} · created {} · updated {}",
                self.stamp(text_at(note, "created_at").unwrap_or_default()),
                self.stamp(text_at(note, "updated_at").unwrap_or_default())
            );
            if let Some(archived) = text_at(note, "archived_at") {
                let _ = write!(out, " · archived {}", self.stamp(archived));
            }
            out.push('\n');
            out.push('\n');
            out.push_str(text_at(note, "body").unwrap_or_default());
            if !out.ends_with('\n') {
                out.push('\n');
            }
            return out;
        }
        let _ = writeln!(out, "{} {id}.", self.said());
        self.undo_line(&mut out);
        out
    }

    fn note_copied(&self) -> String {
        let id = at(self.data, "note")
            .and_then(|note| number_at(note, "id"))
            .unwrap_or_default();
        format!("Note {id} is on the clipboard.\n")
    }

    fn note_check(&self) -> String {
        let found = rows(self.data, "misspellings");
        let setting = flag_at(self.data, "spell_check_notes");
        let mut out = String::new();
        if found.is_empty() {
            out.push_str("Nothing looks misspelt.\n");
        } else {
            for one in found {
                let word = text_at(one, "word").unwrap_or_default();
                let start = number_at(one, "start").unwrap_or_default();
                let end = number_at(one, "end").unwrap_or_default();
                let suggestions: Vec<&str> = rows(one, "suggestions")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                let said = if suggestions.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", suggestions.join(", "))
                };
                let _ = writeln!(out, "  {start}-{end}  {word}{said}");
            }
            let _ = writeln!(
                out,
                "\n{}",
                count(found.len() as i64, "possible mistake", "possible mistakes")
            );
        }
        let _ = writeln!(
            out,
            "spell-check notes is {}",
            if setting { "on" } else { "off" }
        );
        out
    }

    fn settings(&self) -> String {
        let Some(settings) = at(self.data, "settings").and_then(Value::as_object) else {
            return "No settings.\n".to_owned();
        };
        let mut out = String::new();
        if self.op == "settings.set" {
            let _ = writeln!(out, "Saved.");
        }
        let width = settings.keys().map(String::len).max().unwrap_or(0);
        for (key, value) in settings {
            let _ = writeln!(out, "  {key:<width$}  {}", plain(value));
        }
        if let Some(order) = text_at(self.data, "date_order") {
            let written = match order {
                "month_first" => "the month before the day",
                _ => "the day before the month",
            };
            let _ = writeln!(out, "\nDates are written {written}.");
        }
        if let Some(desktop) = at(self.data, "desktop")
            && flag_at(desktop, "window_rule_changed")
        {
            out.push_str(
                "\nThe window settings changed. Run `jobsdone desktop` to write the\n\
                 window rule and reload it; this command does not touch the window\n\
                 manager itself.\n",
            );
        }
        out
    }

    fn dictionary(&self) -> String {
        let words = rows(self.data, "words");
        if words.is_empty() {
            return "The personal dictionary is empty.\n".to_owned();
        }
        let mut out = String::new();
        for word in words {
            let _ = writeln!(out, "  {}", text_at(word, "word").unwrap_or_default());
        }
        let _ = writeln!(
            out,
            "\n{}",
            count(
                number_at(self.data, "count").unwrap_or(words.len() as i64),
                "word",
                "words"
            )
        );
        out
    }

    fn word(&self) -> String {
        let word = at(self.data, "word")
            .and_then(|word| text_at(word, "word"))
            .unwrap_or_default();
        format!("{} {word}.\n", self.said())
    }

    fn undo(&self) -> String {
        let depth = number_at(self.data, "depth").unwrap_or_default();
        match at(self.data, "entry") {
            None => "There is nothing to undo.\n".to_owned(),
            Some(entry) => format!(
                "{} · entry {} · {} on the stack\n",
                text_at(entry, "label").unwrap_or_default(),
                number_at(entry, "id").unwrap_or_default(),
                count(depth, "entry", "entries")
            ),
        }
    }

    fn undone(&self) -> String {
        let mut out = String::new();
        let label = text_at(self.data, "label").unwrap_or("that change");
        if flag_at(self.data, "applied") {
            let _ = writeln!(out, "Took back: {label}");
        } else {
            let _ = writeln!(out, "Dropped: {label}");
        }
        if let Some(dropped) = text_at(self.data, "dropped") {
            let _ = writeln!(out, "{dropped}");
        }
        let _ = writeln!(
            out,
            "{} left on the stack",
            count(
                number_at(self.data, "depth").unwrap_or_default(),
                "entry",
                "entries"
            )
        );
        out
    }

    fn refresh(&self) -> String {
        let made = rows(self.data, "created");
        let mut out = String::new();
        if made.is_empty() {
            return "There was nothing to make.\n".to_owned();
        }
        let _ = writeln!(out, "Made {}.", count(made.len() as i64, "copy", "copies"));
        for task in made {
            let _ = writeln!(out, "{}", self.row(task, true));
        }
        out
    }

    // ---- mutations --------------------------------------------------

    fn task(&self) -> String {
        let mut out = String::new();
        if self.op != "task.get" {
            let _ = writeln!(out, "{}.", self.said());
        }
        if let Some(task) = at(self.data, "task") {
            let _ = writeln!(out, "{}", self.row(task, true));
        }
        if let Some(schedule) = at(self.data, "schedule") {
            let _ = writeln!(out, "{}", self.schedule_line(schedule));
        }
        self.undo_line(&mut out);
        out
    }

    fn tasks(&self) -> String {
        let tasks = rows(self.data, "tasks");
        let n = number_at(self.data, "count").unwrap_or(tasks.len() as i64);
        let mut out = format!("{} {}.\n", self.said(), count(n, "task", "tasks"));
        for task in tasks {
            let _ = writeln!(out, "{}", self.row(task, true));
        }
        self.undo_line(&mut out);
        out
    }

    fn order(&self) -> String {
        let mut out = String::new();
        let place = at(self.data, "place")
            .map(|place| self.place(place))
            .unwrap_or_else(|| "the place".to_owned());
        let _ = writeln!(out, "{place} is now:");
        for task in rows(self.data, "order") {
            let _ = writeln!(out, "{}", self.row(task, false));
        }
        self.undo_line(&mut out);
        out
    }

    /// What the operation did, as the word a summary opens with.
    fn said(&self) -> &'static str {
        match self.op {
            "task.add" => "Added",
            "task.update" => "Changed",
            "task.close" => "Closed",
            "task.reopen" => "Reopened",
            "task.move" => "Moved",
            "task.delete" => "Deleted",
            "schedule.create" => "Added a schedule",
            "schedule.update" => "Changed the schedule",
            "schedule.stop" => "Stopped the schedule",
            "note.create" => "Added note",
            "note.update" => "Rewrote note",
            "note.delete" => "Deleted note",
            "note.archive" => "Archived note",
            "note.unarchive" => "Unarchived note",
            "dictionary.add" => "Added",
            "dictionary.update" => "Changed",
            "dictionary.delete" => "Removed",
            _ => "Done",
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// A settings value, written without its JSON quoting.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(true) => "on".to_owned(),
        Value::Bool(false) => "off".to_owned(),
        Value::Array(items) => items.iter().map(plain).collect::<Vec<String>>().join(","),
        Value::Object(fields) => match (fields.get("width"), fields.get("height")) {
            (Some(width), Some(height)) => format!("{}x{}", plain(width), plain(height)),
            _ => value.to_string(),
        },
        other => other.to_string(),
    }
}

/// A repeat rule, in the words the app uses for it.
pub fn rule(rule: &Value) -> String {
    match text_at(rule, "kind") {
        Some("workdays") => "every work day".to_owned(),
        Some("daily") => "every day".to_owned(),
        Some("weekly") => {
            let days: Vec<&str> = rows(rule, "weekdays")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            format!("every {}", days.join(", "))
        }
        Some("monthly") => match at(rule, "day") {
            Some(Value::String(last)) if last == "last" => "the last day of the month".to_owned(),
            Some(day) => format!("the {} of the month", plain(day)),
            None => "monthly".to_owned(),
        },
        Some("every_n_weeks") => {
            let n = number_at(rule, "n").unwrap_or(1);
            let from = text_at(rule, "from").unwrap_or_default();
            if n == 1 {
                format!("every week from {from}")
            } else {
                format!("every {n} weeks from {from}")
            }
        }
        _ => "on a schedule".to_owned(),
    }
}
