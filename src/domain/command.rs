//! Every change to the model is one command (DOMAIN.md section 12).
//!
//! A command is checked against the model, applied to a copy of it, and
//! the difference is the change storage commits. The commands at the end
//! of the enum are only ever produced as the inverse of another one; they
//! push nothing on the undo stack, because undo pushes nothing.

use jiff::Zoned;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use super::model::{
    Change, FromPlace, Id, Model, Note, Place, Placement, Schedule, Task, UndoEntry, Write, diff,
};
use super::rule::Rule;
use super::{Rejected, working_day};

/// One change to the model, carrying ids and never cursor positions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    AddTask {
        title: String,
        place: Place,
    },
    EditTitle {
        task: Id,
        title: String,
    },
    /// "This and future copies": the task and its schedule. A user gives
    /// the same title twice; only the inverse has two of its own.
    EditTitleAndFuture {
        task: Id,
        title: String,
        schedule_title: String,
    },
    Close {
        task: Id,
    },
    Reopen {
        task: Id,
    },
    SetFocus {
        task: Id,
        focus: bool,
    },
    Move {
        task: Id,
        place: Place,
    },
    Reorder {
        task: Id,
        position: usize,
    },
    SetWaiting {
        task: Id,
        waiting: bool,
    },
    SetDue {
        task: Id,
        date: Option<Date>,
    },
    SetRemind {
        task: Id,
        date: Option<Date>,
    },
    DeleteTask {
        task: Id,
    },
    CreateSchedule {
        task: Id,
        rule: Rule,
    },
    SetRule {
        schedule: Id,
        rule: Rule,
    },
    StopSchedule {
        schedule: Id,
    },
    CreateNote,
    EditNote {
        note: Id,
        body: String,
    },
    DeleteNote {
        note: Id,
    },

    // The inverses, which say more than a user ever does.
    /// The inverse of Move, of Close, and of the move inside SetWaiting:
    /// the task goes back to the place and position it had, open again,
    /// with its old waiting flag, and the placement row the command wrote
    /// is dropped. A close that stayed on its day undoes as a move back
    /// to where the task already is, which is only the reopening.
    MoveBack {
        task: Id,
        place: Place,
        position: usize,
        waiting: bool,
        drop_placement: Option<Date>,
    },
    /// The inverse of Reopen.
    CloseAt {
        task: Id,
        closed_at: Zoned,
        position: usize,
    },
    /// The inverse of DeleteTask.
    RestoreTask {
        task: Id,
        position: usize,
    },
    /// The inverse of CreateSchedule.
    UncreateSchedule {
        task: Id,
        schedule: Id,
    },
    /// The inverse of StopSchedule.
    ResumeSchedule {
        schedule: Id,
    },
    /// The inverse of DeleteNote.
    RestoreNote {
        note: Id,
    },
}

/// What `undo` did: the change to commit either way, the label of the
/// entry it popped, and, when the inverse no longer applied, why it was
/// dropped instead (DOMAIN.md section 11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undone {
    pub change: Change,
    pub label: String,
    pub dropped: Option<Rejected>,
}

/// Every user command: what it would change, or why it is refused.
///
/// `undo_cap` is the length the undo stack is held to. The domain does
/// not choose the number (DOMAIN.md section 11).
pub fn apply(
    model: &Model,
    command: Command,
    now: &Zoned,
    undo_cap: usize,
) -> Result<Change, Rejected> {
    let today = working_day(now);
    let mut after = model.clone();
    let entry = run(&mut after, &command, now, today)?;

    let mut writes = diff(model, &after);
    if let Some(Entry { label, inverse }) = entry {
        writes.push(Write::PushUndo(UndoEntry {
            id: next_id(model.undo.iter().map(|entry| entry.id)),
            at: now.clone(),
            label,
            inverse,
        }));
        writes.push(Write::TruncateUndo(undo_cap));
    }
    Ok(Change { writes })
}

