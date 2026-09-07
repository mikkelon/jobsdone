//! The rows the model is made of, the model as one value, and the writes
//! a change is a list of.

use std::collections::BTreeMap;

use jiff::Zoned;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use super::rule::Rule;
use super::settings::Settings;

/// The id of a row. New ids come from the domain: the largest one in the
/// model plus one (ARCHITECTURE.md section 3).
pub type Id = i64;

/// Where a task lives: the backlog, or one day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Place {
    Backlog,
    Day(Date),
}

impl Place {
    /// The place a `Task::day` describes.
    pub fn of(day: Option<Date>) -> Place {
        match day {
            Some(day) => Place::Day(day),
            None => Place::Backlog,
        }
    }

    /// The date to write into `Task::day`.
    pub fn day(self) -> Option<Date> {
        match self {
            Place::Backlog => None,
            Place::Day(day) => Some(day),
        }
    }
}

/// A title, whether it is done, and where it lives. Nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: Id,
    pub title: String,
    pub day: Option<Date>,
    pub position: usize,
    pub focus: bool,
    pub waiting: bool,
    pub closed_at: Option<Zoned>,
    pub due_on: Option<Date>,
    pub remind_on: Option<Date>,
    pub schedule_id: Option<Id>,
    pub scheduled_on: Option<Date>,
    pub created_at: Zoned,
    pub deleted_at: Option<Zoned>,
}

impl Task {
    /// Live means `deleted_at` is none. Deleted tasks are invisible to
    /// every view, every count, search and generation, but their rows
    /// stay (DOMAIN.md section 11).
    pub fn is_live(&self) -> bool {
        self.deleted_at.is_none()
    }

    pub fn is_open(&self) -> bool {
        self.closed_at.is_none()
    }

    pub fn place(&self) -> Place {
        Place::of(self.day)
    }
}

/// Where a task came from when it arrived on a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FromPlace {
    New,
    Backlog,
    Day(Date),
}

impl FromPlace {
    pub fn of(place: Place) -> FromPlace {
        match place {
            Place::Backlog => FromPlace::Backlog,
            Place::Day(day) => FromPlace::Day(day),
        }
    }
}

/// The record that a task was put on a day, kept after the task leaves.
/// One row per task and day, written on arrival and never updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub task_id: Id,
    pub day: Date,
    pub placed_at: Zoned,
    pub from_place: FromPlace,
}

/// A repeat rule with a title. It creates copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub id: Id,
    pub title: String,
    pub rule: Rule,
    /// Copies exist for every scheduled date up to and including this.
    pub generated_through: Date,
    pub stopped_on: Option<Date>,
    pub created_at: Zoned,
}

impl Schedule {
    pub fn is_stopped(&self) -> bool {
        self.stopped_on.is_some()
    }
}

/// A plain-text scratchpad entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub id: Id,
    pub body: String,
    pub created_at: Zoned,
    pub updated_at: Zoned,
    pub deleted_at: Option<Zoned>,
}

impl Note {
    pub fn is_live(&self) -> bool {
        self.deleted_at.is_none()
    }
}

/// One entry of the undo stack: when, what to call it in the hint bar,
/// and the inverse command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoEntry {
    pub id: Id,
    pub at: Zoned,
    pub label: String,
    pub inverse: super::Command,
}

/// The `meta` keys DOMAIN.md section 17 names.
pub const REVIEW_ON: &str = "review_on";
pub const REVIEW_BEFORE: &str = "review_before";

/// The whole state as a value, loaded from storage in one go. Views are
/// pure functions of a model and a date.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    /// Every task, deleted ones included: a deleted copy keeps its row so
    /// that generation never makes it again.
    pub tasks: BTreeMap<Id, Task>,
    pub placements: BTreeMap<(Id, Date), Placement>,
    pub schedules: BTreeMap<Id, Schedule>,
    pub notes: BTreeMap<Id, Note>,
    /// The undo stack, oldest first. Shared by every running instance.
    pub undo: Vec<UndoEntry>,
    /// The `meta` table: `review_on`, `review_before`.
    pub meta: BTreeMap<String, String>,
    /// The `settings` table, read into one typed value (DOMAIN.md
    /// section 19).
    pub settings: Settings,
    /// The personal dictionary: a canonical key to the word as it was
    /// typed, so that a checker can ask about a word in any
    /// capitalisation and the manager can show the one written here.
    pub personal_dictionary: BTreeMap<String, String>,
}

impl Model {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn task(&self, id: Id) -> Option<&Task> {
        self.tasks.get(&id)
    }

    pub fn live_task(&self, id: Id) -> Option<&Task> {
        self.tasks.get(&id).filter(|task| task.is_live())
    }

    pub fn schedule(&self, id: Id) -> Option<&Schedule> {
        self.schedules.get(&id)
    }

    pub fn note(&self, id: Id) -> Option<&Note> {
        self.notes.get(&id)
    }

