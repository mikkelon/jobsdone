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