/// Pops the top entry and returns its inverse's change, with nothing
/// pushed. An inverse whose precondition no longer holds, because another
/// window has moved on, drops the entry instead of applying it.
pub fn undo(model: &Model, now: &Zoned) -> Result<Undone, Rejected> {
    let Some(entry) = model.undo.last() else {
        return Err(Rejected("There is nothing to undo.".to_owned()));
    };
    let today = working_day(now);
    let mut after = model.clone();

    match run(&mut after, &entry.inverse, now, today) {
        Ok(_) => {
            let mut writes = diff(model, &after);
            writes.push(Write::PopUndo(entry.id));
            Ok(Undone {
                change: Change { writes },
                label: entry.label.clone(),
                dropped: None,
            })
        }
        Err(rejected) => Ok(Undone {
            change: Change {
                writes: vec![Write::PopUndo(entry.id)],
            },
            label: entry.label.clone(),
            dropped: Some(rejected),
        }),
    }
}

/// What a command earns on the undo stack.
struct Entry {
    label: String,
    inverse: Command,
}

impl Entry {
    fn new(label: String, inverse: Command) -> Option<Entry> {
        Some(Entry { label, inverse })
    }
}

/// Applies a command to a working copy of the model, or says why it
/// cannot. `None` is a command that pushes nothing on the undo stack.
fn run(
    model: &mut Model,
    command: &Command,
    now: &Zoned,
    today: Date,
) -> Result<Option<Entry>, Rejected> {
    match command {
        Command::AddTask { title, place } => {
            let title = valid_title(title)?;
            let id = next_id(model.tasks.keys().copied());
            let position = end_of(model, *place);
            model.tasks.insert(
                id,
                Task {
                    id,
                    title: title.clone(),
                    day: place.day(),
                    position,
                    focus: false,
                    waiting: false,
                    closed_at: None,
                    due_on: None,
                    remind_on: None,
                    schedule_id: None,
                    scheduled_on: None,
                    created_at: now.clone(),
                    deleted_at: None,
                },
            );
            if let Place::Day(day) = place {
                place_on_day(model, id, *day, now, FromPlace::New);
            }
            Ok(Entry::new(
                format!("Added {}", named(&title)),
                Command::DeleteTask { task: id },
            ))
        }

        Command::EditTitle { task, title } => {
            let title = valid_title(title)?;
            let was = live(model, *task)?.title.clone();
            set_task(model, *task, |task| task.title = title.clone());
            Ok(Entry::new(
                format!("Renamed {}", named(&title)),
                Command::EditTitle {
                    task: *task,
                    title: was,
                },
            ))
        }

        Command::EditTitleAndFuture {
            task,
            title,
            schedule_title,
        } => {
            let title = valid_title(title)?;
            let schedule_title = valid_title(schedule_title)?;
            let current = live(model, *task)?;
            let was = current.title.clone();
            let Some(schedule_id) = current.schedule_id else {
                return Err(Rejected("That task is not a recurring copy.".to_owned()));
            };
            let Some(schedule) = model.schedules.get_mut(&schedule_id) else {
                return Err(Rejected("That repeat schedule is gone.".to_owned()));
            };
            let was_schedule = schedule.title.clone();
            schedule.title = schedule_title.clone();
            set_task(model, *task, |task| task.title = title.clone());
            Ok(Entry::new(
                format!("Renamed {} and its future copies", named(&title)),
                Command::EditTitleAndFuture {
                    task: *task,
                    title: was,
                    schedule_title: was_schedule,
                },
            ))
        }

        Command::Close { task } => {
            let current = live(model, *task)?;
            if !current.is_open() {
                return Err(Rejected("That task is already closed.".to_owned()));
            }
            let (id, title) = (current.id, current.title.clone());
            let (from, position, waiting) = (current.place(), current.position, current.waiting);

            // A task closed out of the backlog was done today, so it goes
            // onto today's plan first and the day's record shows it.
            let mut drop_placement = None;
            if from == Place::Backlog {
                let end = end_of(model, Place::Day(today));
                drop_placement =
                    place_on_day(model, id, today, now, FromPlace::of(from)).then_some(today);
                put_in_place(model, id, Place::Day(today), end);
                set_task(model, id, |task| task.waiting = false);
            }
            set_task(model, id, |task| task.closed_at = Some(now.clone()));

            Ok(Entry::new(
                format!("Closed {}", named(&title)),
                Command::MoveBack {
                    task: id,
                    place: from,
                    position,
                    waiting,
                    drop_placement,
                },
            ))
        }

        Command::Reopen { task } => {
            let current = live(model, *task)?;
            let Some(closed_at) = current.closed_at.clone() else {
                return Err(Rejected("That task is not closed.".to_owned()));
            };
            let (id, title, position) = (current.id, current.title.clone(), current.position);
            set_task(model, id, |task| task.closed_at = None);
            set_position(model, id, usize::MAX);
            Ok(Entry::new(
                format!("Reopened {}", named(&title)),
                Command::CloseAt {
                    task: id,
                    closed_at,
                    position,
                },
            ))
        }

        Command::SetFocus { task, focus } => {
            let current = live(model, *task)?;
            if current.day.is_none() {
                return Err(Rejected("Focus is for tasks on a day.".to_owned()));
            }
            let (title, was) = (current.title.clone(), current.focus);
            set_task(model, *task, |task| task.focus = *focus);
            let label = if *focus { "Focused" } else { "Unfocused" };
            Ok(Entry::new(
                format!("{label} {}", named(&title)),
                Command::SetFocus {
                    task: *task,
                    focus: was,
                },
            ))
        }

        Command::Move { task, place } => {
            let current = live(model, *task)?;
            if !current.is_open() {
                return Err(Rejected(
                    "A closed task stays where it was done.".to_owned(),
                ));
            }
            if current.place() == *place {
                return Err(Rejected("The task is already there.".to_owned()));
            }
            let (id, title) = (current.id, current.title.clone());
            let (from, position, waiting) = (current.place(), current.position, current.waiting);

            let end = end_of(model, *place);
            let mut drop_placement = None;
            if let Place::Day(day) = place {
                drop_placement =
                    place_on_day(model, id, *day, now, FromPlace::of(from)).then_some(*day);
            }
            put_in_place(model, id, *place, end);
            set_task(model, id, |task| task.waiting = false);

            Ok(Entry::new(
                format!("Moved {} to {}", named(&title), place_name(*place, today)),
                Command::MoveBack {
                    task: id,
                    place: from,
                    position,
                    waiting,
                    drop_placement,
                },
            ))
        }

        Command::Reorder { task, position } => {
            let current = live(model, *task)?;
            let (id, title, was) = (current.id, current.title.clone(), current.position);
            set_position(model, id, *position);
            Ok(Entry::new(
                format!("Moved {} in the list", named(&title)),
                Command::Reorder {
                    task: id,
                    position: was,
                },
            ))
        }

        Command::SetWaiting { task, waiting } => {
            let current = live(model, *task)?;
            if !current.is_open() {
                return Err(Rejected(
                    "A closed task is not waiting on anyone.".to_owned(),
                ));
            }
            let (id, title) = (current.id, current.title.clone());
            let (from, position, was) = (current.place(), current.position, current.waiting);

            // Waiting is a backlog state, so flagging a task on a day
            // sends it to the backlog. One command, one undo entry.
            if *waiting && from != Place::Backlog {
                let end = end_of(model, Place::Backlog);
                put_in_place(model, id, Place::Backlog, end);
                set_task(model, id, |task| task.waiting = true);
                return Ok(Entry::new(
                    format!("Waiting on {}", named(&title)),
                    Command::MoveBack {
                        task: id,
                        place: from,
                        position,
                        waiting: false,
                        drop_placement: None,
                    },
                ));
            }
            set_task(model, id, |task| task.waiting = *waiting);
            let label = if *waiting {
                "Waiting on"
            } else {
                "No longer waiting on"
            };
            Ok(Entry::new(
                format!("{label} {}", named(&title)),
                Command::SetWaiting {
                    task: id,
                    waiting: was,
                },
            ))
        }

        Command::SetDue { task, date } => {
            let current = live(model, *task)?;
            let (title, was) = (current.title.clone(), current.due_on);
            set_task(model, *task, |task| task.due_on = *date);
            let label = match date {
                Some(_) => "Due date on",
                None => "Cleared the due date on",
            };
            Ok(Entry::new(
                format!("{label} {}", named(&title)),
                Command::SetDue {
                    task: *task,
                    date: was,
                },
            ))
        }

        Command::SetRemind { task, date } => {
            let current = live(model, *task)?;
            let (title, was) = (current.title.clone(), current.remind_on);
            set_task(model, *task, |task| task.remind_on = *date);
            let label = match date {
                Some(_) => "Reminder on",
                None => "Cleared the reminder on",
            };
            Ok(Entry::new(
                format!("{label} {}", named(&title)),
                Command::SetRemind {
                    task: *task,
                    date: was,
                },
            ))
        }

        Command::DeleteTask { task } => {
            let current = live(model, *task)?;
            let (id, title) = (current.id, current.title.clone());
            let (place, position) = (current.place(), current.position);
            set_task(model, id, |task| task.deleted_at = Some(now.clone()));
            renumber(model, place);
            Ok(Entry::new(
                format!("Deleted {}", named(&title)),
                Command::RestoreTask { task: id, position },
            ))
        }

        Command::CreateSchedule { task, rule } => {
            let current = live(model, *task)?;
            if current.schedule_id.is_some() {
                return Err(Rejected("That task already repeats.".to_owned()));
            }
            if !rule.is_usable() {
                return Err(Rejected("That repeat never comes round.".to_owned()));
            }
            let (id, title) = (current.id, current.title.clone());
            // The task is the schedule's first copy: its own date need
            // not match the rule (DOMAIN.md section 10).
            let scheduled_on = current.day.unwrap_or(today);
            let schedule = next_id(model.schedules.keys().copied());
            model.schedules.insert(
                schedule,
                Schedule {
                    id: schedule,
                    title: title.clone(),
                    rule: rule.clone(),
                    generated_through: scheduled_on,
                    stopped_on: None,
                    created_at: now.clone(),
                },
            );
            set_task(model, id, |task| {
                task.schedule_id = Some(schedule);
                task.scheduled_on = Some(scheduled_on);
            });
            Ok(Entry::new(
                format!("Set {} to repeat", named(&title)),
                Command::UncreateSchedule { task: id, schedule },
            ))
        }

        Command::SetRule { schedule, rule } => {
            let current = unstopped(model, *schedule)?;
            let (title, was) = (current.title.clone(), current.rule.clone());
            if !rule.is_usable() {
                return Err(Rejected("That repeat never comes round.".to_owned()));
            }
            if let Some(schedule) = model.schedules.get_mut(schedule) {
                schedule.rule = rule.clone();
            }
            Ok(Entry::new(
                format!("Changed the repeat of {}", named(&title)),
                Command::SetRule {
                    schedule: *schedule,
                    rule: was,
                },
            ))
        }

        Command::StopSchedule { schedule } => {
            let title = unstopped(model, *schedule)?.title.clone();
            if let Some(schedule) = model.schedules.get_mut(schedule) {
                schedule.stopped_on = Some(today);
            }
            Ok(Entry::new(
                format!("Stopped repeating {}", named(&title)),
                Command::ResumeSchedule {
                    schedule: *schedule,
                },
            ))
        }

        Command::CreateNote => {
            let id = next_id(model.notes.keys().copied());
            model.notes.insert(
                id,
                Note {
                    id,
                    body: String::new(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    deleted_at: None,
                },
            );
            Ok(Entry::new(
                "Added a note".to_owned(),
                Command::DeleteNote { note: id },
            ))
        }

        Command::EditNote { note, body } => {
            live_note(model, *note)?;
            if let Some(note) = model.notes.get_mut(note) {
                note.body = body.clone();
                note.updated_at = now.clone();
            }
            // A note body is typed, not decided: it pushes nothing.
            Ok(None)
        }

        Command::DeleteNote { note } => {
            live_note(model, *note)?;
            if let Some(note) = model.notes.get_mut(note) {
                note.deleted_at = Some(now.clone());
            }
            Ok(Entry::new(
                "Deleted a note".to_owned(),
                Command::RestoreNote { note: *note },
            ))
        }

        Command::MoveBack {
            task,
            place,
            position,
            waiting,
            drop_placement,
        } => {
            let id = live(model, *task)?.id;
            put_in_place(model, id, *place, *position);
            set_task(model, id, |task| {
                task.waiting = *waiting && place.day().is_none();
                task.closed_at = None;
            });
            if let Some(day) = drop_placement {
                model.placements.remove(&(id, *day));
            }
            Ok(None)
        }

        Command::CloseAt {
            task,
            closed_at,
            position,
        } => {
            let current = live(model, *task)?;
            if !current.is_open() {
                return Err(Rejected("That task is already closed.".to_owned()));
            }
            let id = current.id;
            set_task(model, id, |task| task.closed_at = Some(closed_at.clone()));
            set_position(model, id, *position);
            Ok(None)
        }

        Command::RestoreTask { task, position } => {
            let Some(current) = model.task(*task) else {
                return Err(gone());
            };
            if current.is_live() {
                return Err(Rejected("That task is already back.".to_owned()));
            }
            let id = current.id;
            set_task(model, id, |task| task.deleted_at = None);
            set_position(model, id, *position);
            Ok(None)
        }

        Command::UncreateSchedule { task, schedule } => {
            let current = live(model, *task)?;
            if current.schedule_id != Some(*schedule) {
                return Err(Rejected("That task no longer repeats.".to_owned()));
            }
            let id = current.id;
            set_task(model, id, |task| {
                task.schedule_id = None;
                task.scheduled_on = None;
            });
            // The row goes only when nothing points at it any more, which
            // at the moment this undoes a CreateSchedule is the case.
            let orphan = model
                .tasks
                .values()
                .all(|task| task.schedule_id != Some(*schedule));
            if orphan {
                model.schedules.remove(schedule);
            }
            Ok(None)
        }

        Command::ResumeSchedule { schedule } => {
            if !model.schedules.contains_key(schedule) {
                return Err(Rejected("That repeat schedule is gone.".to_owned()));
            }
            if let Some(schedule) = model.schedules.get_mut(schedule) {
                schedule.stopped_on = None;
            }
            Ok(None)
        }

        Command::RestoreNote { note } => {
            let Some(current) = model.note(*note) else {
                return Err(Rejected("That note is gone.".to_owned()));
            };
            if current.is_live() {
                return Err(Rejected("That note is already back.".to_owned()));
            }
            if let Some(note) = model.notes.get_mut(note) {
                note.deleted_at = None;
            }
            Ok(None)
        }
    }
}

// ---- preconditions ---------------------------------------------------

fn gone() -> Rejected {
    Rejected("That task is gone.".to_owned())
}

fn live(model: &Model, task: Id) -> Result<&Task, Rejected> {
    model.live_task(task).ok_or_else(gone)
}

fn live_note(model: &Model, note: Id) -> Result<&Note, Rejected> {
    model
        .note(note)
        .filter(|note| note.is_live())
        .ok_or_else(|| Rejected("That note is gone.".to_owned()))
}

fn unstopped(model: &Model, schedule: Id) -> Result<&Schedule, Rejected> {
    let schedule = model
        .schedule(schedule)
        .ok_or_else(|| Rejected("That repeat schedule is gone.".to_owned()))?;
    if schedule.is_stopped() {
        return Err(Rejected("That repeat has stopped.".to_owned()));
    }
    Ok(schedule)
}

/// A title is the whole task: non-empty after trimming, one line.
fn valid_title(text: &str) -> Result<String, Rejected> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(Rejected("A task needs a title.".to_owned()));
    }
    if trimmed.contains('\n') {
        return Err(Rejected("A title is one line.".to_owned()));
    }
    Ok(trimmed.to_owned())
}

