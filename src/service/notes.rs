//! Notes, and the spelling of what is in them.
//!
//! A note body arrives whole: newlines, tabs and any Unicode, stored as
//! they were given. The window saves a body on the tick because it is
//! being typed and nobody decided it yet; a request here is somebody
//! handing over the replacement they wrote, so it is one change that can
//! be taken back.
//!
//! The check is the checker the window underlines with, personal
//! dictionary and all, so a word the window passes over is a word this
//! passes over. It reads: no note is rewritten and nothing is stored.

use serde::Deserialize;
use serde_json::{Value, json};
use unicode_segmentation::UnicodeSegmentation;

use super::{Error, Session, dto, spec};
use crate::app::SpellChecker;
use crate::domain::{self, Command, Id, Note};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    #[serde(default)]
    include_body: bool,
    #[serde(default)]
    archived: bool,
}

/// The notes in the list, or the archived ones when asked for.
pub(super) fn list(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: List = spec::fields("note.list", fields)?;
    let model = session.model();
    let view = if request.archived {
        domain::archived_notes(model)
    } else {
        domain::notes(model)
    };
    Ok(dto::notes(model, &view, request.include_body))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct One {
    id: Id,
}

pub(super) fn get(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: One = spec::fields("note.get", fields)?;
    Ok(json!({"note": dto::note(live(session, request.id)?)}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    #[serde(default)]
    body: String,
}

/// A note with its body already in it, as one change.
pub(super) fn create(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Create = spec::fields("note.create", fields)?;

    let mut commands = vec![Command::CreateNote];
    let added = session.foresee(&commands)?;
    let id = added
        .notes
        .keys()
        .find(|id| !session.model().notes.contains_key(id))
        .copied()
        .ok_or_else(|| Error::invalid_argument("That note was not added."))?;

    if !request.body.is_empty() {
        commands.push(Command::ReplaceNote {
            note: id,
            body: request.body,
        });
    }

    let undo = session.commit(commands)?;
    Ok(json!({"note": dto::note(live(session, id)?), "undo": undo}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: Id,
    body: String,
}

pub(super) fn update(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Update = spec::fields("note.update", fields)?;
    live(session, request.id)?;
    let undo = session.commit(vec![Command::ReplaceNote {
        note: request.id,
        body: request.body,
    }])?;
    Ok(json!({"note": dto::note(live(session, request.id)?), "undo": undo}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Delete {
    id: Id,
    #[serde(default)]
    confirm: bool,
}

pub(super) fn delete(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Delete = spec::fields("note.delete", fields)?;
    live(session, request.id)?;
    if session.model().settings.confirm_delete() && !request.confirm {
        return Err(Error::confirmation_required(
            "Deleting asks first while `confirm_delete` is on. Say `confirm` to go ahead.",
        ));
    }
    let undo = session.commit(vec![Command::DeleteNote { note: request.id }])?;
    let model = session.model();
    let note = model.note(request.id).ok_or_else(|| missing(request.id))?;
    Ok(json!({"note": dto::note(note), "undo": undo}))
}

/// Archiving and unarchiving: the domain says whether the note is where
/// the request expects it.
pub(super) fn archive(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: One = spec::fields("note.archive", fields)?;
    live(session, request.id)?;
    let undo = session.commit(vec![Command::ArchiveNote { note: request.id }])?;
    Ok(json!({"note": dto::note(live(session, request.id)?), "undo": undo}))
}

pub(super) fn unarchive(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: One = spec::fields("note.unarchive", fields)?;
    live(session, request.id)?;
    let undo = session.commit(vec![Command::UnarchiveNote { note: request.id }])?;
    Ok(json!({"note": dto::note(live(session, request.id)?), "undo": undo}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpellCheck {
    id: Id,
    check: bool,
}

/// One note out of spell checking, or back into it: the domain says
/// whether it already is.
pub(super) fn spell_check(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: SpellCheck = spec::fields("note.spell_check", fields)?;
    live(session, request.id)?;
    let undo = session.commit(vec![Command::SetNoteSpellCheck {
        note: request.id,
        check: request.check,
    }])?;
    Ok(json!({"note": dto::note(live(session, request.id)?), "undo": undo}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    note: Option<Id>,
    text: Option<String>,
    #[serde(default)]
    suggestions: bool,
}

/// The words US English does not know, as ranges into the text.
///
/// The ranges are in grapheme clusters, end exclusive, which is the unit
/// a note's caret counts in, so a caller can point at a word the same
/// way the window does. The check runs whether or not `spell_check_notes`
/// is on, and on a note taken out of checking: the setting and the note's
/// own switch say whether a note is underlined while it is being typed,
/// not whether this question may be asked.
pub(super) fn check(session: &Session, fields: Value) -> Result<Value, Error> {
    let request: Check = spec::fields("note.check", fields)?;
    let text = match (&request.note, &request.text) {
        (Some(id), None) => live(session, *id)?.body.clone(),
        (None, Some(text)) => text.clone(),
        _ => {
            return Err(Error::invalid_argument(
                "A check is of one `note` or of one piece of `text`.",
            ));
        }
    };

    let mut checker = SpellChecker::default();
    checker.set_personal_dictionary(&session.model().personal_dictionary);
    let found = checker.check(&text);

    let clusters: Vec<&str> = text.graphemes(true).collect();
    let misspellings: Vec<Value> = found
        .iter()
        .map(|range| {
            let word: String =
                clusters[range.start.min(clusters.len())..range.end.min(clusters.len())].concat();
            let mut entry = json!({"start": range.start, "end": range.end, "word": word});
            if request.suggestions
                && let Some(object) = entry.as_object_mut()
            {
                let offered = checker.suggestions(&word);
                object.insert(
                    "suggestions".to_owned(),
                    Value::Array(offered.into_iter().map(Value::String).collect()),
                );
            }
            entry
        })
        .collect();

    Ok(json!({
        "misspellings": misspellings,
        "spell_check_notes": session.model().settings.spell_check_notes(),
    }))
}

fn live<'a>(session: &'a Session<'_>, id: Id) -> Result<&'a Note, Error> {
    session
        .model()
        .note(id)
        .filter(|note| note.is_live())
        .ok_or_else(|| missing(id))
}

fn missing(id: Id) -> Error {
    Error::not_found(format!("There is no note {id}."))
}
