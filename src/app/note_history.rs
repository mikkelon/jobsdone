//! Window-local text history, independent of task decisions and autosave.

use super::{App, Id, glyphs};
use crate::input::Action;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    text: String,
    caret: usize,
    anchor: Option<usize>,
    first: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditKind {
    Typing,
    Backspace,
    Delete,
    Separate,
}

#[derive(Default)]
pub(super) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    group: Option<(EditKind, Snapshot)>,
    current: Option<String>,
}

impl History {
    pub(super) fn end_if_changed(&mut self, body: &str) {
        if self
            .current
            .as_deref()
            .is_some_and(|current| current != body)
        {
            *self = Self::default();
        }
        self.group = None;
    }
}

impl App {
    pub(super) fn note_snapshot(&self) -> Option<(Id, Snapshot)> {
        let draft = self.draft.as_ref()?;
        Some((
            draft.note,
            Snapshot {
                text: draft.text.clone(),
                caret: draft.caret,
                anchor: self.selection_anchor,
                first: draft.first,
            },
        ))
    }

    pub(super) fn record_note_edit(&mut self, before: Option<(Id, Snapshot)>, kind: EditKind) {
        let Some((note, before)) = before else { return };
        let Some((after_note, after)) = self.note_snapshot() else {
            return;
        };
        if note != after_note || before.text == after.text {
            return;
        }
        let history = self.note_history.entry(note).or_default();
        let joined = kind != EditKind::Separate
            && history
                .group
                .as_ref()
                .is_some_and(|(previous_kind, previous)| {
                    *previous_kind == kind && *previous == before
                });
        if !joined {
            history.undo.push(before);
            if history.undo.len() > 100 {
                history.undo.remove(0);
            }
        }
        history.redo.clear();
        history.current = Some(after.text.clone());
        history.group = Some((kind, after));
    }

    pub(super) fn end_note_edit_group(&mut self) {
        if let Some(draft) = &self.draft
            && let Some(history) = self.note_history.get_mut(&draft.note)
        {
            history.group = None;
        }
    }

    pub(super) fn undo_note_edit(&mut self, redo: bool) {
        if self.popup.is_some() {
            return;
        }
        let Some((note, current)) = self.note_snapshot() else {
            return;
        };
        let history = self.note_history.entry(note).or_default();
        history.group = None;
        let (source, destination) = if redo {
            (&mut history.redo, &mut history.undo)
        } else {
            (&mut history.undo, &mut history.redo)
        };
        let Some(snapshot) = source.pop() else {
            self.say(
                if redo {
                    "No note edit to redo."
                } else {
                    "No note edit to undo."
                },
                false,
            );
            return;
        };
        destination.push(current);
        history.current = Some(snapshot.text.clone());
        if let Some(draft) = &mut self.draft {
            draft.text = snapshot.text;
            draft.caret = snapshot.caret.min(glyphs(&draft.text));
            draft.first = snapshot.first;
            draft.wanted = None;
            draft.affinity = Default::default();
        }
        self.selection_anchor = snapshot.anchor;
        self.say(
            if redo {
                "Note edit redone."
            } else {
                "Note edit undone."
            },
            false,
        );
    }
}

pub(super) fn edit_kind(action: Action, selection: bool) -> Option<EditKind> {
    Some(match action {
        Action::Insert(_) if selection => EditKind::Separate,
        Action::Insert('\n') => EditKind::Separate,
        Action::Insert(_) => EditKind::Typing,
        Action::Backspace if !selection => EditKind::Backspace,
        Action::DeleteForward if !selection => EditKind::Delete,
        Action::Backspace | Action::DeleteForward | Action::DeleteWordBackward => {
            EditKind::Separate
        }
        _ => return None,
    })
}
