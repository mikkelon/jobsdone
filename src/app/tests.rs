use super::*;

use jiff::civil::Date;

use crate::domain::tests::MemStore;
use crate::domain::{Change, Placement, Rule, Schedule, Task, Write};

/// The wireframes' own day, which is a Friday.
const NOW: &str = "2025-09-05T09:00:00+02:00[Europe/Copenhagen]";

fn at(text: &str) -> Zoned {
    text.parse().expect("a zoned timestamp")
}

fn on(text: &str) -> Date {
    text.parse().expect("a civil date")
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

fn started() -> App {
    app_at(MemStore::new(), NOW)
}

fn type_in(app: &mut App, text: &str) {
    for typed in text.chars() {
        app.update(Action::Insert(typed));
    }
}

/// `a`, the title, Enter, Escape: one task in the focused pane, the way
/// somebody adds one.
fn add(app: &mut App, title: &str) -> Id {
    app.update(Action::Add);
    type_in(app, title);
    app.update(Action::Confirm);
    app.update(Action::Cancel);
    app.cursor(app.focused()).expect("the task just added")
}

/// The titles of a list, in the order they are drawn.
fn titles(app: &App, list: List) -> Vec<String> {
    app.rows_of(list)
        .into_iter()
        .filter_map(|(id, _)| app.model().task(id).map(|task| task.title.clone()))
        .collect()
}

fn groups(app: &App, list: List) -> Vec<Group> {
    app.rows_of(list)
        .into_iter()
        .map(|(_, group)| group)
        .collect()
}

fn hint(app: &App) -> String {
    app.message()
        .map(|message| message.text.clone())
        .unwrap_or_default()
}

/// An app on a store that already holds a schedule and one copy of it on
/// today, which no key can make until phase 8.
fn with_a_recurring_copy() -> (App, Id) {
    let now = at(NOW);
    let today = domain::working_day(&now);
    let mut model = Model::empty();
    model.schedules.insert(
        1,
        Schedule {
            id: 1,
            title: "Write standup notes".to_owned(),
            rule: Rule::Workdays,
            generated_through: today,
            stopped_on: None,
            created_at: now.clone(),
        },
    );
    model.tasks.insert(
        1,
        Task {
            id: 1,
            title: "Write standup notes".to_owned(),
            day: Some(today),
            position: 0,
            focus: false,
            waiting: false,
            closed_at: None,
            due_on: None,
            remind_on: None,
            schedule_id: Some(1),
            scheduled_on: Some(today),
            created_at: now.clone(),
            deleted_at: None,
        },
    );
    model.placements.insert(
        (1, today),
        Placement {
            task_id: 1,
            day: today,
            placed_at: now,
            from_place: domain::FromPlace::New,
        },
    );
    (app_at(MemStore::holding(model), NOW), 1)
}

// ---- the launch ------------------------------------------------------

#[test]
fn launching_loads_the_model_and_the_working_day() {
    let mut model = Model::empty();
    model.meta.insert("review_on".into(), "2025-09-04".into());

    let app = app_at(
        MemStore::holding(model),
        "2025-09-05T01:30:00+02:00[Europe/Copenhagen]",
    );

    assert_eq!(
        app.model().meta.get("review_on").map(String::as_str),
        Some("2025-09-04")
    );
    assert_eq!(app.today().to_string(), "2025-09-04");
}

#[test]
fn q_quits_and_a_tick_does_not() {
    let mut app = started();

    assert_eq!(app.update(Action::Tick), Flow::Continue);
    assert_eq!(app.update(Action::Resize), Flow::Continue);
    assert_eq!(app.update(Action::Quit), Flow::Quit);
}

#[test]
fn a_tick_picks_up_what_another_window_did() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    assert!(app.model().meta.is_empty());

    let mut elsewhere = store;
    elsewhere
        .commit(&set_meta("review_on", "2025-09-05"))
        .expect("the in-memory store");

    app.update(Action::Tick);

    assert_eq!(
        app.model().meta.get("review_on").map(String::as_str),
        Some("2025-09-05")
    );
}

