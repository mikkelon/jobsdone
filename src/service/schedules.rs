//! Repeat schedules: the list of them, the dates a rule falls on, and
//! the three changes that can be made to one.
//!
//! A schedule is never deleted, so there is no operation to delete one.
//! Stopping it is how a repeat ends, and its old copies keep the mark
//! that says where they came from (DOMAIN.md section 10).

use serde::Deserialize;
use serde_json::{Value, json};

use super::reads::missing_task;
use super::{Error, Session, dto, spec};
use crate::domain::{self, Command, Id};

/// How many dates a preview shows when it is not asked for a number.
const PREVIEW: usize = 3;

/// The most a preview will walk out to. Past this it is a listing rather
/// than a look at what a rule means.
const PREVIEW_MOST: usize = 50;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    #[serde(default)]
    include_stopped: bool,
}

pub(super) fn list(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: List = spec::fields("schedule.list", fields)?;
    let model = session.model();
    let schedules: Vec<Value> = model
        .schedules
        .values()
        .filter(|schedule| request.include_stopped || !schedule.is_stopped())
        .map(|schedule| dto::schedule(model, schedule))
        .collect();
    Ok(json!({"total": schedules.len(), "schedules": schedules}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct One {
    id: Id,
}

pub(super) fn get(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: One = spec::fields("schedule.get", fields)?;
    let model = session.model();
    let schedule = model
        .schedule(request.id)
        .ok_or_else(|| missing(request.id))?;
    let next = domain::next_dates(
        &schedule.rule,
        schedule.generated_through,
        PREVIEW,
        &model.settings.work_days(),
    );
    Ok(json!({
        "schedule": dto::schedule(model, schedule),
        "next": dates(&next),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    rule: Option<spec::RuleValue>,
    schedule: Option<Id>,
    after: Option<spec::DateText>,
    count: Option<usize>,
}

/// The dates a rule falls on, whether or not the rule has been saved to
/// a schedule: the repeat card previews what has been typed, and so does
/// this.
pub(super) fn preview(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: Preview = spec::fields("schedule.preview", fields)?;
    let (today, work_days) = (session.today(), session.model().settings.work_days());
    let model = session.model();

    let (rule, from) = match (&request.rule, request.schedule) {
        (Some(rule), None) => (rule.rule(today, work_days)?, today),
        (None, Some(id)) => {
            let schedule = model.schedule(id).ok_or_else(|| missing(id))?;
            (schedule.rule.clone(), schedule.generated_through)
        }
        _ => {
            return Err(Error::invalid_argument(
                "A preview is of one `rule` or of one saved `schedule`.",
            ));
        }
    };

    let after = match &request.after {
        Some(text) => text.date(today, work_days)?,
        None => from,
    };
    let count = match request.count {
        None => PREVIEW,
        Some(count @ 1..=PREVIEW_MOST) => count,
        Some(_) => {
            return Err(Error::invalid_argument(format!(
                "A preview shows 1 to {PREVIEW_MOST} dates."
            )));
        }
    };

    Ok(json!({
        "rule": dto::rule(&rule),
        "after": after.to_string(),
        "dates": dates(&domain::next_dates(&rule, after, count, &work_days)),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    task: Id,
    rule: spec::RuleValue,
}

/// A schedule made from a task, which becomes its first copy.
pub(super) fn create(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Create = spec::fields("schedule.create", fields)?;
    let (today, work_days) = (session.today(), session.model().settings.work_days());
    let rule = request.rule.rule(today, work_days)?;
    if session.model().live_task(request.task).is_none() {
        return Err(missing_task(request.task));
    }

    let undo = session.commit(vec![Command::CreateSchedule {
        task: request.task,
        rule,
    }])?;

    let model = session.model();
    let task = model
        .task(request.task)
        .ok_or_else(|| missing_task(request.task))?;
    let schedule = task
        .schedule_id
        .and_then(|id| model.schedule(id))
        .ok_or_else(|| Error::invalid_argument("That task does not repeat."))?;
    Ok(json!({
        "schedule": dto::schedule(model, schedule),
        "task": dto::task(model, task),
        "undo": undo,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: Id,
    title: Option<String>,
    rule: Option<spec::RuleValue>,
}

/// The title future copies are made with, the rule they fall on, or both
/// as one change.
pub(super) fn update(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Update = spec::fields("schedule.update", fields)?;
    let (today, work_days) = (session.today(), session.model().settings.work_days());
    if session.model().schedule(request.id).is_none() {
        return Err(missing(request.id));
    }

    let mut commands = Vec::new();
    if let Some(title) = request.title {
        commands.push(Command::EditScheduleTitle {
            schedule: request.id,
            title,
        });
    }
    if let Some(rule) = &request.rule {
        commands.push(Command::SetRule {
            schedule: request.id,
            rule: rule.rule(today, work_days)?,
        });
    }
    if commands.is_empty() {
        return Err(Error::invalid_argument(
            "An update needs a `title`, a `rule`, or both.",
        ));
    }

    let undo = session.commit(commands)?;
    answer(session, request.id, undo)
}

pub(super) fn stop(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: One = spec::fields("schedule.stop", fields)?;
    if session.model().schedule(request.id).is_none() {
        return Err(missing(request.id));
    }
    let undo = session.commit(vec![Command::StopSchedule {
        schedule: request.id,
    }])?;
    answer(session, request.id, undo)
}

fn answer(session: &Session, id: Id, undo: Value) -> Result<Value, Error> {
    let model = session.model();
    let schedule = model.schedule(id).ok_or_else(|| missing(id))?;
    Ok(json!({"schedule": dto::schedule(model, schedule), "undo": undo}))
}

fn missing(id: Id) -> Error {
    Error::not_found(format!("There is no repeat schedule {id}."))
}

fn dates(dates: &[jiff::civil::Date]) -> Value {
    Value::Array(
        dates
            .iter()
            .map(|date| Value::String(date.to_string()))
            .collect(),
    )
}
