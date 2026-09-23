//! One noninteractive request in, one JSON response out.
//!
//! The service is the window's twin: the same model, the same commands,
//! the same undo stack, reached by a JSON object instead of a key press.
//! Every request names an operation and carries that operation's fields,
//! and every response is the whole envelope, so that the caller renders
//! rather than assembles.
//!
//! Three rules hold the seam:
//!
//! **One invocation is one change.** However many properties a request
//! sets, however many tasks it names, the commands it becomes reach
//! storage as one `apply_many` and earn one undo entry. A request that is
//! refused anywhere writes nothing at all.
//!
//! **The program writes to itself only when asked.** Recurring copies and
//! the review gate are the two writes nobody commands, and `refresh` and
//! `review.start` are the only operations that make them; neither touches
//! the undo stack. So looking at a day makes no copy, looking at the
//! review does not spend it, and `context.recurrence_pending` answers
//! whether generation has something to do without doing it.
//!
//! **Nothing is guessed.** Every request structure refuses a field it
//! does not know, every value is held to its range rather than clamped
//! into it, and an id, a date or a position that does not name what it
//! claims to is an error with the reason in it.

use jiff::Zoned;
use jiff::civil::Date;
use serde_json::{Value, json};

use crate::app::{UNDO_CAP, operations};
use crate::domain::{
    self, Change, Command, Context, DateOrder, DateStyle, Model, Rejected, Store, StoreError,
};

mod config;
mod dto;
mod notes;
mod order;
mod reads;
mod schedules;
mod spec;
mod system;
mod tasks;

#[cfg(test)]
mod tests;

/// The version of the envelope. A response that means something else is
/// a different number, never the same number with another shape.
const SCHEMA_VERSION: u64 = 1;

/// Why a request could not be answered: the code a caller matches on, the
/// sentence a person reads, and the status the process leaves with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: String,
    pub message: String,
    pub exit_code: u8,
}

impl Error {
    fn new(code: &str, exit_code: u8, message: impl Into<String>) -> Error {
        Error {
            code: code.to_owned(),
            message: message.into(),
            exit_code,
        }
    }

    /// The request is not a request: no operation, an operation nobody
    /// has, a field nobody has, or a value of the wrong kind.
    pub(crate) fn invalid_request(message: impl Into<String>) -> Error {
        Error::new("invalid_request", 2, message)
    }

    /// The right shape saying something that cannot be meant: a date that
    /// is not a date, an id twice, a position past the end.
    pub(crate) fn invalid_argument(message: impl Into<String>) -> Error {
        Error::new("invalid_argument", 2, message)
    }