#[test]
fn a_command_reloads_before_it_asks_the_domain() {
    // Rule 7: what another window wrote is in the model before the
    // command that is about to be built sees it.
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    let mut elsewhere = app_at(store, NOW);
    elsewhere.update(Action::Add);
    type_in(&mut elsewhere, "Ship invoice export");
    elsewhere.update(Action::Confirm);

    add(&mut app, "Reply to the tender questions");

    assert_eq!(
        titles(&app, List::Day),
        ["Ship invoice export", "Reply to the tender questions"],
        "the other window's task is there, and the new one is after it"
    );
}

#[test]
fn the_review_count_is_the_size_of_the_pile() {
    let mut app = started();
    assert_eq!(app.review_count(), 0);

    // A task added today and then left behind by the clock.
    add(&mut app, "Chase the hosting invoice");
    let mut app = app_at(
        MemStore::holding(app.model().clone()),
        "2025-09-08T09:00:00+02:00[Europe/Copenhagen]",
    );

    assert_eq!(app.review_count(), 1);
    app.update(Action::Close);
    assert_eq!(app.review_count(), 1, "the pile is what is on past days");
}

// ---- adding and renaming ---------------------------------------------

#[test]
fn a_adds_to_the_pane_the_keyboard_is_on_and_keeps_the_field_open() {
    let mut app = started();
    app.update(Action::Add);
    assert_eq!(
        app.page_context(),
        KeyContext::Home {
            pane: Pane::Day,
            field: Some(Field::Adding)
        },
        "every letter types while the field is open"
    );

    type_in(&mut app, "Ship invoice export");
    app.update(Action::Confirm);

    assert_eq!(titles(&app, List::Day), ["Ship invoice export"]);
    assert_eq!(
        app.editor().map(|editor| editor.text.as_str()),
        Some(""),
        "the field is empty and still open, ready for the next one"
    );

    type_in(&mut app, "Reply to the tender questions");
    app.update(Action::Confirm);
    app.update(Action::Cancel);

    assert_eq!(
        titles(&app, List::Day),
        ["Ship invoice export", "Reply to the tender questions"]
    );
    assert!(app.editor().is_none(), "escape stops");
}

#[test]
fn adding_in_the_backlog_puts_the_task_in_the_backlog() {
    let mut app = started();
    app.update(Action::PaneRight);
    add(&mut app, "Clean out the garage");

    assert!(titles(&app, List::Day).is_empty());
    assert_eq!(titles(&app, List::Backlog), ["Clean out the garage"]);
}

#[test]
fn an_empty_field_is_how_quick_add_stops() {
    let mut app = started();
    app.update(Action::Add);
    app.update(Action::Confirm);

    assert!(app.editor().is_none());
    assert!(titles(&app, List::Day).is_empty());
    assert_eq!(hint(&app), "", "and it is not an error either");
}

#[test]
fn e_edits_the_title_in_place() {
    let mut app = started();
    add(&mut app, "Fix the migration test");

    app.update(Action::Edit);
    assert_eq!(
        app.editor().map(|editor| editor.text.as_str()),
        Some("Fix the migration test"),
        "the field starts as the title"
    );
    type_in(&mut app, " (CI only)");
    app.update(Action::Confirm);

    assert_eq!(
        titles(&app, List::Day),
        ["Fix the migration test (CI only)"]
    );
    assert!(app.editor().is_none(), "renaming closes the field");
}

#[test]
fn a_title_that_is_only_spaces_is_refused_and_the_field_stays_open() {
    let mut app = started();
    add(&mut app, "Book dentist");

    app.update(Action::Edit);
    for _ in 0..12 {
        app.update(Action::Backspace);
    }
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "A task needs a title.");
    assert!(app.editor().is_some(), "so that it can be typed again");
    assert_eq!(titles(&app, List::Day), ["Book dentist"]);
}

#[test]
fn renaming_a_recurring_copy_asks_the_one_question() {
    let (mut app, copy) = with_a_recurring_copy();
    app.update(Action::Edit);
    type_in(&mut app, " (long version)");
    app.update(Action::Confirm);

    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::CopyQuestion,
            text_field: false
        },
        "nothing is renamed until the question is answered"
    );
    assert_eq!(
        app.model().task(copy).map(|task| task.title.as_str()),
        Some("Write standup notes")
    );

    app.update(Action::ThisCopy);

    assert_eq!(
        app.model().task(copy).map(|task| task.title.as_str()),
        Some("Write standup notes (long version)")
    );
    assert_eq!(
        app.model()
            .schedule(1)
            .map(|schedule| schedule.title.as_str()),
        Some("Write standup notes"),
        "this copy only: the schedule keeps its title"
    );
}