// ---- places and order ------------------------------------------------

/// The last position of a place, which is where anything arriving lands.
pub(super) fn end_of(model: &Model, place: Place) -> usize {
    model.place(place).len()
}

/// Renumbers a place to the dense integers `0..n` in its current order.
pub(super) fn renumber(model: &mut Model, place: Place) {
    let order: Vec<Id> = model.place(place).iter().map(|task| task.id).collect();
    for (position, id) in order.into_iter().enumerate() {
        if let Some(task) = model.tasks.get_mut(&id) {
            task.position = position;
        }
    }
}

/// Moves a task within its place, renumbering the place around it. A
/// position past the end is clamped to it, which is what undo needs when
/// another window has taken a row away (DOMAIN.md section 11).
fn set_position(model: &mut Model, id: Id, position: usize) {
    let Some(place) = model.task(id).map(Task::place) else {
        return;
    };
    let mut order: Vec<Id> = model.place(place).iter().map(|task| task.id).collect();
    let Some(from) = order.iter().position(|other| *other == id) else {
        return;
    };
    order.remove(from);
    order.insert(position.min(order.len()), id);
    for (position, id) in order.into_iter().enumerate() {
        if let Some(task) = model.tasks.get_mut(&id) {
            task.position = position;
        }
    }
}

/// Puts a task in a place at a position, leaving the place it came from
/// dense.
pub(super) fn put_in_place(model: &mut Model, id: Id, place: Place, position: usize) {
    let Some(was) = model.task(id).map(Task::place) else {
        return;
    };
    if was != place {
        if let Some(task) = model.tasks.get_mut(&id) {
            task.day = place.day();
            // Last of the new place until set_position says otherwise.
            task.position = usize::MAX;
        }
        renumber(model, was);
    }
    set_position(model, id, position);
}

/// Writes the placement for a day unless the task has been there before,
/// in which case the first arrival's row stands. Says whether it wrote.
pub(super) fn place_on_day(
    model: &mut Model,
    task: Id,
    day: Date,
    now: &Zoned,
    from_place: FromPlace,
) -> bool {
    if model.placements.contains_key(&(task, day)) {
        return false;
    }
    model.placements.insert(
        (task, day),
        Placement {
            task_id: task,
            day,
            placed_at: now.clone(),
            from_place,
        },
    );
    true
}

fn set_task(model: &mut Model, id: Id, edit: impl FnOnce(&mut Task)) {
    if let Some(task) = model.tasks.get_mut(&id) {
        edit(task);
    }
}

/// The largest id in a table plus one.
pub(super) fn next_id(ids: impl Iterator<Item = Id>) -> Id {
    ids.max().unwrap_or(0) + 1
}

// ---- labels ----------------------------------------------------------

fn named(title: &str) -> String {
    format!("\"{title}\"")
}

fn place_name(place: Place, today: Date) -> String {
    match place {
        Place::Backlog => "the backlog".to_owned(),
        Place::Day(day) if day == today => "today".to_owned(),
        Place::Day(day) => day.strftime("%a %-d %b").to_string(),
    }
}