    /// `confirm_delete` is on and the request did not say `confirm`.
    pub(crate) fn confirmation_required(message: impl Into<String>) -> Error {
        Error::new("confirmation_required", 2, message)
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Error {
        Error::new("not_found", 3, message)
    }

    /// A rule of the domain, in the domain's own words.
    pub(crate) fn rejected(rejected: Rejected) -> Error {
        Error::new("rejected", 4, rejected.0)
    }

    pub(crate) fn conflict(message: impl Into<String>) -> Error {
        Error::new("conflict", 5, message)
    }

    pub(crate) fn store(error: StoreError) -> Error {
        match error {
            StoreError::Conflict => Error::conflict(error.to_string()),
            StoreError::Other(_) => Error::new("storage_error", 1, error.to_string()),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// One request against one store.
///
/// The model is loaded once and the operation works from that value, so
/// what a response reports is the model the change actually landed on
/// rather than a second read of the database.
pub fn execute(
    store: &mut dyn Store,
    request: Value,
    now: &Zoned,
    dates: DateOrder,
) -> Result<Value, Error> {
    let model = store.load().map_err(Error::store)?;
    let mut session = Session {
        store,
        model,
        now: now.clone(),
        locale: dates,
    };
    let (op, fields) = split(request)?;
    let data = session.run(&op, fields)?;
    Ok(session.envelope(data))
}

/// How a date is written, as a response names it.
pub(crate) fn order_name(order: DateOrder) -> &'static str {
    match order {
        DateOrder::DayFirst => "day_first",
        DateOrder::MonthFirst => "month_first",
    }
}

/// The operation a request names, and the rest of it.
fn split(request: Value) -> Result<(String, Value), Error> {
    let Value::Object(mut object) = request else {
        return Err(Error::invalid_request(
            "A request is a JSON object with an `op` in it.",
        ));
    };
    match object.remove("op") {
        Some(Value::String(op)) => Ok((op, Value::Object(object))),
        Some(_) => Err(Error::invalid_request(
            "`op` is the name of an operation, written as a string.",
        )),
        None => Err(Error::invalid_request(
            "A request needs an `op` naming the operation.",
        )),
    }
}

/// What an operation is carried out against: the store, the model loaded
/// from it, the instant the request was given and the order the
/// environment writes dates in.
pub(crate) struct Session<'a> {
    store: &'a mut dyn Store,
    model: Model,
    now: Zoned,
    locale: DateOrder,
}

impl Session<'_> {
    fn run(&mut self, op: &str, fields: Value) -> Result<Value, Error> {
        match op {
            "task.list" => reads::task_list(self, fields),
            "task.get" => reads::task_get(self, fields),
            "day.get" => reads::day_get(self, fields),
            "backlog.get" => reads::backlog_get(self, fields),
            "history.list" => reads::history_list(self, fields),
            "search" => reads::search(self, fields),

            "task.add" => tasks::add(self, fields),
            "task.update" => tasks::update(self, fields),
            "task.close" => tasks::close(self, fields),
            "task.reopen" => tasks::reopen(self, fields),
            "task.move" => tasks::move_to(self, fields),
            "task.delete" => tasks::delete(self, fields),
            "task.reorder" => tasks::reorder(self, fields),
            "day.reorder" => tasks::reorder_all(self, fields),

            "schedule.list" => schedules::list(self, fields),
            "schedule.get" => schedules::get(self, fields),
            "schedule.preview" => schedules::preview(self, fields),
            "schedule.create" => schedules::create(self, fields),
            "schedule.update" => schedules::update(self, fields),
            "schedule.stop" => schedules::stop(self, fields),

            "note.list" => notes::list(self, fields),
            "note.get" => notes::get(self, fields),
            "note.create" => notes::create(self, fields),
            "note.update" => notes::update(self, fields),
            "note.delete" => notes::delete(self, fields),
            "note.archive" => notes::archive(self, fields),
            "note.unarchive" => notes::unarchive(self, fields),
            "note.check" => notes::check(self, fields),

            "settings.get" => config::settings_get(self, fields),
            "settings.set" => config::settings_set(self, fields),
            "dictionary.list" => config::dictionary_list(self, fields),
            "dictionary.add" => config::dictionary_add(self, fields),
            "dictionary.update" => config::dictionary_update(self, fields),
            "dictionary.delete" => config::dictionary_delete(self, fields),

            "review.get" => system::review_get(self, fields),
            "review.start" => system::review_start(self, fields),
            "refresh" => system::refresh(self, fields),
            "undo.get" => system::undo_get(self, fields),
            "undo.apply" => system::undo_apply(self, fields),

            _ => Err(Error::invalid_request(format!(
                "There is no operation called {op:?}."
            ))),
        }
    }

    /// The whole response. The context is read off the model the
    /// operation left, so a request that moved the hour a day starts at
    /// reports the day it moved to and one that changed `date_style`
    /// reports the order dates are written in from now on.
    fn envelope(&self, data: Value) -> Value {
        json!({
            "schema_version": SCHEMA_VERSION,
            "ok": true,
            "data": data,
            "context": {
                "today": self.today().to_string(),
                "date_order": order_name(self.dates()),
                "recurrence_pending": self.recurrence_pending(),
            },
        })
    }

    pub(crate) fn today(&self) -> Date {
        self.model.settings.working_day(&self.now)
    }

    pub(crate) fn model(&self) -> &Model {
        &self.model
    }

    /// Which way round a date is written: what the setting says, or what
    /// the environment does where the setting leaves it to the locale.
    pub(crate) fn dates(&self) -> DateOrder {
        match self.model.settings.date_style() {
            DateStyle::Locale => self.locale,
            DateStyle::DayFirst => DateOrder::DayFirst,
            DateStyle::MonthFirst => DateOrder::MonthFirst,
        }
    }

    pub(crate) fn context(&self) -> Context {
        Context {
            now: self.now.clone(),
            undo_cap: UNDO_CAP,
            dates: self.dates(),
        }
    }

    /// Whether generation has copies to make, asked without making them.
    fn recurrence_pending(&self) -> bool {
        !domain::generate_copies(&self.model, &self.now)
            .writes
            .is_empty()
    }

    /// The model these commands would leave, for working out the part of
    /// an operation that depends on what the part before it did: which id
    /// a new task got, which position a task holds once it has moved.
    ///
    /// It is the same call the commit makes, so a command that would be
    /// refused is refused here, before anything has been written.
    pub(crate) fn foresee(&self, commands: &[Command]) -> Result<Model, Error> {
        let change = domain::apply_many(&self.model, commands.to_vec(), &self.context())
            .map_err(Error::rejected)?;
        let mut model = self.model.clone();
        model.apply(&change);
        Ok(model)
    }

    /// The commands, committed as one change, and the undo entry they
    /// earned. An operation that turns out to be no change at all writes
    /// nothing and earns none.
    pub(crate) fn commit(&mut self, commands: Vec<Command>) -> Result<Value, Error> {
        if commands.is_empty() {
            return Ok(Value::Null);
        }
        let change =
            domain::apply_many(&self.model, commands, &self.context()).map_err(Error::rejected)?;
        let top = self.model.undo.last().map(|entry| entry.id);
        self.commit_change(&change)?;
        Ok(match self.model.undo.last() {
            Some(entry) if Some(entry.id) != top => dto::undo_entry(entry),
            _ => Value::Null,
        })
    }

    /// A change the domain made outside the command path: generation, the
    /// review gate, the settings, the dictionary, an undo.
    pub(crate) fn commit_change(&mut self, change: &Change) -> Result<(), Error> {
        if change.writes.is_empty() {
            return Ok(());
        }
        operations::commit_change(self.store, &mut self.model, change).map_err(Error::store)
    }
}
