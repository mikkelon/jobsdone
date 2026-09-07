//! The operations that only look.
//!
//! Every one of them is a domain view, so what a caller reads is what
//! the window draws: the same grouping, the same order, the same
//! annotations. None of them generates a copy or spends the review.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Error, Session, dto, spec};
use crate::domain::{self, Id, Place};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskList {
    place: Option<spec::PlaceValue>,
    #[serde(default)]
    state: State,
    focus: Option<bool>,
    waiting: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum State {
    Open,
    Closed,
    #[default]
    All,
}

/// The live tasks, days before the backlog and each place in its order.
pub(super) fn task_list(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: TaskList = spec::fields("task.list", fields)?;
    let model = session.model();
    let wanted = request
        .place
        .as_ref()
        .map(|place| place.place(session.today(), model.settings.work_days()))
        .transpose()?;

    let mut places: Vec<Place> = model
        .tasks
        .values()
        .filter(|task| task.is_live())
        .map(|task| task.place())
        .collect();
    // Days in date order and the backlog after them, the way a person
    // reads a plan: what is dated first, then what is not.
    places.sort_by_key(|place| (place.day().is_none(), place.day()));
    places.dedup();

    let mut tasks = Vec::new();
    for place in places {
        if wanted.is_some_and(|only| only != place) {
            continue;
        }
        for task in model.place(place) {
            let keeps = match request.state {
                State::Open => task.is_open(),
                State::Closed => !task.is_open(),
                State::All => true,
            };
            if !keeps
                || request.focus.is_some_and(|focus| focus != task.focus)
                || request
                    .waiting
                    .is_some_and(|waiting| waiting != task.waiting)
            {
                continue;
            }
            tasks.push(dto::task(model, task));
        }
    }
    Ok(json!({"total": tasks.len(), "tasks": tasks}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct One {
    id: Id,
}

pub(super) fn task_get(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: One = spec::fields("task.get", fields)?;
    let model = session.model();
    let task = model
        .live_task(request.id)
        .ok_or_else(|| missing_task(request.id))?;
    Ok(json!({"task": dto::task(model, task)}))
}

pub(crate) fn missing_task(id: Id) -> Error {
    Error::not_found(format!("There is no task {id}."))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DayGet {
    day: Option<spec::DateText>,
}

/// A day, drawn as the day pane draws it. A day nothing was ever planned
/// for is the empty view rather than an error: a date is a date whether
/// or not anything happened on it.
pub(super) fn day_get(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: DayGet = spec::fields("day.get", fields)?;
    let today = session.today();
    let day = match request.day {
        Some(text) => text.date(today, session.model().settings.work_days())?,
        None => today,
    };
    let view = domain::day_view(session.model(), day, today);
    Ok(dto::day(&view))
}

pub(super) fn backlog_get(session: &Session, fields: Value) -> Result<Value, Error> {
    let _: spec::Nothing = spec::fields("backlog.get", fields)?;
    let view = domain::backlog_view(session.model(), session.today());
    Ok(dto::backlog(&view))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryList {
    limit: Option<usize>,
}

/// The days that have a placement, newest first, in the stretches the
/// history pane groups them under.
pub(super) fn history_list(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: HistoryList = spec::fields("history.list", fields)?;
    if request.limit == Some(0) {
        return Err(Error::invalid_argument("A limit is at least one day."));
    }
    let list = domain::day_list(session.model(), session.today());
    Ok(dto::day_list(&list, request.limit))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    text: String,
}

pub(super) fn search(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: Search = spec::fields("search", fields)?;
    let results = domain::search(session.model(), &request.text, session.today());
    Ok(dto::search(&results))
}
