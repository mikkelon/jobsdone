use super::*;

use crossterm::event::{KeyEventState, MouseButton};

fn home(pane: Pane) -> KeyContext {
    KeyContext::Home {
        pane,
        day: Shown::Today,
        field: None,
        narrow: false,
    }
}

/// The same pane of a window collapsed to tabs.
fn tab(pane: Pane) -> KeyContext {
    KeyContext::Home {
        pane,
        day: Shown::Today,
        field: None,
        narrow: true,
    }
}

/// The same two panes with the day pane stepped off today.
fn browsing(pane: Pane) -> KeyContext {
    KeyContext::Home {
        pane,
        day: Shown::Past,
        field: None,
        narrow: false,
    }
}

fn writing(field: Field) -> KeyContext {
    KeyContext::Home {
        pane: Pane::Day,
        day: Shown::Today,
        field: Some(field),
        narrow: false,
    }
}

/// The notes page, with the keyboard on the list, in the note or in the
/// filter.
fn notes(pane: NotesPane) -> KeyContext {
    KeyContext::Notes {
        pane,
        list: NotesList::Stack,
        text_field: pane != NotesPane::List,
        narrow: false,
    }
}

fn archive(narrow: bool) -> KeyContext {
    KeyContext::Notes {
        pane: NotesPane::List,
        list: NotesList::Archive,
        text_field: false,
        narrow,
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
        tab(Pane::Day),
        tab(Pane::Backlog),
        notes(NotesPane::List),
        archive(false),
        archive(true),
        notes(NotesPane::Note),
        notes(NotesPane::Filter),
        KeyContext::Review {
            last: false,
            step: ReviewStep::Pile,
            asks: true,
            text_field: false,
        },
        KeyContext::Review {
            last: false,
            step: ReviewStep::Surfaced,
            asks: true,
            text_field: false,
        },
        KeyContext::Review {
            last: false,
            step: ReviewStep::Pile,
            asks: true,
            text_field: true,
        },
        KeyContext::Review {
            last: false,
            step: ReviewStep::Surfaced,
            asks: false,
            text_field: false,
        },
        writing(Field::Adding),
        writing(Field::Renaming),
        field(PopupKind::Palette),
        field(PopupKind::Search),
        popup(PopupKind::Help),
        popup(PopupKind::Move),
        popup(PopupKind::CopyQuestion),
        popup(PopupKind::DeleteQuestion),
        field(PopupKind::Date),
        popup(PopupKind::Date),
        popup(PopupKind::Repeat),
        popup(PopupKind::Spelling),
        popup(PopupKind::Dictionary),
        field(PopupKind::Dictionary),
        KeyContext::Settings { field: false },
        KeyContext::Settings { field: true },
        KeyContext::Leader { over: None },
        KeyContext::Leader {
            over: Some(PopupKind::Move),
        },
        KeyContext::Leader {
            over: Some(PopupKind::Repeat),
        },
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
    // Ordinary lists share navigation keys; review has additional tests below.
    let lists = [
        home(Pane::Day),
        home(Pane::Backlog),
        browsing(Pane::Day),
        browsing(Pane::Backlog),
        notes(NotesPane::List),
        popup(PopupKind::Move),
        popup(PopupKind::Repeat),
        popup(PopupKind::Spelling),
        popup(PopupKind::Dictionary),
        popup(PopupKind::Help),
        KeyContext::Settings { field: false },
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
            last: false,
            step,
            asks: true,
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

/// A step whose rows are all information asks nothing, so an outcome
/// there would answer for a row nobody was asked about (DESIGN.md
/// section 5).
#[test]
fn the_step_that_asks_nothing_offers_no_outcome() {
    let context = KeyContext::Review {
        last: false,
        step: ReviewStep::Surfaced,
        asks: false,
        text_field: false,
    };

    let named: Vec<&str> = bindings(context)
        .iter()
        .filter(|binding| binding.bar.slot(binding.label).is_some())
        .map(|binding| binding.label)
        .collect();
    assert_eq!(named, ["start the day", "skip for now"]);

    for outcome in [
        KeyCode::Char('t'),
        KeyCode::Char('s'),
        KeyCode::Char('d'),
        KeyCode::Char('w'),
        KeyCode::Char(' '),
    ] {
        assert_eq!(
            action_for(&press(outcome), context),
            None,
            "{outcome:?} answers a step that asked nothing"
        );
    }
    assert_eq!(
        action_for(&press(KeyCode::Enter), context),
        Some(Action::Confirm),
        "and the one thing to press is still there"
    );
}

#[test]
fn a_title_typed_on_a_review_row_saves_with_enter() {
    let context = KeyContext::Review {
        last: false,
        step: ReviewStep::Pile,
        asks: true,
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
    if name == "shift-tab" {
        return press_with(KeyCode::BackTab, KeyModifiers::SHIFT);
    }
    let (name, alt) = match name.strip_prefix("alt-") {
        Some(rest) => (rest, KeyModifiers::ALT),
        None => (name, KeyModifiers::NONE),
    };
    let (name, alt) = match name.strip_prefix("ctrl-") {
        Some(rest) => (rest, KeyModifiers::CONTROL),
        None => (name, alt),
    };
    let (name, alt) = match name.strip_prefix("shift-") {
        Some(rest) => (rest, KeyModifiers::SHIFT),
        None => (name, alt),
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
            &press(KeyCode::Char(' ')),
            KeyContext::Review {
                last: false,
                step: ReviewStep::Pile,
                asks: true,
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
fn the_arrow_keys_do_what_h_j_k_and_l_do_wherever_those_are_keys() {
    let pairs = [("h", "left"), ("j", "down"), ("k", "up"), ("l", "right")];
    for context in every_context() {
        if context.text_field() {
            continue;
        }
        for (letter, arrow) in pairs {
            assert_eq!(
                bound(context, letter),
                bound(context, arrow),
                "{context:?} does not give {letter:?} and {arrow:?} the same meaning"
            );
        }
    }
}

#[test]
fn the_arrow_keys_switch_pane_as_h_and_l_do() {
    for context in [home(Pane::Day), tab(Pane::Backlog), notes(NotesPane::List)] {
        assert_eq!(
            action_for(&press(KeyCode::Left), context),
            Some(Action::PaneLeft)
        );
        assert_eq!(
            action_for(&press(KeyCode::Right), context),
            Some(Action::PaneRight)
        );
    }
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
            &press_with(KeyCode::Char('t'), KeyModifiers::CONTROL),
            home(Pane::Day)
        ),
        None
    );
    assert_eq!(
        action_for(
            &press_with(KeyCode::Char('t'), KeyModifiers::CONTROL),
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

    assert_eq!(name(writing(Field::Adding)), Some("add"));
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
    assert!(days.contains(&("w", Action::NextWorkDay)));
    assert!(days.contains(&("g", Action::Go)));
    assert!(days.contains(&("b", Action::ToBacklog)));
}

#[test]
fn a_digit_is_the_same_day_on_the_move_card_and_the_date_card() {
    let digits = |context: KeyContext, prefix: &str| -> Vec<(String, Action)> {
        bindings(context)
            .iter()
            .flat_map(|binding| binding.keys)
            .filter_map(|(key, action)| {
                let digit = key.strip_prefix(prefix)?;
                (digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit()))
                    .then(|| (digit.to_owned(), *action))
            })
            .collect()
    };
    let mut on_the_move_card = digits(popup(PopupKind::Move), "");
    // The date card also takes a date off with its zero, which a move
    // has nothing to match.
    let mut on_the_date_card: Vec<_> = digits(field(PopupKind::Date), "alt-")
        .into_iter()
        .filter(|(digit, _)| digit != "0")
        .collect();
    on_the_move_card.sort_by(|a, b| a.0.cmp(&b.0));
    on_the_date_card.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(on_the_move_card, on_the_date_card);
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

#[test]
fn the_delete_question_takes_enter_for_yes_and_esc_for_no() {
    let context = popup(PopupKind::DeleteQuestion);

    assert_eq!(
        action_for(&press(KeyCode::Enter), context),
        Some(Action::Confirm)
    );
    assert_eq!(
        action_for(&press(KeyCode::Esc), context),
        Some(Action::Cancel)
    );
    assert_eq!(
        action_for(&typing('x'), context),
        None,
        "the key that asked the question cannot answer it"
    );
}

/// The backlog is ordered by hand, so the keys that reorder a day reorder
/// it too: dragging a row was the only way (F2).
#[test]
fn the_backlog_reorders_by_keyboard_as_a_day_does() {
    assert_eq!(
        action_for(&typing('J'), home(Pane::Backlog)),
        Some(Action::MoveDown)
    );
    assert_eq!(
        action_for(&typing('K'), home(Pane::Backlog)),
        Some(Action::MoveUp)
    );
}

/// Ctrl-C belongs to no context: it left the program running everywhere,
/// including on the home list where `q` quits (F4).
#[test]
fn ctrl_c_quits_from_every_context() {
    for context in every_context() {
        assert_eq!(
            action_for(
                &press_with(KeyCode::Char('c'), KeyModifiers::CONTROL),
                context
            ),
            Some(Action::Quit),
            "{context:?}"
        );
    }
}

#[test]
fn clipboard_keys_edit_text_without_turning_shifted_copy_into_quit() {
    for context in every_context() {
        let expected = context.text_field();
        for (code, modifiers, action) in [
            (
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                Action::CopySelection,
            ),
            (
                KeyCode::Char('C'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                Action::CopySelection,
            ),
            (
                KeyCode::Insert,
                KeyModifiers::CONTROL,
                Action::CopySelection,
            ),
            (
                KeyCode::Char('x'),
                KeyModifiers::CONTROL,
                Action::CutSelection,
            ),
            (
                KeyCode::Char('X'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                Action::CutSelection,
            ),
            (
                KeyCode::Char('x'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                Action::CutSelection,
            ),
            (KeyCode::Char('v'), KeyModifiers::CONTROL, Action::Paste),
            (
                KeyCode::Char('v'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                Action::Paste,
            ),
            (
                KeyCode::Char('V'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                Action::Paste,
            ),
            (KeyCode::Insert, KeyModifiers::SHIFT, Action::Paste),
        ] {
            assert_eq!(
                action_for(&press_with(code, modifiers), context),
                expected.then_some(action),
                "{context:?} {code:?} {modifiers:?}"
            );
        }
    }
}

/// A pile task leaves the pile only by being closed, moved or deleted
/// (PRODUCT.md), and the panel there lists exactly those. `k` marked a
/// pile row kept and stepped on, which the panel never offered.
#[test]
fn keep_is_the_surfaced_steps_word_and_not_the_piles() {
    let pile = KeyContext::Review {
        last: false,
        step: ReviewStep::Pile,
        asks: true,
        text_field: false,
    };
    let surfaced = KeyContext::Review {
        last: false,
        step: ReviewStep::Surfaced,
        asks: true,
        text_field: false,
    };
    assert_eq!(action_for(&typing('k'), pile), Some(Action::Up));
    assert_eq!(action_for(&typing('s'), pile), None);
    assert_eq!(action_for(&typing('k'), surfaced), Some(Action::Up));
    assert_eq!(action_for(&typing('s'), surfaced), Some(Action::Keep));
}

#[test]
fn notes_yank_without_intercepting_plain_y_in_the_editor() {
    let list = notes(NotesPane::List);
    let editor = notes(NotesPane::Note);
    assert_eq!(action_for(&typing('y'), list), Some(Action::CopyNote));
    assert_eq!(action_for(&typing('y'), editor), Some(Action::Insert('y')));
    for context in [list, editor] {
        assert_eq!(
            action_for(&press_with(KeyCode::Char('y'), KeyModifiers::ALT), context),
            Some(Action::CopyNote)
        );
    }
    assert_eq!(
        action_for(
            &press_with(KeyCode::Char('y'), KeyModifiers::ALT),
            home(Pane::Day)
        ),
        None
    );
}

#[test]
fn y_copies_a_task_wherever_the_rows_are_tasks() {
    for context in [home(Pane::Day), home(Pane::Backlog), browsing(Pane::Day)] {
        assert_eq!(
            action_for(&typing('y'), context),
            Some(Action::CopyTask),
            "{context:?}"
        );
    }
    assert_eq!(action_for(&typing('y'), browsing(Pane::Backlog)), None);
    assert_eq!(
        action_for(&typing('y'), writing(Field::Adding)),
        Some(Action::Insert('y'))
    );
}

#[test]
fn the_open_note_offers_alt_s_where_plain_s_types() {
    let list = notes(NotesPane::List);
    let editor = notes(NotesPane::Note);
    let alt_s = press_with(KeyCode::Char('s'), KeyModifiers::ALT);

    assert_eq!(
        action_for(&alt_s, editor),
        Some(Action::FixSpelling),
        "the word under the caret is what the key is about, so it is the \
         open note that binds it"
    );
    assert_eq!(
        action_for(&typing('s'), editor),
        Some(Action::Insert('s')),
        "and the letter itself still types"
    );
    assert_eq!(
        action_for(&alt_s, list),
        None,
        "the list has no caret and so no word to fix"
    );
    assert_eq!(action_for(&alt_s, home(Pane::Day)), None);
}

#[test]
fn the_open_note_names_alt_s_in_its_hint_bar_at_both_widths() {
    let editor = notes(NotesPane::Note);
    let row = bindings(editor)
        .iter()
        .find(|binding| {
            binding
                .keys
                .iter()
                .any(|(_, action)| *action == Action::FixSpelling)
        })
        .expect("the row that fixes a spelling");

    assert_eq!(row.shown, "alt-s", "the key is written as it is pressed");
    for bar in [row.bar, row.narrow] {
        let (side, name) = bar.slot(row.label).expect("a slot in the bar");
        assert_eq!(side, Side::Left, "beside the note's own keys");
        assert!(!name.is_empty());
    }
}

#[test]
fn the_dictionary_is_a_list_with_the_three_keys_a_list_has() {
    let manager = popup(PopupKind::Dictionary);
    for (key, action) in [
        ("a", Action::Add),
        ("e", Action::Edit),
        ("x", Action::Delete),
        ("enter", Action::Confirm),
        ("esc", Action::Cancel),
        ("j", Action::Down),
        ("k", Action::Up),
        ("up", Action::Up),
        ("down", Action::Down),
    ] {
        assert_eq!(
            action_for(&event_for(key), manager),
            Some(action),
            "{key:?}"
        );
    }
}

#[test]
fn a_word_being_written_types_its_own_x_and_removes_nothing() {
    let writing = field(PopupKind::Dictionary);
    // The one key that would take a word away is a letter of the word
    // being typed, so nothing goes while one is being written.
    assert_eq!(action_for(&typing('x'), writing), Some(Action::Insert('x')));
    for typed in ['a', 'e', 'q', ' '] {
        assert_eq!(
            action_for(&typing(typed), writing),
            Some(Action::Insert(typed)),
            "{typed:?}"
        );
    }
    assert_eq!(
        action_for(&event_for("enter"), writing),
        Some(Action::Confirm)
    );
    assert_eq!(action_for(&event_for("esc"), writing), Some(Action::Cancel));
}

#[test]
fn the_spelling_card_is_walked_and_answered_and_left() {
    let card = popup(PopupKind::Spelling);
    for (key, action) in [
        ("up", Action::Up),
        ("down", Action::Down),
        ("enter", Action::Confirm),
        ("esc", Action::Cancel),
    ] {
        assert_eq!(action_for(&event_for(key), card), Some(action), "{key:?}");
    }
    // A card is answered with the keys of a card, not with the letters
    // the note under it types.
    assert_eq!(action_for(&typing('s'), card), None);
    assert_eq!(
        action_for(&press_with(KeyCode::Char('s'), KeyModifiers::ALT), card),
        None
    );
}

#[test]
fn control_word_shortcuts_work_in_every_text_context_only() {
    for context in every_context() {
        for (code, action) in [
            (KeyCode::Left, Action::WordLeft),
            (KeyCode::Right, Action::WordRight),
            (KeyCode::Backspace, Action::DeleteWordBackward),
            (KeyCode::Char('h'), Action::DeleteWordBackward),
        ] {
            assert_eq!(
                action_for(&press_with(code, KeyModifiers::CONTROL), context),
                context.text_field().then_some(action),
                "{context:?}"
            );
            assert_eq!(
                action_for(
                    &press_with(code, KeyModifiers::CONTROL | KeyModifiers::ALT),
                    context
                ),
                None
            );
        }
    }
}

#[test]
fn shifted_navigation_selects_in_every_text_context() {
    let contexts = [
        writing(Field::Adding),
        writing(Field::Renaming),
        field(PopupKind::Search),
        field(PopupKind::Palette),
        field(PopupKind::Date),
        field(PopupKind::Dictionary),
        KeyContext::Settings { field: true },
        notes(NotesPane::Note),
        KeyContext::Review {
            last: false,
            step: ReviewStep::Pile,
            asks: true,
            text_field: true,
        },
    ];
    for context in contexts {
        for (code, modifiers, action) in [
            (KeyCode::Left, KeyModifiers::SHIFT, Action::SelectLeft),
            (KeyCode::Right, KeyModifiers::SHIFT, Action::SelectRight),
            (KeyCode::Up, KeyModifiers::SHIFT, Action::SelectUp),
            (KeyCode::Down, KeyModifiers::SHIFT, Action::SelectDown),
            (KeyCode::Home, KeyModifiers::SHIFT, Action::SelectStart),
            (KeyCode::End, KeyModifiers::SHIFT, Action::SelectEnd),
            (
                KeyCode::Left,
                KeyModifiers::SHIFT | KeyModifiers::CONTROL,
                Action::SelectWordLeft,
            ),
            (
                KeyCode::Right,
                KeyModifiers::SHIFT | KeyModifiers::CONTROL,
                Action::SelectWordRight,
            ),
            (KeyCode::Char('a'), KeyModifiers::CONTROL, Action::SelectAll),
        ] {
            assert_eq!(
                action_for(&Event::Key(KeyEvent::new(code, modifiers)), context),
                Some(action)
            );
        }
    }
}

#[test]
fn shift_enter_only_keeps_adding_in_the_add_field() {
    let shifted = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(
        action_for(&shifted, writing(Field::Adding)),
        Some(Action::AddAndContinue)
    );
    assert_eq!(
        action_for(&press(KeyCode::Enter), writing(Field::Adding)),
        Some(Action::Confirm)
    );
    assert_eq!(
        action_for(&shifted, writing(Field::Renaming)),
        Some(Action::Confirm)
    );
    assert_ne!(
        action_for(&shifted, home(Pane::Day)),
        Some(Action::AddAndContinue)
    );
}

#[test]
fn tab_h_and_l_switch_panes_at_every_width() {
    let tab_key = press(KeyCode::Tab);
    for pane in [Pane::Day, Pane::Backlog] {
        // A narrow window has the same panes and shows the focused one,
        // so a key means the same at both widths.
        for context in [home(pane), tab(pane)] {
            assert_eq!(action_for(&typing('h'), context), Some(Action::PaneLeft));
            assert_eq!(action_for(&typing('l'), context), Some(Action::PaneRight));
            assert_eq!(action_for(&tab_key, context), Some(Action::NextPane));
            assert_eq!(
                action_for(&press_with(KeyCode::BackTab, KeyModifiers::SHIFT), context),
                None,
                "the home page's panes have no tabs"
            );
        }
    }
}

#[test]
fn the_notes_list_archives_with_a_and_switches_tab_with_shift_tab() {
    for context in [notes(NotesPane::List), archive(false), archive(true)] {
        assert_eq!(action_for(&typing('A'), context), Some(Action::Archive));
        assert_eq!(
            action_for(&press_with(KeyCode::BackTab, KeyModifiers::SHIFT), context),
            Some(Action::NextTab)
        );
        assert_eq!(
            action_for(&press(KeyCode::Tab), context),
            Some(Action::NextPane),
            "tab is the other pane, the scratchpad"
        );
        assert_eq!(action_for(&typing('l'), context), Some(Action::PaneRight));
        assert_eq!(action_for(&typing('h'), context), Some(Action::PaneLeft));
    }
    let label = |context| {
        bindings(context)
            .iter()
            .find(|binding| binding.keys.iter().any(|(_, a)| *a == Action::Archive))
            .map(|binding| binding.label)
    };
    assert_eq!(label(notes(NotesPane::List)), Some("archive note"));
    assert_eq!(label(archive(false)), Some("unarchive note"));
    assert_eq!(name(notes(NotesPane::List)), "STACK");
    assert_eq!(name(archive(false)), "ARCHIVE");
    assert_eq!(name(notes(NotesPane::Note)), "SCRATCHPAD");
    assert_eq!(
        action_for(&press(KeyCode::Tab), notes(NotesPane::Note)),
        Some(Action::NextPane),
        "an open note is left with tab, back to the list, as well as esc"
    );
}

/// `/` filters the notes page's own list, and is search everywhere
/// else.
#[test]
fn slash_filters_the_notes_page_and_searches_the_rest() {
    assert_eq!(
        action_for(&typing('/'), notes(NotesPane::List)),
        Some(Action::Filter)
    );
    assert_eq!(
        action_for(&typing('/'), archive(false)),
        Some(Action::Filter)
    );
    for context in [home(Pane::Day), home(Pane::Backlog), browsing(Pane::Day)] {
        assert_eq!(action_for(&typing('/'), context), Some(Action::Search));
    }
}

#[test]
fn the_filter_types_every_letter_and_keeps_its_few_keys() {
    let filter = notes(NotesPane::Filter);
    assert!(filter.text_field());
    assert_eq!(name(filter), "FILTER");
    for letter in ['A', 'x', 'j', '/', 'n', 'q'] {
        assert_eq!(
            action_for(&typing(letter), filter),
            Some(Action::Insert(letter))
        );
    }
    assert_eq!(
        action_for(&press(KeyCode::Down), filter),
        Some(Action::Down)
    );
    assert_eq!(action_for(&press(KeyCode::Up), filter), Some(Action::Up));
    assert_eq!(
        action_for(&press(KeyCode::Enter), filter),
        Some(Action::Confirm)
    );
    assert_eq!(
        action_for(&press(KeyCode::Esc), filter),
        Some(Action::Cancel)
    );
    assert_eq!(
        action_for(&press(KeyCode::Tab), filter),
        Some(Action::NextPane),
        "tab moves focus to the list, the next control"
    );
}

#[test]
fn one_action_has_one_label_on_every_page() {
    // What a key is called is learnt once. Confirm, Cancel and the keys
    // that move between controls are named for where they lead, which is
    // different on every page, and `A` names which way it archives.
    let named_for_where_it_leads = [
        Action::Confirm,
        Action::Cancel,
        Action::NextTab,
        Action::NextPane,
        Action::Left,
        Action::Right,
        Action::Pick,
        Action::Archive,
        // The key that opens a page is also the way back off it.
        Action::NotesPage,
        Action::SettingsPage,
    ];
    let mut seen: Vec<(Action, &str, KeyContext)> = Vec::new();
    let pages = every_context()
        .into_iter()
        .filter(|context| !context.text_field() && !matches!(context, KeyContext::Popup { .. }));
    for context in pages {
        for binding in bindings(context) {
            let Some((_, action)) = binding.keys.first() else {
                continue;
            };
            if named_for_where_it_leads.contains(action) {
                continue;
            }
            match seen.iter().find(|(other, _, _)| other == action) {
                Some((_, label, first)) => assert_eq!(
                    *label, binding.label,
                    "{action:?} is {label:?} in {first:?} and {:?} in {context:?}",
                    binding.label
                ),
                None => seen.push((*action, binding.label, context)),
            }
        }
    }
}

#[test]
fn g_leads_somewhere_from_every_page_and_every_list() {
    let places = every_context().into_iter().filter(|context| {
        !context.text_field()
            && !matches!(context, KeyContext::Leader { .. })
            && !matches!(
                context,
                KeyContext::Popup {
                    kind: PopupKind::Date | PopupKind::CopyQuestion | PopupKind::DeleteQuestion,
                    ..
                }
            )
    });
    for context in places {
        assert_eq!(
            action_for(&typing('g'), context),
            Some(Action::Go),
            "{context:?}"
        );
        assert_eq!(
            action_for(&typing('G'), context),
            Some(Action::Last),
            "{context:?}"
        );
        assert_eq!(
            action_for(
                &press_with(KeyCode::Char('d'), KeyModifiers::CONTROL),
                context
            ),
            Some(Action::HalfPageDown),
            "{context:?}"
        );
    }
}

#[test]
fn after_g_every_place_is_one_key_and_escape_goes_nowhere() {
    let leader = KeyContext::Leader { over: None };
    for (key, place) in [
        ('g', Action::First),
        ('t', Action::Today),
        ('b', Action::BacklogPane),
        ('d', Action::GoToDate),
        ('n', Action::NotesPage),
        ('a', Action::ArchivePage),
        ('s', Action::SettingsPage),
        ('r', Action::OpenReview),
    ] {
        assert_eq!(action_for(&typing(key), leader), Some(place), "g{key}");
    }
    assert_eq!(
        action_for(&press(KeyCode::Esc), leader),
        Some(Action::Cancel)
    );
    assert_eq!(action_for(&typing('z'), leader), None, "gz is no place");

    // In a card, the first row is the one place, and on the move card a
    // date as well.
    let move_card = KeyContext::Leader {
        over: Some(PopupKind::Move),
    };
    assert_eq!(action_for(&typing('g'), move_card), Some(Action::First));
    assert_eq!(action_for(&typing('d'), move_card), Some(Action::GoToDate));
    assert_eq!(action_for(&typing('n'), move_card), None);
}

#[test]
fn no_place_has_a_key_of_its_own_on_any_page() {
    let places = [
        Action::Today,
        Action::GoToDate,
        Action::NotesPage,
        Action::ArchivePage,
        Action::SettingsPage,
        Action::OpenReview,
        Action::BacklogPane,
    ];
    for context in every_context() {
        if matches!(context, KeyContext::Leader { .. }) {
            continue;
        }
        for binding in bindings(context) {
            for (key, action) in binding.keys {
                assert!(
                    !places.contains(action),
                    "{key:?} goes to {action:?} in {context:?}; places are under g"
                );
            }
        }
    }
}

#[test]
fn the_palette_and_search_page_with_ctrl_d_and_ctrl_u() {
    for kind in [PopupKind::Palette, PopupKind::Search] {
        assert_eq!(
            action_for(
                &press_with(KeyCode::Char('u'), KeyModifiers::CONTROL),
                field(kind)
            ),
            Some(Action::HalfPageUp)
        );
    }
}
