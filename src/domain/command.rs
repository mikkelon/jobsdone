//! Every change to the model is one command (DOMAIN.md section 12).
//!
//! A command is checked against the model, applied to a copy of it, and
//! the difference is the change storage commits. The commands at the end
//! of the enum are only ever produced as the inverse of another one; they
//! push nothing on the undo stack, because undo pushes nothing.

use jiff::Zoned;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use super::date::day_label;
use super::model::{
    Change, FromPlace, Id, Model, Note, Place, Placement, Schedule, Task, UndoEntry, Write, diff,
};
use super::rule::Rule;
use super::settings::DateOrder;
use super::{Context, Rejected};

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
    /// The title future copies are made with, changed on the schedule
    /// alone. `EditTitleAndFuture` renames a copy and its schedule
    /// together, which needs a copy to start from; this needs only the
    /// schedule, so a repeat whose copies have all been deleted can
    /// still be renamed.
    EditScheduleTitle {
        schedule: Id,
        title: String,
    },
    CreateNote,
    /// A note body as it is typed: the keystrokes of an open note, saved
    /// on the tick. Not undoable, because a body between keystrokes is
    /// not a decision anybody made (DOMAIN.md section 12).
    EditNote {
        note: Id,
        body: String,
    },
    /// A note body given whole, in one act, by somebody who wrote the
    /// replacement before asking for it. That is a decision, so unlike
    /// `EditNote` it can be taken back.
    ReplaceNote {
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
    /// The inverse of ReplaceNote: the body the note had, and the
    /// instant it was last changed before the replacement, so that a
    /// note taken back is the note that was there rather than the old
    /// words with a new date on them.
    RestoreNoteBody {
        note: Id,
        body: String,
        updated_at: Zoned,
    },
    /// The inverse of one operation made of several commands: the
    /// inverses of those commands in the order they undo, which is the
    /// reverse of the order they were applied in. It is serialised like
    /// any other inverse, so an entry written before compound operations
    /// existed names one of the variants above and reads back unchanged.
    Sequence(Vec<Command>),
}

impl Command {
    /// Whether this is one of the inverses the domain makes for itself.
    ///
    /// Nothing outside the program may ask for one: an inverse carries
    /// the state a command is being taken back to, which only the
    /// command that was applied can know, so accepting one as a request
    /// would be accepting a rewrite of history rather than a change.
    pub fn is_inverse(&self) -> bool {
        matches!(
            self,
            Command::MoveBack { .. }
                | Command::CloseAt { .. }
                | Command::RestoreTask { .. }
                | Command::UncreateSchedule { .. }
                | Command::ResumeSchedule { .. }
                | Command::RestoreNote { .. }
                | Command::RestoreNoteBody { .. }
                | Command::Sequence(_)
        )
    }
}

/// What `undo` did: the change to commit either way, the label of the
/// entry it popped, the task it was about, and, when the inverse no
/// longer applied, why it was dropped instead (DOMAIN.md section 11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undone {
    pub change: Change,
    pub label: String,
    /// The task the inverse brought back or changed, where it was about
    /// one, so that the cursor can go to it (DESIGN.md section 4).
    pub task: Option<Id>,
    pub dropped: Option<Rejected>,
}

/// Every user command: what it would change, or why it is refused.
///
/// The context carries the instant it was given, the length the undo
/// stack is held to and the order dates are written in; the domain
/// chooses none of the three (DOMAIN.md section 11).
pub fn apply(model: &Model, command: Command, ctx: &Context) -> Result<Change, Rejected> {
    apply_many(model, vec![command], ctx)
}

/// One operation made of several commands: what the whole of it would
/// change, or why it is refused.
///
/// The commands are applied in the order they are given, each to what
/// the one before it left, and nothing is written unless every one of
/// them holds: a command that is refused takes the whole operation with
/// it, so a half-applied operation never reaches storage. The change is
/// the difference between the model that went in and the model the last
/// command left, which is one transaction (DOMAIN.md section 17).
///
/// The operation earns at most one undo entry, whatever it is made of.
/// Its label is the label of the first command that earned one, said
/// with the number of changes that followed it, and its inverse is the
/// inverses of those commands in the order they undo. One command that
/// earns an entry pushes that entry unchanged, so an operation of one is
/// the command it is made of, down to the row written to `undo_log`.
pub fn apply_many(
    model: &Model,
    commands: Vec<Command>,
    ctx: &Context,
) -> Result<Change, Rejected> {
    let now = &ctx.now;
    let today = model.settings.working_day(now);
    let mut after = model.clone();

    let mut entries = Vec::new();
    for command in &commands {
        if command.is_inverse() {
            return Err(Rejected(
                "That is not a change anything may ask for.".to_owned(),
            ));
        }
        if let Some(entry) = run(&mut after, command, now, today, ctx.dates)? {
            entries.push(entry);
        }
    }

    let mut writes = diff(model, &after);
    if let Some(Entry { label, inverse }) = one_entry(entries) {
        writes.push(Write::PushUndo(UndoEntry {
            id: next_id(model.undo.iter().map(|entry| entry.id)),
            at: now.clone(),
            label,
            inverse,
        }));
        writes.push(Write::TruncateUndo(ctx.undo_cap));
    }
    Ok(Change { writes })
}

