use super::*;

use crate::domain::tests::MemStore;
use crate::domain::{Change, Write};

fn at(text: &str) -> Zoned {
    text.parse().expect("a zoned timestamp")
}

fn set_meta(key: &str, value: &str) -> Change {
    Change {
        writes: vec![Write::SetMeta {
            key: key.to_owned(),
            value: value.to_owned(),
        }],
    }
}

fn app_at(store: MemStore, now: &str) -> App {
    App::new(Box::new(store), &at(now)).expect("an app")
}

#[test]
fn launching_loads_the_model_and_the_working_day() {
    let mut model = Model::empty();
    model.meta.insert("review_on".into(), "2026-09-04".into());

    let app = app_at(
        MemStore::holding(model),
        "2026-09-05T01:30:00+02:00[Europe/Copenhagen]",
    );

    assert_eq!(
        app.model().meta.get("review_on").map(String::as_str),
        Some("2026-09-04")
    );
    assert_eq!(app.today().to_string(), "2026-09-04");
}

#[test]
fn q_quits_and_a_tick_does_not() {
    let mut app = app_at(
        MemStore::new(),
        "2026-09-05T09:00:00+02:00[Europe/Copenhagen]",
    );

    assert_eq!(app.update(Action::Tick), Flow::Continue);
    assert_eq!(app.update(Action::Resize), Flow::Continue);
    assert_eq!(app.update(Action::Quit), Flow::Quit);
}

#[test]
fn a_tick_picks_up_what_another_window_did() {
    let store = MemStore::new();
    let mut app = app_at(
        store.clone(),
        "2026-09-05T09:00:00+02:00[Europe/Copenhagen]",
    );
    assert!(app.model().meta.is_empty());

    let mut elsewhere = store;
    elsewhere
        .commit(&set_meta("review_on", "2026-09-05"))
        .unwrap();

    app.update(Action::Tick);

    assert_eq!(
        app.model().meta.get("review_on").map(String::as_str),
        Some("2026-09-05")
    );
}

#[test]
fn the_hint_bar_has_a_context_to_draw_from() {
    let app = app_at(
        MemStore::new(),
        "2026-09-05T09:00:00+02:00[Europe/Copenhagen]",
    );

    assert_eq!(
        app.key_context(),
        KeyContext::Home {
            pane: Pane::Day,
            text_field: false
        }
    );
}

fn started() -> App {
    app_at(
        MemStore::new(),
        "2026-09-05T09:00:00+02:00[Europe/Copenhagen]",
    )
}

fn type_in(app: &mut App, text: &str) {
    for typed in text.chars() {
        app.update(Action::Insert(typed));
    }
}

#[test]
fn the_cursor_walks_the_list_by_id_and_stops_at_both_ends() {
    let mut app = started();
    let ids: Vec<_> = app.view(List::Day).rows().map(|row| row.id).collect();

    assert_eq!(app.cursor(List::Day), Some(ids[0]));
    app.update(Action::Up);
    assert_eq!(app.cursor(List::Day), Some(ids[0]), "the top does not wrap");

    app.update(Action::Down);
    app.update(Action::Down);
    assert_eq!(app.cursor(List::Day), Some(ids[2]));

    for _ in 0..50 {
        app.update(Action::Down);
    }
    assert_eq!(
        app.cursor(List::Day),
        ids.last().copied(),
        "and neither does the bottom"
    );
}

#[test]
fn each_pane_keeps_its_own_cursor() {
    let mut app = started();
    app.update(Action::Down);
    let day = app.cursor(List::Day);

    app.update(Action::PaneRight);
    assert_eq!(app.pane(), Pane::Backlog);
    app.update(Action::Down);
    app.update(Action::Down);

    app.update(Action::PaneLeft);
    assert_eq!(app.pane(), Pane::Day);
    assert_eq!(app.cursor(List::Day), day, "coming back lands where it was");
}