#[test]
fn this_and_future_copies_renames_the_schedule_too() {
    let (mut app, copy) = with_a_recurring_copy();
    app.update(Action::Edit);
    type_in(&mut app, " (long version)");
    app.update(Action::Confirm);
    app.update(Action::ThisAndFuture);

    assert_eq!(
        app.model().task(copy).map(|task| task.title.as_str()),
        Some("Write standup notes (long version)")
    );
    assert_eq!(
        app.model()
            .schedule(1)
            .map(|schedule| schedule.title.as_str()),
        Some("Write standup notes (long version)")
    );
}

#[test]
fn escaping_the_question_renames_nothing() {
    let (mut app, copy) = with_a_recurring_copy();
    app.update(Action::Edit);
    type_in(&mut app, " (long version)");
    app.update(Action::Confirm);
    app.update(Action::Cancel);

    assert!(app.popup().is_none());
    assert_eq!(
        app.model().task(copy).map(|task| task.title.as_str()),
        Some("Write standup notes")
    );
}

// ---- working through the day -----------------------------------------

#[test]
fn space_closes_a_task_and_the_cursor_steps_to_the_next_one() {
    let mut app = started();
    add(&mut app, "Morning review");
    let next = add(&mut app, "Review Anna's PR");
    app.update(Action::Up);

    app.update(Action::Close);

    assert_eq!(groups(&app, List::Day), [Group::Plan, Group::Done]);
    assert_eq!(
        titles(&app, List::Day),
        ["Review Anna's PR", "Morning review"]
    );
    assert_eq!(app.cursor(List::Day), Some(next), "and steps down the plan");
    assert_eq!(hint(&app), "Closed \"Morning review\"");
    assert!(app.message().is_some_and(|message| message.undo));
}

#[test]
fn space_on_a_closed_task_reopens_it_at_the_end_of_the_plan() {
    let mut app = started();
    let first = add(&mut app, "Morning review");
    add(&mut app, "Review Anna's PR");
    app.update(Action::Up);
    app.update(Action::Close);

    // The cursor stepped on; go back to the closed row and reopen it.
    app.update(Action::Down);
    assert_eq!(app.cursor(List::Day), Some(first));
    app.update(Action::Close);

    assert_eq!(
        titles(&app, List::Day),
        ["Review Anna's PR", "Morning review"]
    );
    assert_eq!(groups(&app, List::Day), [Group::Plan, Group::Plan]);
    assert_eq!(app.cursor(List::Day), Some(first), "the cursor follows it");
}

#[test]
fn closing_a_backlog_task_puts_it_on_today_first() {
    let mut app = started();
    app.update(Action::PaneRight);
    add(&mut app, "Pay electricity bill");

    app.update(Action::Close);

    assert!(titles(&app, List::Backlog).is_empty());
    assert_eq!(titles(&app, List::Day), ["Pay electricity bill"]);
    assert_eq!(groups(&app, List::Day), [Group::Done]);
}

#[test]
fn f_marks_a_focus_item_and_unmarks_it() {
    let mut app = started();
    add(&mut app, "Ship invoice export");
    add(&mut app, "Review Anna's PR");
    app.update(Action::Up);

    app.update(Action::Focus);
    assert_eq!(groups(&app, List::Day), [Group::Focus, Group::Plan]);
    assert_eq!(
        titles(&app, List::Day),
        ["Ship invoice export", "Review Anna's PR"]
    );
    assert_eq!(hint(&app), "Focused \"Ship invoice export\"");

    app.update(Action::Focus);
    assert_eq!(groups(&app, List::Day), [Group::Plan, Group::Plan]);
}

#[test]
fn focus_is_for_tasks_on_a_day_and_the_hint_bar_says_so() {
    let mut app = started();
    app.update(Action::PaneRight);
    add(&mut app, "Clean out the garage");

    app.update(Action::Focus);

    assert_eq!(hint(&app), "Focus is for tasks on a day.");
    assert!(
        app.model().tasks.values().all(|task| !task.focus),
        "and nothing changed"
    );
}

