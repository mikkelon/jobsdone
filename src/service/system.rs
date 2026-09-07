//! The three things the program does to itself, and undo.
//!
//! Generation and the review gate are the only writes that are not
//! somebody's command, and neither goes on the undo stack. The window
//! does both at launch; here they are asked for by name, so that reading
//! a day never makes a copy and reading the review never spends it.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Error, Session, dto, spec};
use crate::domain::{self, Id, REVIEW_BEFORE, REVIEW_ON};

/// The pile, the surfaced set and where the gate stands, without moving
/// any of them.
pub(super) fn review_get(session: &Session, fields: Value) -> Result<Value, Error> {
    let _: spec::Nothing = spec::fields("review.get", fields)?;
    Ok(review(session))
}

pub(super) fn review_start(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let _: spec::Nothing = spec::fields("review.start", fields)?;
    let started = match domain::start_review(session.model(), session.today()) {
        Some(change) => {
            session.commit_change(&change)?;
            true
        }
        None => false,
    };
    let mut data = review(session);
    if let Some(object) = data.as_object_mut() {
        object.insert("started".to_owned(), Value::Bool(started));
        object.insert("undo".to_owned(), Value::Null);
    }
    Ok(data)
}

fn review(session: &Session) -> Value {
    let (model, today) = (session.model(), session.today());
    let pile = domain::pile(model, today);
    let surfaced = domain::surfaced(model, today);
    let started_today = model.meta_date(REVIEW_ON) == Some(today);
    let opens_itself = model.settings.review_opens_itself();

    json!({
        "pile": dto::pile(&pile),
        "surfaced": dto::surfaced(&surfaced),
        "gate": {
            "review_on": model.meta.get(REVIEW_ON),
            "review_before": model.meta.get(REVIEW_BEFORE),
            "previous_review": domain::previous_review(model, today).map(|date| date.to_string()),
            "started_today": started_today,
            "opens_itself": opens_itself,
            "would_open": opens_itself
                && !started_today
                && (pile.total > 0 || !surfaced.is_empty()),
        },
    })
}

/// The copies every schedule owes, made now.
pub(super) fn refresh(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let _: spec::Nothing = spec::fields("refresh", fields)?;
    let before: Vec<Id> = session.model().tasks.keys().copied().collect();

    let change = domain::generate_copies(session.model(), &session.context().now);
    session.commit_change(&change)?;

    let model = session.model();
    let made: Vec<Id> = model
        .tasks
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect();
    Ok(json!({
        "count": made.len(),
        "created": dto::tasks(model, &made),
        "undo": Value::Null,
    }))
}

// ---- undo ------------------------------------------------------------

pub(super) fn undo_get(session: &Session, fields: Value) -> Result<Value, Error> {
    let _: spec::Nothing = spec::fields("undo.get", fields)?;
    let model = session.model();
    Ok(json!({
        "entry": model.undo.last().map(dto::undo_entry),
        "depth": model.undo.len(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    expected_id: Option<Id>,
}

/// The top entry, taken back.
///
/// `expected_id` is how a script that read the stack and then decided
/// says which entry it decided about: between the two calls another
/// window may have done something else, and taking back the wrong change
/// is worse than being told the stack moved.
pub(super) fn undo_apply(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Apply = spec::fields("undo.apply", fields)?;
    let Some(entry) = session.model().undo.last().cloned() else {
        return Err(Error::not_found("There is nothing to undo."));
    };
    if let Some(expected) = request.expected_id
        && expected != entry.id
    {
        return Err(Error::conflict(format!(
            "The change on top of the stack is {} now, not {expected}.",
            entry.id
        )));
    }

    let undone = domain::undo(session.model(), &session.context()).map_err(Error::rejected)?;
    session.commit_change(&undone.change)?;

    Ok(json!({
        "applied": undone.dropped.is_none(),
        "entry_id": entry.id,
        "label": undone.label,
        "task": undone.task,
        "dropped": undone.dropped.map(|why| why.0),
        "depth": session.model().undo.len(),
    }))
}