    /// The live tasks of a place in their order. The positions of a place
    /// are the dense integers `0..n` in one sequence, whatever the groups
    /// a screen draws (DOMAIN.md section 4).
    pub fn place(&self, place: Place) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .values()
            .filter(|task| task.is_live() && task.place() == place)
            .collect();
        tasks.sort_by_key(|task| (task.position, task.id));
        tasks
    }

    /// A date the model holds under a `meta` key.
    pub fn meta_date(&self, key: &str) -> Option<Date> {
        self.meta.get(key).and_then(|text| text.parse().ok())
    }

    /// The record that a task was put on a day.
    pub fn placement(&self, task: Id, day: Date) -> Option<&Placement> {
        self.placements.get(&(task, day))
    }

    /// Applies a change that storage has already committed.
    pub fn apply(&mut self, change: &Change) {
        for write in &change.writes {
            match write {
                Write::PutTask(task) => {
                    self.tasks.insert(task.id, task.clone());
                }
                Write::PutPlacement(placement) => {
                    self.placements
                        .insert((placement.task_id, placement.day), placement.clone());
                }
                Write::DeletePlacement { task, day } => {
                    self.placements.remove(&(*task, *day));
                }
                Write::PutSchedule(schedule) => {
                    self.schedules.insert(schedule.id, schedule.clone());
                }
                Write::DeleteSchedule(id) => {
                    self.schedules.remove(id);
                }
                Write::PutNote(note) => {
                    self.notes.insert(note.id, note.clone());
                }
                Write::PushUndo(entry) => self.undo.push(entry.clone()),
                Write::PopUndo(id) => self.undo.retain(|entry| entry.id != *id),
                Write::TruncateUndo(cap) => {
                    let over = self.undo.len().saturating_sub(*cap);
                    self.undo.drain(..over);
                }
                Write::SetMeta { key, value } => {
                    self.meta.insert(key.clone(), value.clone());
                }
                Write::PutSettings(settings) => self.settings = settings.clone(),
                Write::PutDictionaryWord { key, word } => {
                    self.personal_dictionary.insert(key.clone(), word.clone());
                }
                Write::DeleteDictionaryWord { key } => {
                    self.personal_dictionary.remove(key);
                }
            }
        }
    }
}

/// What a command does, as a list of row writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub writes: Vec<Write>,
}

/// One row write. Whole rows, never fields: a renumbered place is one
/// `PutTask` per shifted task (ARCHITECTURE.md section 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    PutTask(Task),
    PutPlacement(Placement),
    /// Only ever from the undo of the move that wrote the row.
    DeletePlacement {
        task: Id,
        day: Date,
    },
    PutSchedule(Schedule),
    /// Only ever from the undo of the CreateSchedule that wrote the row,
    /// when the schedule's only copy is the task being unlinked.
    DeleteSchedule(Id),
    PutNote(Note),
    PushUndo(UndoEntry),
    PopUndo(Id),
    /// Delete the lowest ids beyond the cap.
    TruncateUndo(usize),
    SetMeta {
        key: String,
        value: String,
    },
    /// Every settings row at once: the table is one value, so a change
    /// to it is one write.
    PutSettings(Settings),
    /// One personal dictionary entry, insert or replace. A row apiece,
    /// unlike the settings, so that two windows adding two words keep
    /// both.
    PutDictionaryWord {
        key: String,
        word: String,
    },
    DeleteDictionaryWord {
        key: String,
    },
}

/// The writes that turn one model into another: whole rows, and in an
/// order storage's foreign keys accept, parents before children.
pub(super) fn diff(before: &Model, after: &Model) -> Vec<Write> {
    let mut writes = Vec::new();

    for (id, schedule) in &after.schedules {
        if before.schedules.get(id) != Some(schedule) {
            writes.push(Write::PutSchedule(schedule.clone()));
        }
    }
    for (id, task) in &after.tasks {
        if before.tasks.get(id) != Some(task) {
            writes.push(Write::PutTask(task.clone()));
        }
    }
    for (key, placement) in &after.placements {
        if before.placements.get(key) != Some(placement) {
            writes.push(Write::PutPlacement(placement.clone()));
        }
    }
    for (task, day) in before.placements.keys() {
        if !after.placements.contains_key(&(*task, *day)) {
            writes.push(Write::DeletePlacement {
                task: *task,
                day: *day,
            });
        }
    }
    // After the tasks that pointed at it have let go.
    for id in before.schedules.keys() {
        if !after.schedules.contains_key(id) {
            writes.push(Write::DeleteSchedule(*id));
        }
    }
    for (id, note) in &after.notes {
        if before.notes.get(id) != Some(note) {
            writes.push(Write::PutNote(note.clone()));
        }
    }
    for (key, value) in &after.meta {
        if before.meta.get(key) != Some(value) {
            writes.push(Write::SetMeta {
                key: key.clone(),
                value: value.clone(),
            });
        }
    }
    if before.settings != after.settings {
        writes.push(Write::PutSettings(after.settings.clone()));
    }
    for (key, word) in &after.personal_dictionary {
        if before.personal_dictionary.get(key) != Some(word) {
            writes.push(Write::PutDictionaryWord {
                key: key.clone(),
                word: word.clone(),
            });
        }
    }
    for key in before.personal_dictionary.keys() {
        if !after.personal_dictionary.contains_key(key) {
            writes.push(Write::DeleteDictionaryWord { key: key.clone() });
        }
    }
    writes
}