#[test]
fn j_and_k_reorder_within_a_group() {
    let mut app = started();
    add(&mut app, "One");
    add(&mut app, "Two");
    let three = add(&mut app, "Three");

    app.update(Action::MoveUp);
    assert_eq!(titles(&app, List::Day), ["One", "Three", "Two"]);
    assert_eq!(
        app.cursor(List::Day),
        Some(three),
        "the cursor goes with it"
    );

    app.update(Action::MoveUp);
    assert_eq!(titles(&app, List::Day), ["Three", "One", "Two"]);
    app.update(Action::MoveUp);
    assert_eq!(
        titles(&app, List::Day),
        ["Three", "One", "Two"],
        "the top stops"
    );

    app.update(Action::MoveDown);
    assert_eq!(titles(&app, List::Day), ["One", "Three", "Two"]);
}

#[test]
fn a_focus_item_reorders_among_the_focus_items() {
    let mut app = started();
    let first = add(&mut app, "Ship invoice export");
    add(&mut app, "Fix the migration test");
    let last = add(&mut app, "Reply to the tender questions");

    app.update(Action::Focus);
    app.update(Action::Down);
    assert_eq!(app.cursor(List::Day), Some(first));
    app.update(Action::Focus);

    assert_eq!(
        titles(&app, List::Day),
        [
            "Ship invoice export",
            "Reply to the tender questions",
            "Fix the migration test"
        ]
    );
    app.update(Action::MoveDown);

    assert_eq!(
        titles(&app, List::Day),
        [
            "Reply to the tender questions",
            "Ship invoice export",
            "Fix the migration test"
        ],
        "it swapped with the other focus item, over the plan row between them"
    );
    assert_eq!(app.cursor(List::Day), Some(first));
    assert_eq!(
        groups(&app, List::Day),
        [Group::Focus, Group::Focus, Group::Plan]
    );

    let mut positions: Vec<usize> = [first, last]
        .iter()
        .chain(std::iter::once(&(last - 1)))
        .filter_map(|id| app.model().task(*id).map(|task| task.position))
        .collect();
    positions.sort_unstable();
    assert_eq!(positions, [0, 1, 2], "the positions underneath stay dense");
}

#[test]
fn the_done_group_keeps_the_order_it_was_closed_in() {
    let mut app = started();
    add(&mut app, "Morning review");
    app.update(Action::Close);
    add(&mut app, "Pay electricity bill");
    app.update(Action::Close);
    app.update(Action::Up);

    app.update(Action::MoveDown);

    assert_eq!(hint(&app), "Those rows keep the order they are in.");
    assert_eq!(
        titles(&app, List::Day),
        ["Morning review", "Pay electricity bill"]
    );
}

// ---- moving ----------------------------------------------------------

#[test]
fn t_pulls_a_backlog_task_onto_today() {
    let mut app = started();
    app.update(Action::PaneRight);
    add(&mut app, "Clean out the garage");
    add(&mut app, "Sort photo backups");
    app.update(Action::Up);

    app.update(Action::ToToday);

    assert_eq!(titles(&app, List::Day), ["Clean out the garage"]);
    assert_eq!(titles(&app, List::Backlog), ["Sort photo backups"]);
    assert_eq!(hint(&app), "Moved \"Clean out the garage\" to today");
}

#[test]
fn b_sends_a_task_to_the_backlog_and_leaves_a_pointer_behind() {
    let mut app = started();
    add(&mut app, "Chase the hosting invoice");

    app.update(Action::ToBacklog);

    assert_eq!(titles(&app, List::Backlog), ["Chase the hosting invoice"]);
    assert_eq!(
        groups(&app, List::Day),
        [Group::Moved],
        "the day keeps the record that it was planned"
    );
}

#[test]
fn a_moved_row_is_a_pointer_rather_than_a_task_to_act_on() {
    let mut app = started();
    add(&mut app, "Chase the hosting invoice");
    app.update(Action::ToBacklog);
    assert_eq!(groups(&app, List::Day), [Group::Moved]);

    app.update(Action::Delete);

    assert_eq!(
        hint(&app),
        "That row only points at the task; it has moved."
    );
    assert_eq!(titles(&app, List::Backlog), ["Chase the hosting invoice"]);
}

