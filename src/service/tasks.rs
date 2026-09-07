//! What a request does to tasks.
//!
//! Each of these is one completed intention: everything it asks for
//! becomes one list of commands, the list is checked whole before
//! anything is written, and what reaches storage is one change with one
//! undo entry behind it. A request that names five tasks either moves
//! five or moves none.
//!
//! The order the commands are built in is the order they have to happen
//! in. A move clears waiting and sends the task to the end of its new
//! place, so waiting and focus are set after it and a position is worked
//! out last, against the place the task has by then arrived in.

use serde::Deserialize;
use serde_json::{Value, json};

use super::reads::missing_task;
use super::{Error, Session, dto, order, spec};
use crate::domain::{Command, Id, Model, Place};

// ---- adding ----------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Add {
    title: String,
    place: spec::PlaceValue,
    focus: Option<bool>,
    waiting: Option<bool>,
    due: Option<spec::DateText>,
    remind: Option<spec::DateText>,
    repeat: Option<spec::RuleValue>,
    position: Option<usize>,
    before: Option<Id>,
    after: Option<Id>,
}

/// A task and everything it is being given at once.
pub(super) fn add(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Add = spec::fields("task.add", fields)?;
    let (today, work_days) = (session.today(), session.model().settings.work_days());
    let place = request.place.place(today, work_days)?;

    if request.waiting == Some(true) && place != Place::Backlog {
        return Err(Error::invalid_argument(
            "A waiting task belongs in the backlog; ask for the backlog place or leave waiting \
             off.",
        ));
    }

    let mut commands = vec![Command::AddTask {
        title: request.title,
        place,
    }];

    // The id is the domain's to choose, so it is read off the model the
    // add would leave rather than guessed at.
    let added = session.foresee(&commands)?;
    let id = new_id(session.model(), &added)?;

    if let Some(date) = &request.due {
        commands.push(Command::SetDue {
            task: id,
            date: Some(date.date(today, work_days)?),
        });
    }
    if let Some(date) = &request.remind {
        commands.push(Command::SetRemind {
            task: id,
            date: Some(date.date(today, work_days)?),
        });
    }
    commands.extend(flags(&added, id, request.focus, request.waiting));
    if let Some(rule) = &request.repeat {
        commands.push(Command::CreateSchedule {
            task: id,
            rule: rule.rule(today, work_days)?,
        });
    }

    let landing = Landing::of(request.position, request.before, request.after)?;
    if let Some(landing) = landing {
        let settled = session.foresee(&commands)?;
        commands.extend(landing.commands(&settled, id)?);
    }

    let undo = session.commit(commands)?;
    let model = session.model();
    let task = model.task(id).ok_or_else(|| missing_task(id))?;
    Ok(json!({
        "task": dto::task(model, task),
        "schedule": task.schedule_id.and_then(|id| model.schedule(id)).map(|schedule| dto::schedule(model, schedule)),
        "undo": undo,
    }))
}

/// The commands the two flags earn, against the task as `model` has it.
///
/// A flag already saying what it is asked to say earns none. Saying so is
/// not a change, and a `SetFocus` of false on a backlog task would
/// otherwise be refused for a rule about focus the request was not
/// breaking: it named the state it wanted and the task is in it.
fn flags(model: &Model, id: Id, focus: Option<bool>, waiting: Option<bool>) -> Vec<Command> {
    let Some(task) = model.live_task(id) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    if let Some(waiting) = waiting.filter(|wanted| *wanted != task.waiting) {
        commands.push(Command::SetWaiting { task: id, waiting });
    }
    if let Some(focus) = focus.filter(|wanted| *wanted != task.focus) {
        commands.push(Command::SetFocus { task: id, focus });
    }
    commands
}

/// The task an add made: the one the model did not have before.
fn new_id(before: &Model, after: &Model) -> Result<Id, Error> {
    after
        .tasks
        .keys()
        .find(|id| !before.tasks.contains_key(id))
        .copied()
        .ok_or_else(|| Error::invalid_argument("That task was not added."))
}

