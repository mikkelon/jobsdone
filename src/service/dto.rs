//! What a response is made of.
//!
//! Two shapes stand for a task. A **task** is the row: what it is, where
//! it is, what dates it carries. A **row** is what a view of the model
//! hands a screen, carrying the things a screen works out from more than
//! one row — whether a due date has passed, whether the task is on the
//! pile, whether it arrived from the backlog on the day being drawn. A
//! mutation answers with the task it changed; a view answers with rows,
//! so that a caller drawing a day sees what the window draws.
//!
//! Ids are the model's own and stable. Dates are ISO, instants are the
//! RFC 3339 the rows are stored as. Nothing here is formatted for a
//! person: a label is the caller's to write.

use jiff::Zoned;
use serde_json::{Value, json};

use super::order;
use crate::domain::{
    BacklogView, DayList, DayView, Id, Model, Note, NotesView, Pile, Place, Row, Rule, Schedule,
    SearchResults, Stretch, Surfaced, Task, UndoEntry,
};

pub(super) fn place(place: Place) -> Value {
    match place {
        Place::Backlog => json!({"kind": "backlog"}),
        Place::Day(day) => json!({"kind": "day", "day": day.to_string()}),
    }
}

pub(super) fn rule(rule: &Rule) -> Value {
    serde_json::to_value(rule).unwrap_or_default()
}

/// A task as it stands, with the position a caller reorders by.
pub(super) fn task(model: &Model, task: &Task) -> Value {
    json!({
        "id": task.id,
        "title": task.title,
        "place": place(task.place()),
        "position": order::position_of(model, task),
        "open": task.is_open(),
        "focus": task.focus,
        "waiting": task.waiting,
        "closed_at": task.closed_at.as_ref().map(Zoned::to_string),
        "due_on": task.due_on.map(|date| date.to_string()),
        "remind_on": task.remind_on.map(|date| date.to_string()),
        "created_at": task.created_at.to_string(),
        "schedule": task.schedule_id.and_then(|id| model.schedule(id)).map(|schedule| json!({
            "id": schedule.id,
            "scheduled_on": task.scheduled_on.map(|date| date.to_string()),
            "rule": rule(&schedule.rule),
        })),
    })
}

/// The tasks these ids name, in the order they were named. An id whose
/// task has gone is left out rather than reported as null.
pub(super) fn tasks(model: &Model, ids: &[Id]) -> Value {
    Value::Array(
        ids.iter()
            .filter_map(|id| model.task(*id))
            .map(|found| task(model, found))
            .collect(),
    )
}

/// A row of a view, with everything the view worked out about it.
pub(super) fn row(row: &Row) -> Value {
    json!({
        "id": row.task,
        "title": row.title,
        "place": place(row.place),
        "closed_at": row.closed_at.as_ref().map(Zoned::to_string),
        "focus": row.focus,
        "waiting": row.waiting,
        "due": row.due.map(|due| json!({"on": due.on.to_string(), "overdue": due.overdue})),
        "remind": row.remind.map(|date| date.to_string()),
        "repeat": row.repeat.as_ref().map(rule),
        "was_focus": row.was_focus,
        "on_the_pile": row.on_the_pile,
        "still_open": row.still_open,
        "from_backlog": row.from_backlog,
        "closed_on_this_day": row.closed_on_this_day,
    })
}

pub(super) fn rows(list: &[Row]) -> Value {
    Value::Array(list.iter().map(row).collect())
}

pub(super) fn day(view: &DayView) -> Value {
    json!({
        "day": view.day.to_string(),
        "focus": rows(&view.focus),
        "plan": rows(&view.plan),
        "done": rows(&view.done),
        "moved": rows(&view.moved),
        "counts": {
            "planned": view.counts.planned,
            "open": view.counts.open,
            "done": view.counts.done,
            "moved": view.counts.moved,
        },
    })
}

