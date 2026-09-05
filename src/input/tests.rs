use super::*;

use crossterm::event::{KeyEventState, MouseButton};

fn home(pane: Pane) -> KeyContext {
    KeyContext::Home {
        pane,
        day: Shown::Today,
        field: None,
    }
}

/// The same two panes with the day pane stepped off today.
fn browsing(pane: Pane) -> KeyContext {
    KeyContext::Home {
        pane,
        day: Shown::Past,
        field: None,
    }
}

fn writing(field: Field) -> KeyContext {
    KeyContext::Home {
        pane: Pane::Day,
        day: Shown::Today,
        field: Some(field),
    }
}

fn popup(kind: PopupKind) -> KeyContext {
    KeyContext::Popup {
        kind,
        text_field: false,
    }
}

fn field(kind: PopupKind) -> KeyContext {
    KeyContext::Popup {
        kind,
        text_field: true,
    }
}

/// Every context there is, so a test can sweep the whole table.
fn every_context() -> Vec<KeyContext> {
    vec![
        home(Pane::Day),
        home(Pane::Backlog),
        browsing(Pane::Day),
        browsing(Pane::Backlog),
        KeyContext::Notes {
            pane: NotesPane::List,
            text_field: false,
        },
        KeyContext::Notes {
            pane: NotesPane::Note,
            text_field: true,
        },
        KeyContext::Review {
            step: ReviewStep::Pile,
            text_field: false,
        },
        KeyContext::Review {
            step: ReviewStep::Surfaced,
            text_field: false,
        },
        KeyContext::Review {
            step: ReviewStep::Pile,
            text_field: true,
        },
        writing(Field::Adding),
        writing(Field::Renaming),
        field(PopupKind::Palette),
        field(PopupKind::Search),
        popup(PopupKind::Help),
        popup(PopupKind::Move),
        popup(PopupKind::CopyQuestion),
        field(PopupKind::Date),
        popup(PopupKind::Date),
        popup(PopupKind::Repeat),
    ]
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn press_with(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

fn typing(typed: char) -> Event {
    press(KeyCode::Char(typed))
}

#[test]
fn every_list_of_rows_moves_with_j_and_k() {
    // Every list but the review's, where `k` is "keep" and the arrows
    // stand in for it (DESIGN.md section 4).
    let lists = [
        home(Pane::Day),
        home(Pane::Backlog),
        browsing(Pane::Day),
        browsing(Pane::Backlog),
        KeyContext::Notes {
            pane: NotesPane::List,
            text_field: false,
        },
    ];
    for context in lists {
        for wanted in [
            ("j", Action::Down),
            ("k", Action::Up),
            ("down", Action::Down),
            ("up", Action::Up),
        ] {
            assert!(
                bindings(context)
                    .iter()
                    .flat_map(|binding| binding.keys)
                    .any(|key| *key == wanted),
                "{context:?} does not move with {:?}",
                wanted.0
            );
        }
    }
}

#[test]
fn no_context_binds_a_key_twice() {
    for context in every_context() {
        let mut seen: Vec<&str> = Vec::new();
        for binding in bindings(context) {
            for (key, _) in binding.keys {
                assert!(
                    !seen.contains(key),
                    "{context:?} binds {key:?} twice, so one of them is unreachable"
                );
                seen.push(key);
            }
        }
    }
}

#[test]
fn every_key_the_review_panel_offers_is_a_key_of_that_step() {
    for step in [ReviewStep::Pile, ReviewStep::Surfaced] {
        let context = KeyContext::Review {
            step,
            text_field: false,
        };
        for decision in decisions(step) {
            assert!(
                bindings(context)
                    .iter()
                    .flat_map(|binding| binding.keys)
                    .any(|key| *key == (decision.key, decision.action)),
                "the {step:?} panel offers {:?}, which the step does not bind",
                decision.key
            );
        }
    }
}

#[test]
fn a_title_typed_on_a_review_row_saves_with_enter() {
    let context = KeyContext::Review {
        step: ReviewStep::Pile,
        text_field: true,
    };

    assert!(context.text_field());
    assert_eq!(
        action_for(&press(KeyCode::Enter), context),
        Some(Action::Confirm)
    );
    assert_eq!(action_for(&typing('x'), context), Some(Action::Insert('x')));
    // The step's own Enter would have moved the review on instead.
    assert_eq!(
        bindings(context)
            .iter()
            .find(|binding| binding.shown == "⏎")
            .map(|binding| binding.label),
        Some("save")
    );
}

#[test]
fn every_row_the_hint_bar_shows_has_a_name_to_show() {
    for context in every_context() {
        for binding in bindings(context) {
            assert!(!binding.shown.is_empty(), "{context:?} has a nameless row");
            // A row that binds a key says what the key does. A row that
            // binds none may be a caption over the rows beside it in the
            // bar, which is all "in the list:" is.
            assert!(
                !binding.label.is_empty() || binding.keys.is_empty(),
                "{:?} has no label",
                binding.shown
            );
        }
    }
}

/// A key press written the way the table writes it.
fn event_for(name: &str) -> Event {
    let (name, alt) = match name.strip_prefix("alt-") {
        Some(rest) => (rest, KeyModifiers::ALT),
        None => (name, KeyModifiers::NONE),
    };
    let code = match name {
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        other => {
            let mut letters = other.chars();
            let only = letters.next().expect("a key name");
            assert!(letters.next().is_none(), "{other:?} is not a key name");
            KeyCode::Char(only)
        }
    };
    press_with(code, alt)
}

#[test]
fn every_key_the_table_advertises_dispatches() {
    // The hint bar, the palette and the help overlay are drawn from the
    // table, so a key that is in the table and not in the dispatcher would
    // be a key the screen teaches and the program does not have.
    for context in every_context() {
        for binding in bindings(context) {
            for (key, action) in binding.keys {
                assert_eq!(
                    action_for(&event_for(key), context),
                    Some(*action),
                    "{key:?} of {:?} in {context:?}",
                    binding.label
                );
            }
        }
    }
}

#[test]
fn a_key_means_what_its_context_says() {
    assert_eq!(
        action_for(&typing('q'), home(Pane::Day)),
        Some(Action::Quit)
    );
    assert_eq!(
        action_for(&typing('j'), home(Pane::Day)),
        Some(Action::Down)
    );
    assert_eq!(
        action_for(&typing('J'), home(Pane::Day)),
        Some(Action::MoveDown),
        "shift is a different key, not a modifier on j"
    );
    assert_eq!(
        action_for(&typing('b'), home(Pane::Day)),
        Some(Action::ToBacklog)
    );
    assert_eq!(
        action_for(&typing('d'), home(Pane::Backlog)),
        Some(Action::DueBy)
    );
    assert_eq!(
        action_for(
            &typing('d'),
            KeyContext::Review {
                step: ReviewStep::Pile,
                text_field: false
            }
        ),
        Some(Action::Close),
        "the same key, another meaning, because the context is another one"
    );
    assert_eq!(action_for(&typing('§'), home(Pane::Day)), None);
}

#[test]
fn the_arrow_keys_stand_in_for_j_and_k() {
    assert_eq!(
        action_for(&press(KeyCode::Down), home(Pane::Day)),
        Some(Action::Down)
    );
    assert_eq!(
        action_for(&press(KeyCode::Up), home(Pane::Backlog)),
        Some(Action::Up)
    );
}

#[test]
fn a_text_field_types_every_letter_and_digit() {
    let context = field(PopupKind::Palette);
    for typed in ['a', 'q', 'J', '7', ' ', '/', ':'] {
        assert_eq!(
            action_for(&typing(typed), context),
            Some(Action::Insert(typed)),
            "{typed:?} types rather than acting"
        );
    }
}

#[test]
fn a_text_field_keeps_enter_escape_tab_and_the_arrows() {
    let context = field(PopupKind::Palette);
    assert_eq!(
        action_for(&press(KeyCode::Enter), context),
        Some(Action::Confirm)
    );
    assert_eq!(
        action_for(&press(KeyCode::Esc), context),
        Some(Action::Cancel)
    );
    assert_eq!(
        action_for(&press(KeyCode::Down), context),
        Some(Action::Down)
    );
    assert_eq!(action_for(&press(KeyCode::Up), context), Some(Action::Up));
}

#[test]
fn a_text_field_has_the_editing_keys_without_the_key_table() {
    let context = field(PopupKind::Search);
    for (code, action) in [
        (KeyCode::Backspace, Action::Backspace),
        (KeyCode::Delete, Action::DeleteForward),
        (KeyCode::Left, Action::Left),
        (KeyCode::Right, Action::Right),
        (KeyCode::Home, Action::LineStart),
        (KeyCode::End, Action::LineEnd),
    ] {
        assert_eq!(action_for(&press(code), context), Some(action));
    }
    assert!(
        !bindings(context)
            .iter()
            .any(|binding| binding.label.contains("backspace")),
        "the editing keys are the field itself, never a row of the hint bar"
    );
}

#[test]
fn a_control_key_is_not_a_key_this_program_has() {
    assert_eq!(
        action_for(
            &press_with(KeyCode::Char('c'), KeyModifiers::CONTROL),
            home(Pane::Day)
        ),
        None
    );
    assert_eq!(
        action_for(
            &press_with(KeyCode::Char('c'), KeyModifiers::CONTROL),
            field(PopupKind::Search)
        ),
        None,
        "and it does not type either"
    );
}

#[test]
fn a_key_release_is_not_a_key_press() {
    let released = Event::Key(KeyEvent::new_with_kind_and_state(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
        KeyEventState::NONE,
    ));
    assert_eq!(action_for(&released, home(Pane::Day)), None);
}

#[test]
fn the_loop_events_are_actions_like_any_other() {
    assert_eq!(
        action_for(&Event::Resize(120, 36), home(Pane::Day)),
        Some(Action::Resize)
    );
    assert_eq!(
        action_for(&Event::FocusGained, home(Pane::Day)),
        Some(Action::FocusGained)
    );
    assert_eq!(action_for(&Event::FocusLost, home(Pane::Day)), None);
}

#[test]
fn the_mouse_carries_cell_coordinates_and_no_meaning() {
    let mouse = |kind| {
        Event::Mouse(MouseEvent {
            kind,
            column: 7,
            row: 9,
            modifiers: KeyModifiers::NONE,
        })
    };
    assert_eq!(
        action_for(
            &mouse(MouseEventKind::Down(MouseButton::Left)),
            home(Pane::Day)
        ),
        Some(Action::MouseDown { column: 7, row: 9 })
    );
    assert_eq!(
        action_for(
            &mouse(MouseEventKind::Drag(MouseButton::Left)),
            home(Pane::Day)
        ),
        Some(Action::MouseDrag { column: 7, row: 9 })
    );
    assert_eq!(
        action_for(&mouse(MouseEventKind::ScrollDown), home(Pane::Day)),
        Some(Action::Scroll {
            column: 7,
            row: 9,
            down: true
        })
    );
    assert_eq!(
        action_for(&mouse(MouseEventKind::Moved), home(Pane::Day)),
        None
    );
}

#[test]
fn the_hint_bar_shortens_its_names_when_the_window_is_narrow() {
    let day = bindings(home(Pane::Day));
    let delete = day
        .iter()
        .find(|binding| binding.shown == "x")
        .expect("a delete row");

    assert_eq!(delete.bar.slot(delete.label), Some((Side::Left, "delete")));
    assert_eq!(delete.narrow.slot(delete.label), Some((Side::Left, "del")));

    let help = day
        .iter()
        .find(|binding| binding.shown == "?")
        .expect("a help row");
    assert_eq!(help.bar.slot(help.label), None, "the status line has it");
    assert_eq!(help.narrow.slot(help.label), Some((Side::Right, "more")));
}

#[test]
fn the_in_place_field_types_and_keeps_only_enter_and_escape() {
    for field in [Field::Adding, Field::Renaming] {
        let context = writing(field);
        assert_eq!(
            action_for(&typing('j'), context),
            Some(Action::Insert('j')),
            "every letter types while a title is being written"
        );
        assert_eq!(
            action_for(&typing(' '), context),
            Some(Action::Insert(' ')),
            "space types rather than closing the task"
        );
        assert_eq!(
            action_for(&press(KeyCode::Enter), context),
            Some(Action::Confirm)
        );
        assert_eq!(
            action_for(&press(KeyCode::Esc), context),
            Some(Action::Cancel)
        );
    }
}

#[test]
fn adding_and_renaming_call_enter_different_things() {
    let name = |context| {
        bindings(context)
            .iter()
            .find(|binding| binding.shown == "⏎")
            .map(|binding| binding.label)
    };

    assert_eq!(name(writing(Field::Adding)), Some("add & keep typing"));
    assert_eq!(name(writing(Field::Renaming)), Some("save"));
}

#[test]
fn the_move_card_offers_a_day_under_every_key_it_names() {
    let card = bindings(popup(PopupKind::Move));
    let days: Vec<(&str, Action)> = card
        .iter()
        .filter_map(|binding| binding.keys.first().copied())
        .collect();

    assert!(days.contains(&("t", Action::ToToday)));
    assert!(days.contains(&("1", Action::Tomorrow)));
    assert!(days.contains(&("2", Action::NextWorkDay)));
    assert!(days.contains(&("3", Action::NextMonday)));
    assert!(days.contains(&("g", Action::GoToDate)));
    assert!(days.contains(&("b", Action::ToBacklog)));
}

#[test]
fn the_copy_question_has_a_key_for_each_answer_and_no_default() {
    let context = popup(PopupKind::CopyQuestion);

    assert_eq!(action_for(&typing('1'), context), Some(Action::ThisCopy));
    assert_eq!(
        action_for(&typing('2'), context),
        Some(Action::ThisAndFuture)
    );
    assert_eq!(
        action_for(&press(KeyCode::Enter), context),
        None,
        "there is no answer safe enough to be the one Enter picks"
    );
}
