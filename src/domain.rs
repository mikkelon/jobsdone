//! The model, every rule, commands and their inverses, the views of the
//! model, rule dates, and the working day.
//!
//! The domain imports nothing else in the crate, reads no clock and knows
//! no I/O. It is handed an instant and derives the working day itself.

use std::fmt;

use jiff::civil::Date;
use jiff::{Span, Zoned};

mod command;
mod model;
mod rule;
mod system;
mod view;

#[cfg(test)]
pub(crate) mod tests;

pub use self::command::{Command, Undone, apply, undo};
pub use self::model::{
    Change, FromPlace, Id, Model, Note, Place, Placement, REVIEW_BEFORE, REVIEW_ON, Schedule, Task,
    UndoEntry, Write,
};
pub use self::rule::{MonthDay, Rule, Weekday, next_dates};
pub use self::system::{generate_copies, start_review};
pub use self::view::{
    BacklogView, DayCounts, DayList, DayListRow, DayView, DueChip, NoteRow, NotesView, Pile,
    PileDay, Row, ScheduleRow, SearchResults, Surfaced, backlog_view, day_list, day_view, notes,
    pile, previous_review, search, surfaced,
};

/// The hour a day begins, so 01:30 on Saturday belongs to Friday. A
/// constant, not a setting (DOMAIN.md section 2).
const DAY_STARTS_AT: i64 = 5;

/// The working day of an instant.
pub fn working_day(instant: &Zoned) -> Date {
    instant
        .saturating_sub(Span::new().hours(DAY_STARTS_AT))
        .date()
}

/// Why a command was refused, as the sentence the hint bar shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected(pub String);

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What the domain needs of persistence. Storage implements it; the
/// application only ever sees this.
pub trait Store {
    fn load(&self) -> Result<Model, StoreError>;
    fn commit(&mut self, change: &Change) -> Result<(), StoreError>;
    fn version(&self) -> Result<u64, StoreError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// A unique or primary key violation: another window got there first.
    Conflict,
    Other(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Conflict => f.write_str("another window changed the same thing"),
            StoreError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StoreError {}