// ---- updating --------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: Id,
    title: Option<String>,
    title_scope: Option<Scope>,
    place: Option<spec::PlaceValue>,
    focus: Option<bool>,
    waiting: Option<bool>,
    #[serde(default, deserialize_with = "spec::clearable")]
    due: Option<Option<spec::DateText>>,
    #[serde(default, deserialize_with = "spec::clearable")]
    remind: Option<Option<spec::DateText>>,
    position: Option<usize>,
    before: Option<Id>,
    after: Option<Id>,
}

/// Which title a rename is of: the copy on the day, or the copy and the
/// schedule every later one is made from.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Scope {
    This,
    Future,
}

pub(super) fn update(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Update = spec::fields("task.update", fields)?;
    let (today, work_days) = (session.today(), session.model().settings.work_days());
    let id = request.id;

    let current = session
        .model()
        .live_task(id)
        .ok_or_else(|| missing_task(id))?;
    let repeats = current.schedule_id.is_some();

    let place = request
        .place
        .as_ref()
        .map(|place| place.place(today, work_days))
        .transpose()?;
    if request.waiting == Some(true) && matches!(place, Some(Place::Day(_))) {
        return Err(Error::invalid_argument(
            "A waiting task belongs in the backlog, so it cannot be put on a day in the same \
             breath.",
        ));
    }

    let mut commands = Vec::new();

    if let Some(title) = request.title {
        commands.push(rename(id, title, request.title_scope, repeats)?);
    } else if request.title_scope.is_some() {
        return Err(Error::invalid_argument(
            "`title_scope` says which titles a rename is of; there is no title here to rename.",
        ));
    }

    if let Some(date) = &request.due {
        commands.push(Command::SetDue {
            task: id,
            date: date
                .as_ref()
                .map(|d| d.date(today, work_days))
                .transpose()?,
        });
    }
    if let Some(date) = &request.remind {
        commands.push(Command::SetRemind {
            task: id,
            date: date
                .as_ref()
                .map(|d| d.date(today, work_days))
                .transpose()?,
        });
    }
    if let Some(place) = place {
        commands.push(Command::Move { task: id, place });
    }

    let landing = Landing::of(request.position, request.before, request.after)?;
    let asked = !commands.is_empty()
        || landing.is_some()
        || request.focus.is_some()
        || request.waiting.is_some();
    if !asked {
        return Err(Error::invalid_argument(
            "An update needs something to change: a title, a place, focus, waiting, a due or \
             remind date, or a position.",
        ));
    }

    // The flags are read against the move rather than against the place
    // the task was in, because a move clears waiting on its way.
    if request.focus.is_some() || request.waiting.is_some() {
        let moved = session.foresee(&commands)?;
        commands.extend(flags(&moved, id, request.focus, request.waiting));
    }
    if let Some(landing) = landing {
        let settled = session.foresee(&commands)?;
        commands.extend(landing.commands(&settled, id)?);
    }

    let undo = session.commit(commands)?;
    let model = session.model();
    let task = model.task(id).ok_or_else(|| missing_task(id))?;
    Ok(json!({"task": dto::task(model, task), "undo": undo}))
}

/// A rename, of the copy alone or of the schedule with it. Which of the
/// two is never guessed: a copy renamed without saying would rename
/// either everything after it or nothing, and both are somebody's
/// intention rather than a default.
fn rename(id: Id, title: String, scope: Option<Scope>, repeats: bool) -> Result<Command, Error> {
    match (scope, repeats) {
        (None, true) => Err(Error::invalid_argument(
            "That task is a recurring copy, so a rename needs `title_scope`: \"this\" for the \
             copy alone, \"future\" for the copy and every later one.",
        )),
        (Some(Scope::Future), false) => Err(Error::invalid_argument(
            "That task is not a recurring copy, so there are no future copies to rename.",
        )),
        (Some(Scope::Future), true) => Ok(Command::EditTitleAndFuture {
            task: id,
            schedule_title: title.clone(),
            title,
        }),
        (None | Some(Scope::This), _) => Ok(Command::EditTitle { task: id, title }),
    }
}

