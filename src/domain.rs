//! The model, every rule, commands and their inverses, the views of the
//! model, rule dates, the settings, and the working day.
//!
//! The domain imports nothing else in the crate, reads no clock and knows
//! no I/O. It is handed an instant and derives the working day itself,
//! from the hour the settings say a day begins.

use std::fmt;

use jiff::Zoned;

mod command;
mod date;
mod dictionary;
pub mod fuzzy;
mod model;
mod rule;
mod settings;
mod system;
mod view;

#[cfg(test)]
pub(crate) mod tests;

pub use self::command::{Command, Undone, apply, apply_many, undo};
pub use self::date::{
    Looking, day_label, end_of_week, last_weekday, parse_date, short_label, stamp_label,
    start_of_month, start_of_week,
};
pub use self::dictionary::{
    add_dictionary_word, dictionary_key, edit_dictionary_word, remove_dictionary_word,
};
pub use self::model::{
    Change, FromPlace, Id, Model, Note, Place, Placement, REVIEW_BEFORE, REVIEW_ON, Schedule, Task,
    UndoEntry, Write,
};
pub use self::rule::{MonthDay, Rule, Weekday, next_dates};
pub use self::settings::{
    DateOrder, DateStyle, Settings, WeekStart, WindowSize, WorkDays, change_settings,
};
pub use self::system::{generate_copies, start_review};
pub use self::view::{
    BacklogView, DayCounts, DayList, DayListRow, DayStretch, DayView, DueChip, NoteRow, NotesView,
    Pile, PileDay, Row, ScheduleRow, SearchResults, Stretch, Surfaced, archived_notes,
    backlog_view, day_list, day_view, notes, pile, pile_again, previous_review, search, surfaced,
    surfaced_again,
};

/// What the domain is told about the world outside it: the instant the
/// action was given, how long the undo stack is held to, and the order
/// dates are written in. The domain reads no clock, chooses no cap and
/// knows no locale, so all three come in with the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub now: Zoned,
    pub undo_cap: usize,
    pub dates: DateOrder,
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
