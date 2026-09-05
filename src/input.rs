//! A terminal event plus the current key context in, a named action out.
//! Owns the key table.
//!
//! This module knows nothing about tasks: an action names what the user
//! asked for, never what it is asked of.

use crossterm::event::{Event, KeyCode, KeyEventKind};

/// A pane of a two-pane screen, or the tab that stands for it when the
/// window is too narrow for both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Day,
    Backlog,
}

/// Where the keyboard is, which is what decides what a key means.
///
/// Phase 6 adds `Notes`, `Review` and `Popup`, and the `text_field`
/// overlay starts doing its work: with it set, printable keys become
/// `Insert` and only the editing keys keep a name of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyContext {
    Home { pane: Pane, text_field: bool },
}

/// What the user asked for. Cursor-relative, never carrying an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Tick,
    Resize,
    FocusGained,
}

/// One row of the key table: a key, what it does, and what to call it.
///
/// The hint bar, the command palette and the help overlay are drawn from
/// these rows and from nothing else, so none of them can disagree with
/// the dispatcher.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub key: &'static str,
    pub action: Action,
    pub label: &'static str,
}

const HOME: &[Binding] = &[Binding {
    key: "q",
    action: Action::Quit,
    label: "quit",
}];

/// The rows of the key table for a context.
pub fn bindings(context: KeyContext) -> &'static [Binding] {
    match context {
        KeyContext::Home { .. } => HOME,
    }
}

/// The action a terminal event means in a context, if it means one.
pub fn action_for(event: &Event, context: KeyContext) -> Option<Action> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Char(typed) => bindings(context)
                .iter()
                .find(|binding| binding.key == typed.to_string())
                .map(|binding| binding.action),
            _ => None,
        },
        Event::Resize(_, _) => Some(Action::Resize),
        Event::FocusGained => Some(Action::FocusGained),
        _ => None,
    }
}