// ---- where a task lands ----------------------------------------------

/// Where in its place a task is being put: at a position, or beside
/// another task.
enum Landing {
    At(usize),
    Before(Id),
    After(Id),
}

impl Landing {
    fn of(
        position: Option<usize>,
        before: Option<Id>,
        after: Option<Id>,
    ) -> Result<Option<Landing>, Error> {
        match (position, before, after) {
            (None, None, None) => Ok(None),
            (Some(position), None, None) => {
                if position < 1 {
                    return Err(Error::invalid_argument("Positions count from 1."));
                }
                Ok(Some(Landing::At(position)))
            }
            (None, Some(id), None) => Ok(Some(Landing::Before(id))),
            (None, None, Some(id)) => Ok(Some(Landing::After(id))),
            _ => Err(Error::invalid_argument(
                "Say where a task goes once: `position`, `before` or `after`.",
            )),
        }
    }

    /// The commands that put `id` where this says, against the model the
    /// rest of the operation has already left.
    fn commands(&self, model: &Model, id: Id) -> Result<Vec<Command>, Error> {
        let Some(task) = model.live_task(id) else {
            return Err(missing_task(id));
        };
        if !task.is_open() {
            return Err(Error::invalid_argument(
                "A closed task keeps the place it was done in; there is no position to give it.",
            ));
        }
        let place = task.place();
        let mut order = order::open_order(model, place);
        let Some(from) = order.iter().position(|other| *other == id) else {
            return Err(missing_task(id));
        };
        order.remove(from);

        let at = match self {
            Landing::At(position) => {
                if *position > order.len() + 1 {
                    return Err(Error::invalid_argument(format!(
                        "There are {} open tasks in that place, so the last position is {}.",
                        order.len() + 1,
                        order.len() + 1
                    )));
                }
                position - 1
            }
            Landing::Before(other) | Landing::After(other) => {
                if *other == id {
                    return Err(Error::invalid_argument(
                        "A task cannot be put beside itself.",
                    ));
                }
                let at = order
                    .iter()
                    .position(|found| found == other)
                    .ok_or_else(|| beside(model, *other, place))?;
                match self {
                    Landing::Before(_) => at,
                    _ => at + 1,
                }
            }
        };
        order.insert(at, id);
        Ok(order::reorder(model, place, &order))
    }
}

/// Why a task cannot be the one another is put beside.
fn beside(model: &Model, other: Id, place: Place) -> Error {
    match model.live_task(other) {
        None => missing_task(other),
        Some(task) if !task.is_open() => Error::invalid_argument(format!(
            "Task {other} is closed, so nothing is ordered against it."
        )),
        Some(task) if task.place() != place => Error::invalid_argument(format!(
            "Task {other} is not in the place the task is being put in."
        )),
        Some(_) => missing_task(other),
    }
}

// ---- reordering ------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reorder {
    id: Id,
    position: Option<usize>,
    before: Option<Id>,
    after: Option<Id>,
}

