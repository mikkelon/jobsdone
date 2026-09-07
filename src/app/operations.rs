//! The one way a change reaches storage and then the model.
//!
//! Every caller — the window, `jobsdone desktop`, and the noninteractive
//! commands — has the same two steps to take in the same order, and the
//! order is the whole of it: storage first, the model only if storage
//! took it. A model advanced past a commit that failed is a window
//! showing rows that are not there, and the next change it works out
//! from those rows would write them over rows that are (ARCHITECTURE.md
//! rule 10).

use crate::domain::{Change, Model, Store, StoreError};

/// Commits a change and, only if the commit succeeded, applies it to the
/// model.
///
/// The model is left exactly as it was on any failure, `Conflict`
/// included: a conflict says the change was worked out from rows that
/// have since moved, so the answer is to load the model again and decide
/// afresh, never to send the same change a second time. What it would
/// write is true only of the model it was made from — the ids it
/// allocated, the positions it renumbered and the entry it pops off the
/// undo stack are all that model's.
pub fn commit_change(
    store: &mut dyn Store,
    model: &mut Model,
    change: &Change,
) -> Result<(), StoreError> {
    store.commit(change)?;
    model.apply(change);
    Ok(())
}
