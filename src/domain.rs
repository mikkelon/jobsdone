//! The model, every rule, commands and their inverses, the views of the
//! model, rule dates, and the working day.
//!
//! The domain imports nothing else in the crate, reads no clock and knows
//! no I/O. It is handed an instant and derives the working day itself.

use std::collections::BTreeMap;
use std::fmt;

use jiff::civil::Date;
use jiff::{Span, Zoned};

#[cfg(test)]
mod tests;

/// The hour a day begins, so 01:30 on Saturday belongs to Friday. A
/// constant, not a setting (DOMAIN.md section 2).
const DAY_STARTS_AT: i64 = 5;

/// The working day of an instant.
pub fn working_day(instant: &Zoned) -> Date {
    instant
        .saturating_sub(Span::new().hours(DAY_STARTS_AT))
        .date()
}

/// The whole state as a value, loaded from storage in one go.
///
/// Phase 5 adds tasks, placements, schedules, notes and the undo stack.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    /// The `meta` table: `review_on`, `review_before`, and whatever else
    /// is keyed by name.
    pub meta: BTreeMap<String, String>,
}

impl Model {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Applies a change that storage has already committed.
    pub fn apply(&mut self, change: &Change) {
        for write in &change.writes {
            match write {
                Write::SetMeta { key, value } => {
                    self.meta.insert(key.clone(), value.clone());
                }
            }
        }
    }
}

/// One of the commands in DOMAIN.md section 12, carrying ids and never
/// cursor positions.
///
/// Phase 5 fills this in. Until then it has no variants, which is what
/// makes the empty match in [`apply`] exhaustive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {}

/// What a command does, as a list of row writes. Whole rows, never
/// fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub writes: Vec<Write>,
}

/// One row write. The remaining variants of ARCHITECTURE.md section 3
/// arrive with the rows they write, in phase 5.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    SetMeta { key: String, value: String },
}

/// Why a command was refused, as the sentence the hint bar shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected(pub String);

/// Every user command: what it would change, or why it is refused.
pub fn apply(model: &Model, command: Command, now: &Zoned) -> Result<Change, Rejected> {
    let _ = (model, now);
    match command {}
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