pub(super) fn reorder(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Reorder = spec::fields("task.reorder", fields)?;
    let landing =
        Landing::of(request.position, request.before, request.after)?.ok_or_else(|| {
            Error::invalid_argument(
                "A reorder needs `position`, `before` or `after` to say where the task goes.",
            )
        })?;

    let place = live_place(session, request.id)?;
    let commands = landing.commands(session.model(), request.id)?;
    let undo = session.commit(commands)?;
    Ok(ordered(session, place, undo))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReorderAll {
    place: spec::PlaceValue,
    ids: Vec<Id>,
}

/// The whole open order of a place, given as the exact set of its ids.
///
/// Nothing is inferred from a partial list: a list missing a task would
/// have to put it somewhere, and wherever it put it would be a guess
/// about a task the caller did not mention.
pub(super) fn reorder_all(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: ReorderAll = spec::fields("day.reorder", fields)?;
    let (today, work_days) = (session.today(), session.model().settings.work_days());
    let place = request.place.place(today, work_days)?;
    let ids = spec::distinct(&request.ids, "task")?;

    let open = order::open_order(session.model(), place);
    check_membership(session.model(), &ids, &open, place)?;

    let commands = order::reorder(session.model(), place, &ids);
    let undo = session.commit(commands)?;
    Ok(ordered(session, place, undo))
}

/// Whether the ids given are exactly the open tasks of the place.
fn check_membership(model: &Model, ids: &[Id], open: &[Id], place: Place) -> Result<(), Error> {
    for id in ids {
        if open.contains(id) {
            continue;
        }
        return Err(match model.live_task(*id) {
            None => missing_task(*id),
            Some(task) if !task.is_open() => Error::invalid_argument(format!(
                "Task {id} is closed, and a full order is of the open tasks."
            )),
            Some(_) => {
                Error::invalid_argument(format!("Task {id} is not in {}.", place_name(place)))
            }
        });
    }
    if let Some(missing) = open.iter().find(|id| !ids.contains(id)) {
        return Err(Error::invalid_argument(format!(
            "A full order names every open task of {}, and task {missing} is not in it.",
            place_name(place)
        )));
    }
    Ok(())
}

fn place_name(place: Place) -> String {
    match place {
        Place::Backlog => "the backlog".to_owned(),
        Place::Day(day) => day.to_string(),
    }
}

/// The open tasks of a place in the order they are now in.
fn ordered(session: &Session, place: Place, undo: Value) -> Value {
    let model = session.model();
    json!({
        "place": dto::place(place),
        "order": dto::tasks(model, &order::open_order(model, place)),
        "undo": undo,
    })
}

fn live_place(session: &Session, id: Id) -> Result<Place, Error> {
    session
        .model()
        .live_task(id)
        .map(|task| task.place())
        .ok_or_else(|| missing_task(id))
}

// ---- the operations that name several tasks --------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Several {
    ids: Vec<Id>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveMany {
    ids: Vec<Id>,
    place: spec::PlaceValue,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteMany {
    ids: Vec<Id>,
    #[serde(default)]
    confirm: bool,
}

pub(super) fn close(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Several = spec::fields("task.close", fields)?;
    let ids = live(session, spec::distinct(&request.ids, "task")?)?;
    many(session, &ids, |id| Command::Close { task: id })
}

pub(super) fn reopen(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Several = spec::fields("task.reopen", fields)?;
    let ids = live(session, spec::distinct(&request.ids, "task")?)?;
    many(session, &ids, |id| Command::Reopen { task: id })
}

pub(super) fn move_to(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: MoveMany = spec::fields("task.move", fields)?;
    let (today, work_days) = (session.today(), session.model().settings.work_days());
    let place = request.place.place(today, work_days)?;
    let ids = live(session, spec::distinct(&request.ids, "task")?)?;
    many(session, &ids, |id| Command::Move { task: id, place })
}

pub(super) fn delete(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: DeleteMany = spec::fields("task.delete", fields)?;
    let ids = live(session, spec::distinct(&request.ids, "task")?)?;
    if session.model().settings.confirm_delete() && !request.confirm {
        return Err(Error::confirmation_required(
            "Deleting asks first while `confirm_delete` is on. Say `confirm` to go ahead.",
        ));
    }
    many(session, &ids, |id| Command::DeleteTask { task: id })
}

/// One command per task, committed as one change.
fn many(
    session: &mut Session,
    ids: &[Id],
    command: impl Fn(Id) -> Command,
) -> Result<Value, Error> {
    let commands: Vec<Command> = ids.iter().map(|id| command(*id)).collect();
    let undo = session.commit(commands)?;
    let model = session.model();
    Ok(json!({
        "count": ids.len(),
        "tasks": dto::tasks(model, ids),
        "undo": undo,
    }))
}

/// The ids, once every one of them names a task that is still there. The
/// domain would say the same, but "there is no task 9" is an answer
/// about the request rather than about the rules.
fn live(session: &Session, ids: Vec<Id>) -> Result<Vec<Id>, Error> {
    for id in &ids {
        if session.model().live_task(*id).is_none() {
            return Err(missing_task(*id));
        }
    }
    Ok(ids)
}
