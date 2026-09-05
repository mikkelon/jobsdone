//! The two operations the program does to itself rather than at
//! somebody's request. Neither touches the undo stack.

use jiff::Zoned;
use jiff::civil::Date;

use super::command::{end_of, next_id, place_on_day};
use super::model::{
    Change, FromPlace, Id, Model, Place, REVIEW_BEFORE, REVIEW_ON, Task, Write, diff,
};
use super::working_day;

/// Creates the copies for every scheduled date since the last launch.
///
/// Recurring schedules are the one thing that creates tasks by itself,
/// and there is no cap: three weeks away means fifteen standup copies,
/// each on its own past day, all on the pile. That is where the cost of
/// being away is meant to be seen (DOMAIN.md section 10).
pub fn generate_copies(model: &Model, now: &Zoned) -> Change {
    let today = working_day(now);
    let mut after = model.clone();

    let schedules: Vec<Id> = model
        .schedules
        .values()
        .filter(|schedule| !schedule.is_stopped())
        .map(|schedule| schedule.id)
        .collect();

    for id in schedules {
        let Some(schedule) = after.schedules.get(&id) else {
            continue;
        };
        let (title, rule, through) = (
            schedule.title.clone(),
            schedule.rule.clone(),
            schedule.generated_through,
        );
        if today <= through {
            continue;
        }

        let mut day = through;
        while let Ok(next) = day.tomorrow() {
            if next > today {
                break;
            }
            day = next;
            if rule.falls_on(day) {
                copy(&mut after, id, &title, day, now);
            }
        }
        if let Some(schedule) = after.schedules.get_mut(&id) {
            schedule.generated_through = today;
        }
    }

    Change {
        writes: diff(model, &after),
    }
}

/// One copy of a schedule for one date, unless the copy is already
/// there. A deleted copy keeps its row, so it is never made again.
fn copy(model: &mut Model, schedule: Id, title: &str, day: Date, now: &Zoned) {
    let exists = model
        .tasks
        .values()
        .any(|task| task.schedule_id == Some(schedule) && task.scheduled_on == Some(day));
    if exists {
        return;
    }

    let id = next_id(model.tasks.keys().copied());
    let position = end_of(model, Place::Day(day));
    model.tasks.insert(
        id,
        Task {
            id,
            title: title.to_owned(),
            day: Some(day),
            position,
            focus: false,
            waiting: false,
            closed_at: None,
            due_on: None,
            remind_on: None,
            schedule_id: Some(schedule),
            scheduled_on: Some(day),
            created_at: now.clone(),
            deleted_at: None,
        },
    );
    place_on_day(model, id, day, now, FromPlace::New);
}

/// Opens the once-a-day gate. Running it again on the same day changes
/// nothing, which is what makes a second window skip a review the first
/// has started (DOMAIN.md section 13).
pub fn start_review(model: &Model, today: Date) -> Option<Change> {
    if model.meta_date(REVIEW_ON) == Some(today) {
        return None;
    }

    let mut writes = Vec::new();
    if let Some(before) = model.meta.get(REVIEW_ON) {
        writes.push(Write::SetMeta {
            key: REVIEW_BEFORE.to_owned(),
            value: before.clone(),
        });
    }
    writes.push(Write::SetMeta {
        key: REVIEW_ON.to_owned(),
        value: today.to_string(),
    });
    Some(Change { writes })
}