#[test]
fn the_move_card_offers_the_days_and_moves_to_the_one_chosen() {
    let mut app = started();
    add(&mut app, "Clean out the garage");

    app.update(Action::MoveToDay);
    let choices = app.move_choices();
    let days: Vec<MoveTarget> = choices.iter().map(|choice| choice.target).collect();

    assert_eq!(
        days,
        [
            MoveTarget::Day(on("2025-09-05")),
            MoveTarget::Day(on("2025-09-06")),
            MoveTarget::Day(on("2025-09-08")),
            MoveTarget::Day(on("2025-09-08")),
            MoveTarget::Pick,
            MoveTarget::Backlog,
        ],
        "today, tomorrow, the next work day, next Monday, a date, no day"
    );

    // The second row is tomorrow; Enter on it is the same as pressing 1.
    app.update(Action::Down);
    app.update(Action::Confirm);

    assert!(app.popup().is_none());
    assert_eq!(groups(&app, List::Day), [Group::Moved]);
    assert_eq!(hint(&app), "Moved \"Clean out the garage\" to Sat 6 Sep");
}

#[test]
fn the_move_card_acts_on_the_row_it_was_opened_on() {
    let mut app = started();
    let first = add(&mut app, "Clean out the garage");
    add(&mut app, "Sort photo backups");
    app.update(Action::Up);

    app.update(Action::MoveToDay);
    app.update(Action::ToBacklog);

    assert_eq!(titles(&app, List::Backlog), ["Clean out the garage"]);
    assert_eq!(app.model().task(first).and_then(|task| task.day), None);
}

// ---- delete and undo -------------------------------------------------

#[test]
fn x_deletes_without_asking_and_u_puts_it_back() {
    let mut app = started();
    add(&mut app, "Book dentist");
    let kept = add(&mut app, "Review Anna's PR");
    app.update(Action::Up);

    app.update(Action::Delete);
    assert_eq!(titles(&app, List::Day), ["Review Anna's PR"]);
    assert_eq!(hint(&app), "Deleted \"Book dentist\"");
    assert!(app.message().is_some_and(|message| message.undo));
    assert_eq!(app.cursor(List::Day), Some(kept));

    app.update(Action::Undo);

    assert_eq!(
        titles(&app, List::Day),
        ["Book dentist", "Review Anna's PR"]
    );
    assert_eq!(hint(&app), "Undone: Deleted \"Book dentist\"");
    assert!(!app.message().is_some_and(|message| message.undo));
}

#[test]
fn undo_walks_back_through_the_day() {
    let mut app = started();
    add(&mut app, "Ship invoice export");
    app.update(Action::Focus);
    app.update(Action::Close);

    app.update(Action::Undo);
    assert_eq!(groups(&app, List::Day), [Group::Focus]);
    app.update(Action::Undo);
    assert_eq!(groups(&app, List::Day), [Group::Plan]);
    app.update(Action::Undo);
    assert!(titles(&app, List::Day).is_empty());

    app.update(Action::Undo);
    assert_eq!(hint(&app), "There is nothing to undo.");
}

// ---- the hint bar ----------------------------------------------------

#[test]
fn what_the_hint_bar_says_stands_until_the_next_key() {
    let mut app = started();
    add(&mut app, "Book dentist");
    app.update(Action::Delete);
    assert!(app.message().is_some());

    app.update(Action::Tick);
    assert!(app.message().is_some(), "a tick is not a key");

    app.update(Action::Down);
    assert!(app.message().is_none());
}

#[test]
fn a_key_a_later_phase_owns_says_so_and_does_nothing() {
    let mut app = started();
    add(&mut app, "Clean out the garage");

    for (action, said) in [
        (Action::DueBy, "Due dates and reminders are not built yet."),
        (Action::Waiting, "Waiting is not built yet."),
        (Action::Repeat, "The repeat card is not built yet."),
        (Action::PrevDay, "Stepping through days is not built yet."),
        (Action::GoToDate, "The date card is not built yet."),
    ] {
        app.update(action);
        assert_eq!(hint(&app), said);
    }
    assert_eq!(titles(&app, List::Day), ["Clean out the garage"]);
}

