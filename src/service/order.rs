//! Positions, in the two senses of the word.
//!
//! A place is one dense sequence of **all** its live tasks, closed ones
//! included (DOMAIN.md section 4), and `Command::Reorder` speaks in
//! indices into that sequence. A caller has no reason to know that: a
//! closed task is drawn in the Done group, ordered by when it was
//! closed, so counting it would make the second open task of a day the
//! fifth position of it for no reason anybody could see. So the position
//! a request writes is one-based among the **open** tasks of the place,
//! and this module is the whole of the translation between the two.
//!
//! Reordering therefore permutes the open tasks through the slots the
//! open tasks already hold, leaving every closed task exactly where it
//! is. A day's record is not rewritten by somebody sorting today's plan.

use std::collections::BTreeSet;

use crate::domain::{Command, Id, Model, Place, Task};

/// The ids of a place in their underlying order, closed tasks included.
pub(super) fn slots(model: &Model, place: Place) -> Vec<Id> {
    model.place(place).iter().map(|task| task.id).collect()
}

/// The ids of the open tasks of a place, in their order. This is the
/// sequence a position counts along.
pub(super) fn open_order(model: &Model, place: Place) -> Vec<Id> {
    model
        .place(place)
        .iter()
        .filter(|task| task.is_open())
        .map(|task| task.id)
        .collect()
}

/// A task's one-based position among the open tasks of its place, or
/// none for a closed task, which has no position anybody names.
pub(super) fn position_of(model: &Model, task: &Task) -> Option<usize> {
    if !task.is_open() {
        return None;
    }
    open_order(model, task.place())
        .iter()
        .position(|id| *id == task.id)
        .map(|at| at + 1)
}

/// The commands that leave the open tasks of a place in `wanted`.
///
/// `wanted` is the whole of them: the caller has already checked that it
/// is the same set. Each command moves one task to one underlying index,
/// walking the target order from the front, so a list already in the
/// order asked for costs no commands at all.
pub(super) fn reorder(model: &Model, place: Place, wanted: &[Id]) -> Vec<Command> {
    let current = slots(model, place);
    let open: BTreeSet<Id> = open_order(model, place).into_iter().collect();

    let mut wanted = wanted.iter();
    let target: Vec<Id> = current
        .iter()
        .map(|id| {
            if open.contains(id) {
                wanted.next().copied().unwrap_or(*id)
            } else {
                *id
            }
        })
        .collect();

    let mut order = current;
    let mut commands = Vec::new();
    for (index, id) in target.iter().enumerate() {
        if order.get(index) == Some(id) {
            continue;
        }
        let Some(from) = order.iter().position(|other| other == id) else {
            continue;
        };
        order.remove(from);
        order.insert(index, *id);
        commands.push(Command::Reorder {
            task: *id,
            position: index,
        });
    }
    commands
}