/// The one entry an operation earns, from the entries its commands
/// earned. Several become a `Sequence` of their inverses in the order
/// they undo, which is the reverse of the order they were applied in.
fn one_entry(mut entries: Vec<Entry>) -> Option<Entry> {
    match entries.len() {
        0 => None,
        1 => entries.pop(),
        rest => {
            let label = match rest {
                2 => format!("{} and one more change", entries[0].label),
                _ => format!("{} and {} more changes", entries[0].label, rest - 1),
            };
            let mut inverse: Vec<Command> =
                entries.into_iter().map(|entry| entry.inverse).collect();
            inverse.reverse();
            Some(Entry {
                label,
                inverse: Command::Sequence(inverse),
            })
        }
    }
}

/// Pops the top entry and returns its inverse's change, with nothing
/// pushed. An inverse whose precondition no longer holds, because another
/// window has moved on, drops the entry instead of applying it.
pub fn undo(model: &Model, ctx: &Context) -> Result<Undone, Rejected> {
    let Some(entry) = model.undo.last() else {
        return Err(Rejected("There is nothing to undo.".to_owned()));
    };
    let now = &ctx.now;
    let today = model.settings.working_day(now);
    let mut after = model.clone();

    match run(&mut after, &entry.inverse, now, today, ctx.dates) {
        Ok(_) => {
            let mut writes = diff(model, &after);
            writes.push(Write::PopUndo(entry.id));
            Ok(Undone {
                change: Change { writes },
                label: entry.label.clone(),
                task: task_of(&entry.inverse),
                dropped: None,
            })
        }
        // A dropped entry changed nothing, so there is no task to go to.
        Err(rejected) => Ok(Undone {
            change: Change {
                writes: vec![Write::PopUndo(entry.id)],
            },
            label: entry.label.clone(),
            task: None,
            dropped: Some(rejected),
        }),
    }
}

/// The task a command is about, where it is about one. A command that
/// names a schedule or a note names no task, and `AddTask` makes the id
/// it is about rather than carrying it.
fn task_of(command: &Command) -> Option<Id> {
    match command {
        Command::EditTitle { task, .. }
        | Command::EditTitleAndFuture { task, .. }
        | Command::Close { task }
        | Command::Reopen { task }
        | Command::SetFocus { task, .. }
        | Command::Move { task, .. }
        | Command::Reorder { task, .. }
        | Command::SetWaiting { task, .. }
        | Command::SetDue { task, .. }
        | Command::SetRemind { task, .. }
        | Command::DeleteTask { task }
        | Command::CreateSchedule { task, .. }
        | Command::MoveBack { task, .. }
        | Command::CloseAt { task, .. }
        | Command::RestoreTask { task, .. }
        | Command::UncreateSchedule { task, .. } => Some(*task),
        Command::AddTask { .. }
        | Command::SetRule { .. }
        | Command::StopSchedule { .. }
        | Command::EditScheduleTitle { .. }
        | Command::CreateNote
        | Command::EditNote { .. }
        | Command::ReplaceNote { .. }
        | Command::DeleteNote { .. }
        | Command::ResumeSchedule { .. }
        | Command::RestoreNote { .. }
        | Command::RestoreNoteBody { .. } => None,
        // The task the operation as a whole was about, which is the one
        // the first of its commands that was about a task named.
        Command::Sequence(commands) => commands.iter().find_map(task_of),
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
    dates: DateOrder,
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
                format!(
                    "Moved {} to {}",
                    named(&title),
                    place_name(*place, today, dates)
                ),
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

        Command::EditScheduleTitle { schedule, title } => {
            let title = valid_title(title)?;
            let Some(current) = model.schedules.get_mut(schedule) else {
                return Err(Rejected("That repeat schedule is gone.".to_owned()));
            };
            let was = current.title.clone();
            current.title = title.clone();
            Ok(Entry::new(
                format!("Renamed the repeat {}", named(&title)),
                Command::EditScheduleTitle {
                    schedule: *schedule,
                    title: was,
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

        Command::ReplaceNote { note, body } => {
            let current = live_note(model, *note)?;
            let (was, was_at) = (current.body.clone(), current.updated_at.clone());
            if let Some(note) = model.notes.get_mut(note) {
                note.body = body.clone();
                note.updated_at = now.clone();
            }
            Ok(Entry::new(
                "Replaced a note".to_owned(),
                Command::RestoreNoteBody {
                    note: *note,
                    body: was,
                    updated_at: was_at,
                },
            ))
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

        Command::RestoreNoteBody {
            note,
            body,
            updated_at,
        } => {
            live_note(model, *note)?;
            if let Some(note) = model.notes.get_mut(note) {
                note.body = body.clone();
                note.updated_at = updated_at.clone();
            }
            Ok(None)
        }

        // Every one of them, or none: the model is only written back
        // once they have all held, so an inverse that has been overtaken
        // half way through takes the whole entry with it rather than
        // leaving the operation half taken back (DOMAIN.md section 11).
        Command::Sequence(commands) => {
            let mut working = model.clone();
            for command in commands {
                run(&mut working, command, now, today, dates)?;
            }
            *model = working;
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

/// A title in quotes, cut to the few words a hint bar has room for beside
/// what happened to it.
fn named(title: &str) -> String {
    let mut chars = title.char_indices().skip(NAMED_MOST);
    match chars.next() {
        Some((at, _)) => format!("\"{}…\"", title[..at].trim_end()),
        None => format!("\"{title}\""),
    }
}

/// The most characters of a title a label quotes.
const NAMED_MOST: usize = 40;

fn place_name(place: Place, today: Date, dates: DateOrder) -> String {
    match place {
        Place::Backlog => "the backlog".to_owned(),
        Place::Day(day) if day == today => "today".to_owned(),
        Place::Day(day) => day_label(day, dates),
    }
}