#[test]
fn a_key_on_an_empty_pane_says_there_is_nothing_there() {
    let mut app = started();
    app.update(Action::Close);

    assert_eq!(hint(&app), "There is no task here yet.");
}

// ---- moving about ----------------------------------------------------

#[test]
fn the_cursor_walks_the_list_by_id_and_stops_at_both_ends() {
    let mut app = started();
    for title in ["One", "Two", "Three"] {
        add(&mut app, title);
    }
    let ids: Vec<Id> = app
        .rows_of(List::Day)
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    app.update(Action::Up);
    app.update(Action::Up);
    app.update(Action::Up);
    assert_eq!(app.cursor(List::Day), Some(ids[0]), "the top does not wrap");

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
    add(&mut app, "One");
    add(&mut app, "Two");
    app.update(Action::Up);
    let day = app.cursor(List::Day);

    app.update(Action::PaneRight);
    assert_eq!(app.pane(), Pane::Backlog);
    add(&mut app, "In the backlog");

    app.update(Action::PaneLeft);
    assert_eq!(app.pane(), Pane::Day);
    assert_eq!(app.cursor(List::Day), day, "coming back lands where it was");
}

#[test]
fn a_cursor_on_a_row_another_window_took_away_clamps_to_the_first() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    add(&mut app, "One");
    let second = add(&mut app, "Two");
    assert_eq!(app.cursor(List::Day), Some(second));

    let mut elsewhere = app_at(store, NOW);
    elsewhere.update(Action::Down);
    elsewhere.update(Action::Delete);
    app.update(Action::Tick);

    assert_eq!(titles(&app, List::Day), ["One"]);
    assert_eq!(
        app.cursor(List::Day),
        app.rows_of(List::Day).first().map(|(id, _)| *id)
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
fn a_field_holds_the_keyboard_until_it_is_answered() {
    let mut app = started();
    app.update(Action::Add);
    app.update(Action::NextPane);

    assert_eq!(
        app.pane(),
        Pane::Day,
        "tab does not leave a half-typed task"
    );
    assert!(app.editor().is_some());
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
fn the_notes_page_says_it_is_not_built_yet() {
    let mut app = started();
    app.update(Action::NotesPage);
    app.update(Action::Add);

    assert_eq!(hint(&app), "The notes page is not built yet.");
    assert!(app.editor().is_none());
}

// ---- popups ----------------------------------------------------------

#[test]
fn the_hint_bar_has_a_context_to_draw_from() {
    let app = started();

    assert_eq!(
        app.key_context(),
        KeyContext::Home {
            pane: Pane::Day,
            field: None
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
            field: None
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
    add(&mut app, "Ship invoice export");
    add(&mut app, "Book dentist");
    app.update(Action::Close);

    app.update(Action::Search);
    type_in(&mut app, "invoice");
    let found = app.search_results();

    assert_eq!(found.total, 1);
    assert_eq!(found.open[0].title, "Ship invoice export");

    type_in(&mut app, " and then some");
    assert_eq!(app.search_results().total, 0);
}

#[test]
fn a_search_with_nothing_in_it_offers_to_add_what_was_typed() {
    let mut app = started();
    app.update(Action::Search);
    type_in(&mut app, "tax return");
    app.update(Action::Confirm);

    assert!(app.popup().is_none());
    assert_eq!(titles(&app, List::Day), ["tax return"]);
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
    add(&mut app, "One");
    add(&mut app, "Two");
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

// ---- the mouse -------------------------------------------------------

#[test]
fn a_click_outside_any_row_still_moves_the_keyboard_to_that_pane() {
    let mut app = started();
    app.update(Action::PaneRight);
    add(&mut app, "Clean out the garage");
    app.update(Action::PaneLeft);
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
    add(&mut app, "One");
    add(&mut app, "Two");
    app.update(Action::Up);
    let first = app.cursor(List::Day);

    app.update(Action::Scroll {
        column: 4,
        row: 6,
        down: true,
    });

    assert_ne!(app.cursor(List::Day), first);
}