pub(super) fn backlog(view: &BacklogView) -> Value {
    json!({
        "ordinary": rows(&view.ordinary),
        "waiting": rows(&view.waiting),
        "schedules": Value::Array(view.schedules.iter().map(|row| json!({
            "id": row.schedule,
            "title": row.title,
            "rule": rule(&row.rule),
        })).collect()),
        "open": view.open,
        "waiting_count": view.waiting_count,
    })
}

pub(super) fn day_list(list: &DayList, limit: Option<usize>) -> Value {
    let mut left = limit.unwrap_or(usize::MAX);
    let mut stretches = Vec::new();
    let mut total = 0;

    for stretch in &list.stretches {
        if left == 0 {
            break;
        }
        let days: Vec<Value> = stretch
            .days
            .iter()
            .take(left)
            .map(|day| {
                json!({
                    "day": day.day.to_string(),
                    "kept": day.kept,
                    "done": day.done,
                    "open": day.open,
                })
            })
            .collect();
        left -= days.len();
        total += days.len();
        stretches.push(json!({"stretch": stretch_name(stretch.stretch), "days": days}));
    }
    json!({"stretches": stretches, "total_days": total})
}

fn stretch_name(stretch: Stretch) -> &'static str {
    match stretch {
        Stretch::Later => "later",
        Stretch::ThisWeek => "this_week",
        Stretch::LastWeek => "last_week",
        Stretch::Earlier => "earlier",
    }
}

pub(super) fn search(results: &SearchResults) -> Value {
    json!({
        "open": rows(&results.open),
        "closed": rows(&results.closed),
        "total": results.total,
    })
}

pub(super) fn pile(pile: &Pile) -> Value {
    json!({
        "total": pile.total,
        "days": Value::Array(pile.days.iter().map(|day| json!({
            "day": day.day.to_string(),
            "age": day.age,
            "rows": rows(&day.rows),
        })).collect()),
    })
}

pub(super) fn surfaced(surfaced: &Surfaced) -> Value {
    json!({
        "total": surfaced.total,
        "due": rows(&surfaced.due),
        "reminders": rows(&surfaced.reminders),
        "also_starting_today": rows(&surfaced.also_starting_today),
    })
}

/// A schedule, with how many live copies still point at it.
pub(super) fn schedule(model: &Model, schedule: &Schedule) -> Value {
    let copies = model
        .tasks
        .values()
        .filter(|task| task.is_live() && task.schedule_id == Some(schedule.id))
        .count();
    json!({
        "id": schedule.id,
        "title": schedule.title,
        "rule": rule(&schedule.rule),
        "generated_through": schedule.generated_through.to_string(),
        "stopped_on": schedule.stopped_on.map(|date| date.to_string()),
        "stopped": schedule.is_stopped(),
        "created_at": schedule.created_at.to_string(),
        "copies": copies,
    })
}

pub(super) fn note(note: &Note) -> Value {
    json!({
        "id": note.id,
        "body": note.body,
        "created_at": note.created_at.to_string(),
        "updated_at": note.updated_at.to_string(),
        "archived_at": note.archived_at.as_ref().map(ToString::to_string),
        "spell_check": note.spell_check,
    })
}

/// The stack or the archive, in the order the notes page draws it,
/// and how many notes that list holds.
pub(super) fn notes(model: &Model, view: &NotesView, include_body: bool) -> Value {
    let rows: Vec<Value> = view
        .rows
        .iter()
        .map(|row| {
            let mut entry = json!({
                "id": row.note,
                "first_line": row.first_line,
                "created_at": row.created_at.to_string(),
                "updated_at": model.note(row.note).map(|note| note.updated_at.to_string()),
                "archived_at": row.archived_at.as_ref().map(ToString::to_string),
            });
            if include_body
                && let Some(found) = model.note(row.note)
                && let Some(entry) = entry.as_object_mut()
            {
                entry.insert("body".to_owned(), Value::String(found.body.clone()));
            }
            entry
        })
        .collect();
    json!({"notes": rows, "count": rows.len()})
}

pub(super) fn undo_entry(entry: &UndoEntry) -> Value {
    json!({
        "id": entry.id,
        "label": entry.label,
        "at": entry.at.to_string(),
    })
}