#[test]
fn a_cursor_on_a_row_that_has_gone_clamps_to_the_first() {
    let mut app = started();
    app.update(Action::Down);
    assert_ne!(
        app.cursor(List::Day),
        app.view(List::Day).rows().next().map(|row| row.id)
    );

    // The empty fixture stands in for another window having emptied the
    // list under the cursor.
    app.show_empty();
    assert_eq!(app.cursor(List::Day), None);
    assert_eq!(
        app.cursor(List::Notes),
        app.view(List::Notes).rows().next().map(|row| row.id)
    );
}

#[test]
fn h_and_l_stop_at_the_ends_and_tab_goes_round() {
    let mut app = started();
    app.update(Action::PaneLeft);
    assert_eq!(app.pane(), Pane::Day, "already at the left");

    app.update(Action::NextPane);
    assert_eq!(app.pane(), Pane::Backlog);
    app.update(Action::NextPane);
    assert_eq!(app.pane(), Pane::Day, "tab wraps");

    app.update(Action::PaneRight);
    app.update(Action::PaneRight);
    assert_eq!(app.pane(), Pane::Backlog, "l does not");
}

#[test]
fn notes_is_the_third_tab_only_when_the_window_is_narrow() {
    let mut app = started();
    app.update(Action::PaneRight);
    app.update(Action::PaneRight);
    assert_eq!(app.page(), Page::Home, "wide, the notes page is a page");

    app.set_layout(Layout {
        narrow: true,
        ..Layout::default()
    });
    app.update(Action::PaneRight);
    assert_eq!(app.page(), Page::Notes);
    app.update(Action::PaneLeft);
    assert_eq!(app.page(), Page::Home);
    assert_eq!(app.pane(), Pane::Backlog);
}

#[test]
fn n_turns_the_page_and_turns_it_back() {
    let mut app = started();
    app.update(Action::NotesPage);

    assert_eq!(app.page(), Page::Notes);
    assert_eq!(
        app.key_context(),
        KeyContext::Notes {
            pane: NotesPane::List,
            text_field: false
        }
    );

    app.update(Action::NotesPage);
    assert_eq!(app.page(), Page::Home);
}

#[test]
fn an_open_note_is_a_text_field_and_the_list_beside_it_is_not() {
    let mut app = started();
    app.update(Action::NotesPage);
    assert!(!app.key_context().text_field());

    app.update(Action::PaneRight);
    assert_eq!(
        app.key_context(),
        KeyContext::Notes {
            pane: NotesPane::Note,
            text_field: true
        }
    );
}

#[test]
fn a_popup_takes_the_keyboard_and_escape_gives_it_back() {
    let mut app = started();
    app.update(Action::Commands);

    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::Palette,
            text_field: true
        }
    );
    assert_eq!(
        app.page_context(),
        KeyContext::Home {
            pane: Pane::Day,
            text_field: false
        },
        "the page underneath is what the palette lists"
    );

    app.update(Action::Cancel);
    assert!(app.popup().is_none());
    assert_eq!(app.key_context(), app.page_context());
}

#[test]
fn the_help_overlay_is_read_rather_than_typed_in() {
    let mut app = started();
    app.update(Action::Help);

    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::Help,
            text_field: false
        }
    );
}

#[test]
fn typing_in_a_field_narrows_the_palette() {
    let mut app = started();
    app.update(Action::Commands);
    let all = app.palette_rows().len();

    type_in(&mut app, "mo");
    let some = app.palette_rows();
    assert!(some.len() < all);
    assert!(
        some.iter().all(|row| row.label.contains("mo")),
        "every row still names what was typed: {some:?}"
    );

    type_in(&mut app, "ve to day");
    assert_eq!(app.palette_rows().len(), 1);
    assert_eq!(app.palette_rows()[0].shown, "m");

    type_in(&mut app, "zzz");
    assert!(app.palette_rows().is_empty());
}

#[test]
fn the_palette_never_offers_a_row_that_is_not_a_key() {
    let mut app = started();
    app.update(Action::Search);

    // "type to filter" is a line of the hint bar, not a command.
    assert!(app.palette_rows().iter().all(|row| !row.keys.is_empty()));
}

#[test]
fn a_field_edits_where_the_caret_is() {
    let mut app = started();
    app.update(Action::Search);
    type_in(&mut app, "invoce");

    app.update(Action::Left);
    app.update(Action::Left);
    app.update(Action::Insert('i'));
    assert_eq!(
        app.popup().map(|popup| popup.text.as_str()),
        Some("invoice")
    );

    app.update(Action::LineEnd);
    app.update(Action::Backspace);
    assert_eq!(app.popup().map(|popup| popup.text.as_str()), Some("invoic"));

    app.update(Action::LineStart);
    app.update(Action::DeleteForward);
    assert_eq!(app.popup().map(|popup| popup.text.as_str()), Some("nvoic"));
    assert_eq!(app.popup().map(|popup| popup.caret), Some(0));
}

#[test]
fn the_search_field_finds_what_was_typed() {
    let mut app = started();
    app.update(Action::Search);
    assert_eq!(
        app.search_results().count(),
        9,
        "an empty filter matches all"
    );

    type_in(&mut app, "Nordic");
    let found = app.search_results();
    assert_eq!(found.count(), 1);
    assert_eq!(found.closed[0].title, "Send the invoice to Nordic Ltd");

    type_in(&mut app, " and then some");
    assert!(app.search_results().is_empty());
}

#[test]
fn enter_in_the_palette_runs_the_row_it_is_on() {
    let mut app = started();
    app.update(Action::Commands);
    type_in(&mut app, "notes page");
    assert_eq!(app.palette_rows().len(), 1);

    app.update(Action::Confirm);

    assert!(app.popup().is_none(), "the palette closes");
    assert_eq!(app.page(), Page::Notes, "and the command runs");
}

#[test]
fn the_palette_can_quit_the_program() {
    let mut app = started();
    app.update(Action::Commands);
    type_in(&mut app, "quit");

    assert_eq!(app.update(Action::Confirm), Flow::Quit);
}

#[test]
fn the_arrows_move_the_selection_inside_a_popup_not_the_pane() {
    let mut app = started();
    let cursor = app.cursor(List::Day);
    app.update(Action::Commands);

    app.update(Action::Down);
    app.update(Action::Down);
    assert_eq!(app.popup().map(|popup| popup.selected), Some(2));
    assert_eq!(app.cursor(List::Day), cursor, "the pane did not move");

    for _ in 0..99 {
        app.update(Action::Down);
    }
    let last = app.palette_rows().len() - 1;
    assert_eq!(app.popup().map(|popup| popup.selected), Some(last));
}

#[test]
fn typing_puts_the_selection_back_at_the_top() {
    let mut app = started();
    app.update(Action::Commands);
    app.update(Action::Down);
    app.update(Action::Down);

    app.update(Action::Insert('a'));
    assert_eq!(app.popup().map(|popup| popup.selected), Some(0));
}

#[test]
fn a_click_outside_any_row_still_moves_the_keyboard_to_that_pane() {
    let mut app = started();
    app.set_layout(Layout {
        narrow: false,
        lists: vec![ListArea {
            list: List::Backlog,
            area: Rect {
                x: 60,
                y: 5,
                width: 60,
                height: 28,
            },
        }],
        rows: Vec::new(),
    });
    let cursor = app.cursor(List::Backlog);

    app.update(Action::MouseDown {
        column: 70,
        row: 30,
    });

    assert_eq!(app.pane(), Pane::Backlog);
    assert_eq!(app.cursor(List::Backlog), cursor);
}

#[test]
fn the_wheel_moves_the_cursor() {
    let mut app = started();
    let first = app.cursor(List::Day);

    app.update(Action::Scroll {
        column: 4,
        row: 6,
        down: true,
    });

    assert_ne!(app.cursor(List::Day), first);
}
