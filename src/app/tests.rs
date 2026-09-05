use super::*;

use jiff::civil::Date;

use crate::domain::tests::MemStore;
use crate::domain::{Change, Placement, Rule, Schedule, Task, Write};

/// A store that refuses to write, so that a failed commit can be seen to
/// leave the model as it was (ARCHITECTURE.md rule 10).
struct Broken(MemStore);

impl Store for Broken {
    fn load(&self) -> Result<Model, StoreError> {
        self.0.load()
    }

    fn commit(&mut self, _change: &Change) -> Result<(), StoreError> {
        Err(StoreError::Other("the disk is full".to_owned()))
    }

    fn version(&self) -> Result<u64, StoreError> {
        self.0.version()
    }
}

/// The wireframes' own day, which is a Friday.
const NOW: &str = "2025-09-05T09:00:00+02:00[Europe/Copenhagen]";
const NOW_DAY: &str = "2025-09-05";

fn at(text: &str) -> Zoned {
    text.parse().expect("a zoned timestamp")
}

fn on(text: &str) -> Date {
    text.parse().expect("a civil date")
}

/// A model whose review has already run today, so that a launch lands on
/// the home page rather than in the review.
fn reviewed(mut model: Model) -> Model {
    model.meta.insert(
        "review_on".to_owned(),
        domain::working_day(&at(NOW)).to_string(),
    );
    model
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
    cursor(app, app.focused()).expect("the task just added")
}

/// The task the cursor is on, which is what these tests mean by it.
fn cursor(app: &App, list: List) -> Option<Id> {
    app.cursor(list).and_then(RowId::task)
}

/// The note the cursor is on, on the notes page.
fn note_cursor(app: &App) -> Option<Id> {
    app.cursor(List::Notes).and_then(RowId::note)
}

/// The titles of a list, in the order they are drawn.
fn titles(app: &App, list: List) -> Vec<String> {
    app.rows_of(list)
        .into_iter()
        .filter_map(|(id, _)| app.model().task(id.task()?).map(|task| task.title.clone()))
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
    (app_at(MemStore::holding(reviewed(model)), NOW), 1)
}

/// A store holding one schedule and nothing else, generated through the
/// date given, which is what being away since then looks like.
fn a_schedule_through(through: &str) -> MemStore {
    let mut model = Model::empty();
    model.schedules.insert(
        1,
        Schedule {
            id: 1,
            title: "Write standup notes".to_owned(),
            rule: Rule::Workdays,
            generated_through: on(through),
            stopped_on: None,
            created_at: at(NOW),
        },
    );
    MemStore::holding(reviewed(model))
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
fn launching_makes_a_copy_for_every_scheduled_date_since_the_last_one() {
    // Away since Tuesday: Wednesday and Thursday land on the pile and
    // Friday on today's plan. There is no cap, and the pile is where the
    // cost of being away is meant to be seen (DOMAIN.md section 10).
    let app = app_at(a_schedule_through("2025-09-02"), NOW);

    assert_eq!(titles(&app, List::Day), ["Write standup notes"]);
    assert_eq!(app.review_count(), 2);
    assert_eq!(
        app.model()
            .schedule(1)
            .map(|schedule| schedule.generated_through.to_string()),
        Some("2025-09-05".to_owned())
    );
}

#[test]
fn generation_is_not_a_user_action_and_cannot_be_undone() {
    let mut app = app_at(a_schedule_through("2025-09-04"), NOW);
    assert_eq!(titles(&app, List::Day), ["Write standup notes"]);

    app.update(Action::Undo);

    assert_eq!(hint(&app), "There is nothing to undo.");
    assert_eq!(titles(&app, List::Day), ["Write standup notes"]);
}

#[test]
fn a_window_left_open_over_the_night_is_owed_todays_copies() {
    let mut app = app_at(a_schedule_through("2025-09-04"), NOW);
    assert_eq!(titles(&app, List::Day), ["Write standup notes"]);

    app.clock = at("2025-09-08T09:00:00+02:00[Europe/Copenhagen]");
    app.update(Action::Tick);

    assert_eq!(app.today().to_string(), "2025-09-08");
    assert_eq!(
        titles(&app, List::Day),
        ["Write standup notes"],
        "Monday's copy, made without a launch"
    );
    assert_eq!(app.review_count(), 1, "Friday's is on the pile");
}

#[test]
fn a_second_window_generating_at_the_same_moment_makes_no_second_copy() {
    let store = a_schedule_through("2025-09-04");
    let first = app_at(store.clone(), NOW);
    let second = app_at(store, NOW);

    assert_eq!(titles(&first, List::Day), ["Write standup notes"]);
    assert_eq!(titles(&second, List::Day), ["Write standup notes"]);
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
    // The launch opens the review over the pile; past it, today is empty
    // and there is nothing here for `space` to close.
    app.update(Action::Cancel);
    app.update(Action::Close);
    assert_eq!(app.review_count(), 1, "the pile is what is on past days");
}

// ---- the morning review ----------------------------------------------

/// A store holding the tasks given, each on the day given, and nothing
/// else: the pile a review opens on.
fn left_behind(tasks: &[(&str, &str)]) -> MemStore {
    let mut model = Model::empty();
    for (nth, (title, day)) in tasks.iter().enumerate() {
        model.tasks.insert(
            nth as Id + 1,
            Task {
                id: nth as Id + 1,
                title: (*title).to_owned(),
                day: Some(on(day)),
                position: 0,
                focus: false,
                waiting: false,
                closed_at: None,
                due_on: None,
                remind_on: None,
                schedule_id: None,
                scheduled_on: None,
                created_at: at(NOW),
                deleted_at: None,
            },
        );
    }
    MemStore::holding(model)
}

/// A backlog task whose due date has arrived, which is the whole of the
/// second step.
fn something_surfaced() -> MemStore {
    let mut model = Model::empty();
    model.tasks.insert(
        1,
        Task {
            id: 1,
            title: "Migrate CI to the new runners".to_owned(),
            day: None,
            position: 0,
            focus: false,
            waiting: false,
            closed_at: None,
            due_on: Some(on("2025-09-03")),
            remind_on: None,
            schedule_id: None,
            scheduled_on: None,
            created_at: at(NOW),
            deleted_at: None,
        },
    );
    MemStore::holding(model)
}

fn step(app: &App) -> Option<ReviewStep> {
    app.review().map(Review::step)
}

#[test]
fn the_launch_opens_the_review_over_the_pile_and_writes_the_gate() {
    let app = app_at(
        left_behind(&[("Order new office chair", "2025-09-01")]),
        NOW,
    );

    assert_eq!(step(&app), Some(ReviewStep::Pile));
    assert_eq!(app.focused(), List::Review);
    assert_eq!(
        app.key_context(),
        KeyContext::Review {
            step: ReviewStep::Pile,
            asks: true,
            text_field: false
        }
    );
    assert_eq!(
        app.model().meta.get("review_on").map(String::as_str),
        Some("2025-09-05")
    );
}

#[test]
fn the_review_is_not_shown_twice_in_one_day() {
    let store = left_behind(&[("Order new office chair", "2025-09-01")]);
    let first = app_at(store.clone(), NOW);
    assert!(first.review().is_some());

    // A second window the same morning goes straight to today, and the
    // pile is still counted in red.
    let second = app_at(store, NOW);

    assert!(second.review().is_none());
    assert_eq!(second.review_count(), 1);
}

#[test]
fn a_review_with_nothing_in_it_is_not_shown_at_all() {
    let app = started();

    assert!(app.review().is_none());
    // The gate is not written either, so tomorrow's review is the first.
    assert_eq!(app.model().meta.get("review_on"), None);
}

#[test]
fn an_empty_step_is_skipped() {
    // Nothing on the pile, so the review opens on its second step and
    // says so.
    let app = app_at(something_surfaced(), NOW);

    assert_eq!(step(&app), Some(ReviewStep::Surfaced));
    assert_eq!(app.review().map(Review::steps), Some((1, 1)));
}

#[test]
fn enter_walks_the_steps_and_then_starts_the_day() {
    let mut app = app_at(
        left_behind(&[("Order new office chair", "2025-09-01")]),
        NOW,
    );
    assert_eq!(app.review().map(Review::steps), Some((1, 1)));

    // Sending the pile task back to the backlog with an old due date on
    // it makes a second step: the date prompts again (DOMAIN.md 8).
    app.update(Action::ToBacklog);
    assert_eq!(app.review().map(Review::steps), Some((1, 1)));

    app.update(Action::Confirm);
    assert!(app.review().is_none(), "one step, so Enter starts the day");
}

#[test]
fn a_date_unearthed_on_the_pile_makes_the_second_step() {
    let mut store = left_behind(&[("Order new office chair", "2025-09-01")]);
    let mut model = store.load().expect("the store");
    model.tasks.entry(1).and_modify(|task| {
        task.due_on = Some(on("2025-09-02"));
    });
    store = MemStore::holding(model);
    let mut app = app_at(store, NOW);

    // On a day it is not surfaced from, since only a backlog task does.
    assert_eq!(app.review().map(Review::steps), Some((1, 1)));
    app.update(Action::ToBacklog);
    assert_eq!(app.review().map(Review::steps), Some((1, 2)));

    app.update(Action::Confirm);
    assert_eq!(step(&app), Some(ReviewStep::Surfaced));
    assert_eq!(app.review().map(Review::steps), Some((2, 2)));
    app.update(Action::Confirm);
    assert!(app.review().is_none());
}

#[test]
fn escape_leaves_the_review_with_the_pile_intact() {
    let mut app = app_at(
        left_behind(&[
            ("Order new office chair", "2025-09-01"),
            ("Book the team dinner", "2025-08-12"),
        ]),
        NOW,
    );

    app.update(Action::Cancel);

    assert!(app.review().is_none());
    assert_eq!(app.focused(), List::Day);
    assert_eq!(app.review_count(), 2, "the pile is untouched");
}

#[test]
fn a_row_the_review_has_answered_stays_where_it_was() {
    let mut app = app_at(
        left_behind(&[
            ("Send the invoice", "2025-09-04"),
            ("Prepare slides", "2025-09-04"),
        ]),
        NOW,
    );
    assert_eq!(
        titles(&app, List::Review),
        ["Send the invoice", "Prepare slides"]
    );

    app.update(Action::Close);

    // Off the pile, still on the screen, and the cursor has stepped on.
    assert_eq!(app.review_count(), 1);
    assert_eq!(
        titles(&app, List::Review),
        ["Send the invoice", "Prepare slides"]
    );
    assert_eq!(cursor(&app, List::Review), Some(2));
    let review = app.review().expect("the review");
    assert_eq!(review.decision(1), Some(Decided::Done));
    assert_eq!(review.progress(), (1, 2));
}

#[test]
fn every_decision_the_pile_offers_answers_a_row() {
    let mut app = app_at(
        left_behind(&[
            ("One", "2025-09-04"),
            ("Two", "2025-09-04"),
            ("Three", "2025-09-04"),
            ("Four", "2025-09-04"),
        ]),
        NOW,
    );

    app.update(Action::Close);
    app.update(Action::ToToday);
    app.update(Action::ToBacklog);
    app.update(Action::Delete);

    let review = app.review().expect("the review");
    assert_eq!(review.decision(1), Some(Decided::Done));
    assert_eq!(
        review.decision(2),
        Some(Decided::Moved(Place::Day(on(NOW_DAY))))
    );
    assert_eq!(review.decision(3), Some(Decided::Moved(Place::Backlog)));
    assert_eq!(review.decision(4), Some(Decided::Deleted));
    assert_eq!(review.progress(), (4, 4));
    assert_eq!(app.review_count(), 0);
}

#[test]
fn the_cursor_walks_on_to_the_next_row_still_to_be_dealt_with() {
    let mut app = app_at(
        left_behind(&[
            ("One", "2025-09-04"),
            ("Two", "2025-09-04"),
            ("Three", "2025-09-04"),
        ]),
        NOW,
    );

    // Out of order, and the cursor finds what is left either way.
    app.update(Action::Down);
    app.update(Action::Close);
    assert_eq!(cursor(&app, List::Review), Some(3));
    app.update(Action::Close);
    assert_eq!(cursor(&app, List::Review), Some(1), "round to the first");
    app.update(Action::Close);
    assert_eq!(cursor(&app, List::Review), Some(1), "nothing left to find");
}

#[test]
fn keeping_a_surfaced_task_decides_it_and_changes_nothing() {
    let mut app = app_at(something_surfaced(), NOW);
    assert_eq!(step(&app), Some(ReviewStep::Surfaced));

    app.update(Action::Keep);

    let review = app.review().expect("the review");
    assert_eq!(review.decision(1), Some(Decided::Kept));
    assert_eq!(review.progress(), (1, 1));
    assert_eq!(app.model().task(1).and_then(|task| task.day), None);
    assert!(app.model().undo.is_empty(), "keeping pushes nothing");
}

#[test]
fn undo_takes_the_last_decision_back_with_the_change() {
    let mut app = app_at(left_behind(&[("Send the invoice", "2025-09-04")]), NOW);
    app.update(Action::Close);
    assert_eq!(
        app.review().and_then(|review| review.decision(1)),
        Some(Decided::Done)
    );

    app.update(Action::Undo);

    assert_eq!(app.review().map(Review::progress), Some((0, 1)));
    assert_eq!(app.review_count(), 1);
}

#[test]
fn a_copy_starting_today_is_shown_but_not_asked_about() {
    let (app, copy) = with_a_recurring_copy();
    let mut model = app.model().clone();
    // The review has not run yet on the morning the copy was made.
    model.meta.remove("review_on");
    let mut app = app_at(MemStore::holding(model), NOW);
    assert_eq!(step(&app), Some(ReviewStep::Surfaced));

    // It is a row of the step and the cursor reaches it, but the step
    // asks nothing about it (DOMAIN.md section 13).
    assert_eq!(titles(&app, List::Review), ["Write standup notes"]);
    assert_eq!(app.review().map(Review::progress), Some((0, 0)));
    app.update(Action::Keep);
    assert_eq!(app.review().map(Review::progress), Some((0, 0)));
    assert!(app.model().task(copy).is_some());
}

#[test]
fn a_title_is_edited_in_place_in_the_review() {
    let mut app = app_at(left_behind(&[("Send the invocie", "2025-09-04")]), NOW);

    app.update(Action::Edit);
    assert_eq!(
        app.page_context(),
        KeyContext::Review {
            step: ReviewStep::Pile,
            asks: true,
            text_field: true
        }
    );
    for _ in 0..7 {
        app.update(Action::Backspace);
    }
    type_in(&mut app, "invoice");
    app.update(Action::Confirm);

    assert_eq!(titles(&app, List::Review), ["Send the invoice"]);
    assert!(
        app.review().is_some(),
        "Enter saved the title, not the step"
    );
    assert_eq!(app.review().map(Review::progress), Some((0, 1)));
}

#[test]
fn an_interrupted_review_is_picked_up_where_the_pile_now_stands() {
    let mut app = app_at(
        left_behind(&[
            ("One", "2025-09-04"),
            ("Two", "2025-09-04"),
            ("Three", "2025-09-04"),
        ]),
        NOW,
    );
    app.update(Action::Close);
    app.update(Action::Cancel);
    assert!(app.review().is_none());
    assert_eq!(app.review_count(), 2);

    app.update(Action::OpenReview);

    // The gate has been written today, so this is not the launch's
    // review; what is left of the pile is what it opens on.
    assert_eq!(step(&app), Some(ReviewStep::Pile));
    assert_eq!(titles(&app, List::Review), ["Two", "Three"]);
    assert_eq!(app.review().map(Review::progress), Some((0, 2)));
}

#[test]
fn the_review_comes_back_to_today_to_open() {
    let (mut app, _) = with_a_past_day();
    app.update(Action::PrevDay);
    assert!(app.browsing());

    app.update(Action::OpenReview);

    assert_eq!(step(&app), Some(ReviewStep::Pile));
    assert!(!app.browsing(), "the review is about today");
    app.update(Action::Cancel);
    assert_eq!(app.showing(), app.today());
}

#[test]
fn the_review_will_not_open_on_nothing() {
    let mut app = started();

    app.update(Action::OpenReview);

    assert!(app.review().is_none());
    assert_eq!(
        hint(&app),
        "Nothing to review: the pile is empty and nothing is due."
    );
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
            day: Shown::Today,
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
    assert_eq!(
        cursor(&app, List::Day),
        Some(next),
        "and steps down the plan"
    );
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
    assert_eq!(cursor(&app, List::Day), Some(first));
    app.update(Action::Close);

    assert_eq!(
        titles(&app, List::Day),
        ["Review Anna's PR", "Morning review"]
    );
    assert_eq!(groups(&app, List::Day), [Group::Plan, Group::Plan]);
    assert_eq!(
        cursor(&app, List::Day),
        Some(first),
        "the cursor follows it"
    );
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
        cursor(&app, List::Day),
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

/// The cursor is one row id per list, so a day pane that has been on
/// another day holds an id today has not got. The row that answers for
/// it must not change under the reorder it asked for.
#[test]
fn a_reorder_follows_its_task_after_the_day_pane_has_been_elsewhere() {
    let mut app = started();
    let alpha = add(&mut app, "Alpha");
    add(&mut app, "Beta");

    // A task added on tomorrow leaves the day cursor on a row today
    // does not hold, so today answers with its first row instead.
    app.update(Action::NextDay);
    add(&mut app, "Gamma");
    app.update(Action::PrevDay);
    assert_eq!(cursor(&app, List::Day), Some(alpha));

    app.update(Action::MoveDown);
    assert_eq!(titles(&app, List::Day), ["Beta", "Alpha"]);
    assert_eq!(
        cursor(&app, List::Day),
        Some(alpha),
        "the cursor goes with it"
    );

    app.update(Action::MoveUp);
    assert_eq!(
        titles(&app, List::Day),
        ["Alpha", "Beta"],
        "and the way back is the key that came"
    );
}

/// The same with a schedule's copy in the plan, which is the shape the
/// acceptance test found it in.
#[test]
fn a_reorder_follows_its_task_past_a_copy_of_a_schedule() {
    let (mut app, _copy) = with_a_recurring_copy();
    let dentist = add(&mut app, "Book dentist");
    app.update(Action::MoveUp);
    assert_eq!(
        titles(&app, List::Day),
        ["Book dentist", "Write standup notes"]
    );

    app.update(Action::NextDay);
    add(&mut app, "Collect the parcel");
    app.update(Action::PrevDay);

    app.update(Action::MoveDown);
    assert_eq!(
        titles(&app, List::Day),
        ["Write standup notes", "Book dentist"]
    );
    assert_eq!(
        cursor(&app, List::Day),
        Some(dentist),
        "the cursor goes with it"
    );

    app.update(Action::MoveUp);
    assert_eq!(
        titles(&app, List::Day),
        ["Book dentist", "Write standup notes"],
        "and the way back is the key that came"
    );
}

#[test]
fn a_focus_item_reorders_among_the_focus_items() {
    let mut app = started();
    let first = add(&mut app, "Ship invoice export");
    add(&mut app, "Fix the migration test");
    let last = add(&mut app, "Reply to the tender questions");

    app.update(Action::Focus);
    app.update(Action::Down);
    assert_eq!(cursor(&app, List::Day), Some(first));
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
    assert_eq!(cursor(&app, List::Day), Some(first));
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

// ---- the repeat card -------------------------------------------------

fn rule_of(app: &App, schedule: Id) -> Option<Rule> {
    app.model()
        .schedule(schedule)
        .map(|schedule| schedule.rule.clone())
}

#[test]
fn r_gives_a_task_a_repeat_and_the_task_is_the_first_copy() {
    let mut app = started();
    let task = add(&mut app, "Write standup notes");

    app.update(Action::Repeat);
    app.update(Action::EveryWorkDay);
    app.update(Action::Confirm);

    assert!(app.popup().is_none());
    assert_eq!(hint(&app), "Set \"Write standup notes\" to repeat");
    assert_eq!(rule_of(&app, 1), Some(Rule::Workdays));
    assert_eq!(
        app.model().task(task).and_then(|task| task.scheduled_on),
        Some(on("2025-09-05")),
        "the task it was made from is the first copy"
    );
    assert_eq!(app.backlog().schedules.len(), 1, "and the backlog lists it");
}

#[test]
fn the_weekly_shape_is_a_set_of_days_that_space_picks() {
    let mut app = started();
    add(&mut app, "Water the plants");

    app.update(Action::Repeat);
    app.update(Action::EveryWeek);
    // The card opens on the weekday of the day the task is on, Friday.
    // Left four times is Monday; space adds it, and Thursday too.
    for _ in 0..4 {
        app.update(Action::Left);
    }
    app.update(Action::Pick);
    for _ in 0..3 {
        app.update(Action::Right);
    }
    app.update(Action::Pick);
    app.update(Action::Confirm);

    assert_eq!(
        rule_of(&app, 1),
        Some(Rule::Weekly {
            weekdays: vec![Weekday::Mon, Weekday::Thu, Weekday::Fri]
        })
    );
}

#[test]
fn a_repeat_with_no_day_in_it_is_refused_and_the_card_stays_open() {
    let mut app = started();
    add(&mut app, "Water the plants");

    app.update(Action::Repeat);
    app.update(Action::EveryWeek);
    // Friday is the only day in the set, and space takes it out again.
    app.update(Action::Pick);
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "That repeat never comes round.");
    assert!(app.popup().is_some(), "the card stays open to be fixed");
    assert!(app.model().schedules.is_empty());
}

#[test]
fn the_card_previews_the_dates_the_rule_falls_on() {
    let mut app = started();
    add(&mut app, "Pay rent");

    app.update(Action::Repeat);
    app.update(Action::EveryMonth);
    // The 5th of the month, which is the day the task is on.
    assert_eq!(
        app.repeat_preview()
            .iter()
            .map(|date| date.to_string())
            .collect::<Vec<_>>(),
        ["2025-10-05", "2025-11-05", "2025-12-05"]
    );

    app.update(Action::Left);
    app.update(Action::Left);
    assert_eq!(
        app.repeat_preview().first().map(|date| date.to_string()),
        Some("2025-10-03".to_owned()),
        "h walks the day of the month back, and the 3rd has gone"
    );
}

#[test]
fn r_on_a_copy_changes_the_schedule_behind_it() {
    let (mut app, copy) = with_a_recurring_copy();

    app.update(Action::Repeat);
    app.update(Action::EveryDay);
    app.update(Action::Confirm);

    assert_eq!(rule_of(&app, 1), Some(Rule::Daily));
    assert_eq!(hint(&app), "Changed the repeat of \"Write standup notes\"");
    assert!(
        app.model()
            .task(copy)
            .is_some_and(|task| task.day.is_some()),
        "the copy on the day stays as it is"
    );
}

#[test]
fn r_on_a_schedule_row_stops_it_and_the_copies_stay() {
    let mut app = app_at(a_schedule_through("2025-09-04"), NOW);
    app.update(Action::PaneRight);
    assert_eq!(app.cursor(List::Backlog), Some(RowId::Schedule(1)));

    app.update(Action::Repeat);
    app.update(Action::StopRepeat);
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "Stopped repeating \"Write standup notes\"");
    assert!(
        app.backlog().schedules.is_empty(),
        "a stopped schedule leaves the list"
    );
    assert_eq!(
        titles(&app, List::Day),
        ["Write standup notes"],
        "and its copies stay where they are"
    );

    app.update(Action::Undo);
    assert_eq!(app.backlog().schedules.len(), 1);
}

#[test]
fn stopping_a_task_that_does_not_repeat_says_so() {
    let mut app = started();
    add(&mut app, "Book dentist");

    app.update(Action::Repeat);
    app.update(Action::StopRepeat);
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "That task does not repeat.");
    assert!(app.model().schedules.is_empty());
}

// ---- the schedules under the backlog ---------------------------------

#[test]
fn the_cursor_walks_on_to_the_schedules_and_a_task_key_says_so() {
    let mut app = app_at(a_schedule_through("2025-09-04"), NOW);
    app.update(Action::PaneRight);
    let waiting = add(&mut app, "Quote from the electrician");
    app.update(Action::Waiting);

    assert_eq!(
        groups(&app, List::Backlog),
        [Group::Waiting, Group::Schedules]
    );

    app.update(Action::Down);
    assert_eq!(
        app.cursor(List::Backlog),
        Some(RowId::Schedule(1)),
        "the row under the last task is the schedule"
    );

    app.update(Action::Close);
    assert_eq!(hint(&app), "That row is a repeat schedule, not a task.");
    assert!(app.model().task(waiting).is_some_and(|task| task.is_open()));
}

// ---- the date card ---------------------------------------------------

/// An app with one task in the backlog and the cursor on it.
fn with_a_backlog_task(title: &str) -> (App, Id) {
    let mut app = started();
    app.update(Action::PaneRight);
    let id = add(&mut app, title);
    app.update(Action::Cancel);
    (app, id)
}

fn due_on(app: &App, task: Id) -> Option<String> {
    app.model()
        .task(task)
        .and_then(|task| task.due_on)
        .map(|date| date.to_string())
}

#[test]
fn d_writes_a_due_date_and_r_a_reminder() {
    let (mut app, task) = with_a_backlog_task("Write the Q4 planning doc");

    app.update(Action::DueBy);
    type_in(&mut app, "30 sep");
    app.update(Action::Confirm);

    assert_eq!(due_on(&app, task).as_deref(), Some("2025-09-30"));
    assert_eq!(hint(&app), "Due date on \"Write the Q4 planning doc\"");
    assert!(app.popup().is_none(), "the card is done");
    // The task stays in the backlog: a date surfaces it, never moves it.
    assert_eq!(titles(&app, List::Backlog), ["Write the Q4 planning doc"]);

    app.update(Action::RemindOn);
    type_in(&mut app, "1 oct");
    app.update(Action::Confirm);

    assert_eq!(
        app.model()
            .task(task)
            .and_then(|task| task.remind_on)
            .map(|date| date.to_string())
            .as_deref(),
        Some("2025-10-01")
    );
    assert_eq!(due_on(&app, task).as_deref(), Some("2025-09-30"), "both");
}

#[test]
fn the_date_card_is_one_card_with_two_modes() {
    let (mut app, task) = with_a_backlog_task("Renew passport");

    app.update(Action::DueBy);
    type_in(&mut app, "30 sep");
    // alt-r inside the card: the same typed date, the other meaning.
    app.update(Action::RemindOn);
    app.update(Action::Confirm);

    assert_eq!(due_on(&app, task), None);
    assert_eq!(
        app.model()
            .task(task)
            .and_then(|task| task.remind_on)
            .map(|date| date.to_string())
            .as_deref(),
        Some("2025-09-30")
    );
}

#[test]
fn a_quick_pick_is_the_answer_and_closes_the_card() {
    let (mut app, task) = with_a_backlog_task("Book dentist");

    app.update(Action::DueBy);
    app.update(Action::EndOfMonth);

    assert!(app.popup().is_none());
    assert_eq!(due_on(&app, task).as_deref(), Some("2025-09-30"));

    app.update(Action::DueBy);
    app.update(Action::ClearDate);

    assert_eq!(due_on(&app, task), None);
    assert_eq!(hint(&app), "Cleared the due date on \"Book dentist\"");
}

#[test]
fn the_calendar_walks_days_and_months_once_tab_is_pressed() {
    let (mut app, task) = with_a_backlog_task("Renew passport");

    app.update(Action::DueBy);
    app.update(Action::NextPane);
    for action in [Action::Right, Action::Down, Action::NextMonth] {
        app.update(action);
    }
    app.update(Action::Confirm);

    // Friday 5 September, a day on, a week on, a month on.
    assert_eq!(due_on(&app, task).as_deref(), Some("2025-10-13"));
}

#[test]
fn text_that_is_not_a_date_is_refused_rather_than_guessed() {
    let (mut app, task) = with_a_backlog_task("Renew passport");

    app.update(Action::DueBy);
    type_in(&mut app, "someday");
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "That is not a date I can read.");
    assert!(app.popup().is_some(), "the card stays open");
    assert_eq!(due_on(&app, task), None);

    app.update(Action::Cancel);
    assert!(app.popup().is_none());
    assert_eq!(due_on(&app, task), None, "escape writes nothing");
}

#[test]
fn clearing_is_not_a_day_the_move_card_can_send_a_task_to() {
    let (mut app, task) = with_a_backlog_task("Clean out the garage");

    app.update(Action::MoveToDay);
    app.update(Action::GoToDate);
    app.update(Action::ClearDate);

    assert!(app.popup().is_some(), "the card is still asking");
    assert_eq!(app.model().task(task).and_then(|task| task.day), None);
}

#[test]
fn the_move_cards_pick_a_date_moves_the_task_to_the_day() {
    let (mut app, task) = with_a_backlog_task("Clean out the garage");

    app.update(Action::MoveToDay);
    app.update(Action::GoToDate);
    type_in(&mut app, "8 sep");
    app.update(Action::Confirm);

    assert_eq!(
        app.model().task(task).and_then(|task| task.day),
        Some(on("2025-09-08"))
    );
    assert!(titles(&app, List::Backlog).is_empty());
    assert_eq!(hint(&app), "Moved \"Clean out the garage\" to Mon 8 Sep");
}

// ---- waiting ---------------------------------------------------------

#[test]
fn w_moves_a_backlog_row_under_waiting_and_back() {
    let mut app = started();
    app.update(Action::PaneRight);
    let blocked = add(&mut app, "Quote from the electrician");
    add(&mut app, "Clean out the garage");
    app.update(Action::Up);

    app.update(Action::Waiting);
    assert_eq!(hint(&app), "Waiting on \"Quote from the electrician\"");
    assert_eq!(
        groups(&app, List::Backlog),
        [Group::Ordinary, Group::Waiting]
    );
    assert_eq!(
        titles(&app, List::Backlog),
        ["Clean out the garage", "Quote from the electrician"]
    );
    // The row moved between the groups of one pane; the cursor is a flag
    // behind, not a task behind.
    assert_eq!(cursor(&app, List::Backlog), Some(blocked));

    app.update(Action::Waiting);
    assert_eq!(
        hint(&app),
        "No longer waiting on \"Quote from the electrician\""
    );
    assert_eq!(
        groups(&app, List::Backlog),
        [Group::Ordinary, Group::Ordinary]
    );
}

#[test]
fn w_on_a_day_task_sends_it_to_the_backlog_as_waiting() {
    let mut app = started();
    add(&mut app, "Chase the hosting invoice");
    let next = add(&mut app, "Review Anna's PR");
    app.update(Action::Up);

    app.update(Action::Waiting);

    assert_eq!(
        titles(&app, List::Backlog),
        ["Chase the hosting invoice"],
        "waiting is a backlog state"
    );
    assert!(app.backlog().waiting.len() == 1);
    // A move like any other: the pointer stays on the day and the cursor
    // steps to the next row of the group it left.
    assert_eq!(groups(&app, List::Day), [Group::Plan, Group::Moved]);
    assert_eq!(cursor(&app, List::Day), Some(next));

    app.update(Action::Undo);
    assert_eq!(
        titles(&app, List::Day),
        ["Chase the hosting invoice", "Review Anna's PR"]
    );
    assert!(app.backlog().waiting.is_empty());
}

#[test]
fn pulling_a_waiting_task_onto_today_clears_the_flag() {
    let mut app = started();
    app.update(Action::PaneRight);
    let task = add(&mut app, "Feedback on the proposal");
    app.update(Action::Waiting);

    app.update(Action::ToToday);

    assert!(app.model().task(task).is_some_and(|task| !task.waiting));
    assert_eq!(titles(&app, List::Day), ["Feedback on the proposal"]);
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
    assert_eq!(cursor(&app, List::Day), Some(kept));

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

/// Closing steps the cursor on, so `space` `u` `space` closed two
/// different tasks until the undo brought the cursor back with the task
/// (DESIGN.md section 4).
#[test]
fn undo_puts_the_cursor_on_the_task_it_brought_back() {
    let mut app = started();
    let invoice = add(&mut app, "Ship invoice export");
    let dentist = add(&mut app, "Book dentist");
    app.update(Action::Up);
    assert_eq!(cursor(&app, List::Day), Some(invoice));

    app.update(Action::Close);
    assert_eq!(
        cursor(&app, List::Day),
        Some(dentist),
        "the close steps the cursor on"
    );

    app.update(Action::Undo);
    assert_eq!(cursor(&app, List::Day), Some(invoice));

    app.update(Action::Close);
    assert!(
        app.model().task(dentist).is_some_and(Task::is_open),
        "and the second space closes the same task, not the next one"
    );
}

#[test]
fn undo_puts_the_cursor_on_the_task_it_restored() {
    let mut app = started();
    let invoice = add(&mut app, "Ship invoice export");
    add(&mut app, "Book dentist");
    app.update(Action::Up);

    app.update(Action::Delete);
    assert_ne!(cursor(&app, List::Day), Some(invoice));

    app.update(Action::Undo);
    assert_eq!(cursor(&app, List::Day), Some(invoice));
}

#[test]
fn undo_of_a_move_takes_the_keyboard_to_the_pane_the_task_went_back_to() {
    let mut app = started();
    let invoice = add(&mut app, "Ship invoice export");
    add(&mut app, "Book dentist");
    app.update(Action::Up);
    app.update(Action::ToBacklog);
    app.update(Action::PaneRight);
    assert_eq!(app.focused(), List::Backlog);

    app.update(Action::Undo);
    assert_eq!(app.focused(), List::Day);
    assert_eq!(cursor(&app, List::Day), Some(invoice));
}

/// "On a list that is on screen" is the whole of it: the notes page has
/// neither pane, so the cursor is left where the close put it.
#[test]
fn undo_from_another_page_leaves_the_cursor_where_it_was() {
    let mut app = started();
    add(&mut app, "Ship invoice export");
    let dentist = add(&mut app, "Book dentist");
    app.update(Action::Up);
    app.update(Action::Close);
    app.update(Action::NotesPage);

    app.update(Action::Undo);
    assert_eq!(app.page(), Page::Notes);
    assert_eq!(cursor(&app, List::Day), Some(dentist));
}

// ---- stepping through the days ---------------------------------------

/// The day before the one the tests stand on, which is the day `[` steps
/// back to.
const YESTERDAY: &str = "2025-09-04";

/// The tasks of today's plan moved back a day, as if the window had been
/// left open over the night, which is the shape every past-day test
/// needs and no key can make.
fn rewound(app: &App, ids: &[Id]) -> App {
    let mut model = app.model().clone();
    let yesterday = on(YESTERDAY);
    for id in ids {
        if let Some(task) = model.tasks.get_mut(id)
            && task.day == Some(on("2025-09-05"))
        {
            task.day = Some(yesterday);
        }
        if let Some(placed) = model.placements.remove(&(*id, on("2025-09-05"))) {
            model.placements.insert(
                (*id, yesterday),
                Placement {
                    day: yesterday,
                    ..placed
                },
            );
        }
    }
    // The task on yesterday is a pile, and these tests are about
    // stepping back to it rather than about the review it would open.
    app_at(MemStore::holding(reviewed(model)), NOW)
}

/// An app on a store where one task was planned for yesterday and is
/// still there, so yesterday is somewhere to step back to.
fn with_a_past_day() -> (App, Id) {
    let mut app = started();
    let id = add(&mut app, "Call the accountant about VAT");
    (rewound(&app, &[id]), id)
}

#[test]
fn brackets_step_the_day_pane_and_a_dot_comes_back_to_today() {
    let mut app = started();
    assert_eq!(app.showing(), on("2025-09-05"));

    app.update(Action::PrevDay);
    app.update(Action::PrevDay);
    assert_eq!(app.showing(), on("2025-09-03"));
    assert_eq!(app.day().day, on("2025-09-03"));

    app.update(Action::NextDay);
    assert_eq!(app.showing(), on("2025-09-04"));

    app.update(Action::Today);
    assert_eq!(app.showing(), on("2025-09-05"));
}

#[test]
fn the_pane_beside_a_day_that_is_not_today_is_the_list_of_days() {
    let (mut app, _) = with_a_past_day();
    app.update(Action::PaneRight);
    assert_eq!(app.focused(), List::Backlog);

    app.update(Action::PrevDay);

    assert_eq!(app.focused(), List::Days, "the backlog gives way to it");
    assert_eq!(
        app.rows_of(List::Days),
        [(RowId::Day(on(YESTERDAY)), Group::Days)]
    );
    assert_eq!(
        app.key_context(),
        KeyContext::Home {
            pane: Pane::Backlog,
            day: Shown::Past,
            field: None
        }
    );
}

#[test]
fn a_past_day_shows_what_was_planned_on_it_however_it_ended() {
    let mut app = started();
    let open = add(&mut app, "Call the accountant about VAT");
    let done = add(&mut app, "Weekly planning");
    app.update(Action::Close);
    let moved = add(&mut app, "Book the venue");
    app.update(Action::ToBacklog);

    let mut app = rewound(&app, &[open, done, moved]);
    app.update(Action::PrevDay);

    assert_eq!(app.day().counts.planned, 3);
    assert!(
        app.day().plan[0].on_the_pile,
        "still open on a day that is over"
    );
    assert_eq!(
        titles(&app, List::Day),
        [
            "Call the accountant about VAT",
            "Weekly planning",
            "Book the venue"
        ]
    );
    assert_eq!(
        groups(&app, List::Day),
        [Group::Plan, Group::Done, Group::Moved]
    );
    assert_eq!(
        app.day().moved[0].place,
        Place::Backlog,
        "it points at the backlog"
    );
}

#[test]
fn moving_a_task_off_a_day_never_rewrites_what_that_day_planned() {
    let (mut app, id) = with_a_past_day();
    app.update(Action::PrevDay);
    assert_eq!(groups(&app, List::Day), [Group::Plan]);

    app.update(Action::ToToday);
    assert_eq!(
        groups(&app, List::Day),
        [Group::Moved],
        "the record stands; only the row's state changed"
    );
    assert_eq!(app.day().moved[0].place, Place::Day(on("2025-09-05")));

    // And again, on: the day it was planned for points at wherever it
    // ends up, not at the step in between.
    app.update(Action::Today);
    app.update(Action::MoveToDay);
    app.update(Action::Tomorrow);
    app.update(Action::GoToDate);
    type_in(&mut app, YESTERDAY);
    app.update(Action::Confirm);

    assert_eq!(app.showing(), on(YESTERDAY));
    assert_eq!(app.day().moved[0].place, Place::Day(on("2025-09-06")));
    assert_eq!(
        app.model().task(id).and_then(|task| task.day),
        Some(on("2025-09-06"))
    );
}

#[test]
fn enter_on_a_moved_row_goes_to_where_the_task_is_now() {
    let (mut app, id) = with_a_past_day();
    app.update(Action::PrevDay);
    app.update(Action::ToToday);
    assert_eq!(groups(&app, List::Day), [Group::Moved]);

    app.update(Action::Confirm);

    assert_eq!(app.showing(), on("2025-09-05"), "today, where it went");
    assert_eq!(app.focused(), List::Day);
    assert_eq!(cursor(&app, List::Day), Some(id));
}

#[test]
fn enter_on_a_moved_row_that_points_at_the_backlog_comes_back_to_today() {
    let (mut app, id) = with_a_past_day();
    app.update(Action::PrevDay);
    app.update(Action::ToBacklog);

    app.update(Action::Confirm);

    assert_eq!(app.showing(), on("2025-09-05"));
    assert_eq!(app.focused(), List::Backlog);
    assert_eq!(cursor(&app, List::Backlog), Some(id));
}

#[test]
fn enter_on_a_day_of_the_list_goes_to_that_day() {
    let (mut app, _) = with_a_past_day();
    app.update(Action::PrevDay);
    app.update(Action::PrevDay);
    assert_eq!(app.showing(), on("2025-09-03"), "a day with nothing on it");
    app.update(Action::PaneRight);

    app.update(Action::Confirm);

    assert_eq!(app.showing(), on(YESTERDAY));
    assert_eq!(
        app.focused(),
        List::Day,
        "going to a day means looking at it"
    );
}

#[test]
fn g_opens_the_card_that_goes_to_a_day_and_is_about_no_task() {
    let mut app = started();
    add(&mut app, "Ship invoice export");

    app.update(Action::GoToDate);
    assert_eq!(
        app.popup().map(|popup| (popup.kind, popup.target)),
        Some((PopupKind::Date, None))
    );

    type_in(&mut app, YESTERDAY);
    app.update(Action::Confirm);

    assert!(app.popup().is_none());
    assert_eq!(app.showing(), on(YESTERDAY));
    assert_eq!(
        titles(&app, List::Day),
        Vec::<String>::new(),
        "nothing was planned that day"
    );
}

#[test]
fn a_task_added_while_a_past_day_is_shown_goes_on_that_day() {
    let (mut app, _) = with_a_past_day();
    app.update(Action::PrevDay);

    add(&mut app, "Send the meter reading");

    assert_eq!(
        titles(&app, List::Day),
        ["Call the accountant about VAT", "Send the meter reading"]
    );
    app.update(Action::Today);
    assert_eq!(titles(&app, List::Day), Vec::<String>::new());
}

#[test]
fn the_day_pane_follows_the_day_over_only_when_it_was_on_today() {
    let mut app = started();
    app.update(Action::PrevDay);
    app.clock = at("2025-09-06T09:00:00+02:00[Europe/Copenhagen]");
    app.update(Action::Tick);

    assert_eq!(app.today(), on("2025-09-06"));
    assert_eq!(app.showing(), on("2025-09-04"), "the day it was reading");

    app.update(Action::Today);
    app.clock = at("2025-09-07T09:00:00+02:00[Europe/Copenhagen]");
    app.update(Action::Tick);
    assert_eq!(app.showing(), on("2025-09-07"));
}

// ---- search ----------------------------------------------------------

#[test]
fn enter_in_search_goes_to_the_day_the_result_is_on() {
    let mut app = started();
    let id = add(&mut app, "Send the invoice to Nordic Ltd");
    let mut app = rewound(&app, &[id]);
    app.update(Action::Search);
    type_in(&mut app, "invoice");

    app.update(Action::Confirm);

    assert!(app.popup().is_none());
    assert_eq!(app.showing(), on(YESTERDAY));
    assert_eq!(app.focused(), List::Day);
    assert_eq!(cursor(&app, List::Day), Some(id));
}

#[test]
fn enter_in_search_on_a_backlog_task_comes_back_to_today() {
    let mut app = started();
    app.update(Action::PaneRight);
    let id = add(&mut app, "Chase the unpaid invoices");
    app.update(Action::PaneLeft);
    app.update(Action::PrevDay);
    app.update(Action::Search);
    type_in(&mut app, "invoice");

    app.update(Action::Confirm);

    assert_eq!(
        app.showing(),
        on("2025-09-05"),
        "the backlog is beside today"
    );
    assert_eq!(app.focused(), List::Backlog);
    assert_eq!(cursor(&app, List::Backlog), Some(id));
}

#[test]
fn alt_t_in_search_starts_the_task_it_found_again_on_today() {
    let mut app = started();
    let old = add(&mut app, "Send the meter reading");
    app.update(Action::Close);
    let mut app = rewound(&app, &[old]);
    app.update(Action::Search);
    type_in(&mut app, "meter");

    app.update(Action::ToToday);

    assert!(app.popup().is_none());
    assert_eq!(app.showing(), on("2025-09-05"));
    assert_eq!(titles(&app, List::Day), ["Send the meter reading"]);
    let added = cursor(&app, List::Day).expect("the new task");
    assert_ne!(added, old, "a fresh task, not the one that was found");
    assert!(
        app.model()
            .task(old)
            .is_some_and(|task| !task.is_open() && task.day == Some(on(YESTERDAY))),
        "what was found stays where it was, and closed"
    );
}

#[test]
fn a_search_that_found_nothing_adds_what_was_typed() {
    let mut app = started();
    app.update(Action::Search);
    type_in(&mut app, "tax return");

    app.update(Action::Confirm);

    assert_eq!(titles(&app, List::Day), ["tax return"]);
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
        .filter_map(|(id, _)| id.task())
        .collect();

    app.update(Action::Up);
    app.update(Action::Up);
    app.update(Action::Up);
    assert_eq!(
        cursor(&app, List::Day),
        Some(ids[0]),
        "the top does not wrap"
    );

    for _ in 0..50 {
        app.update(Action::Down);
    }
    assert_eq!(
        cursor(&app, List::Day),
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
    let day = cursor(&app, List::Day);

    app.update(Action::PaneRight);
    assert_eq!(app.pane(), Pane::Backlog);
    add(&mut app, "In the backlog");

    app.update(Action::PaneLeft);
    assert_eq!(app.pane(), Pane::Day);
    assert_eq!(
        cursor(&app, List::Day),
        day,
        "coming back lands where it was"
    );
}

#[test]
fn a_cursor_on_a_row_another_window_took_away_clamps_to_the_first() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    add(&mut app, "One");
    let second = add(&mut app, "Two");
    assert_eq!(cursor(&app, List::Day), Some(second));

    let mut elsewhere = app_at(store, NOW);
    elsewhere.update(Action::Down);
    elsewhere.update(Action::Delete);
    app.update(Action::Tick);

    assert_eq!(titles(&app, List::Day), ["One"]);
    assert_eq!(
        cursor(&app, List::Day),
        app.rows_of(List::Day).first().and_then(|(id, _)| id.task())
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
fn esc_leaves_the_notes_page_the_way_n_does() {
    let mut app = started();
    app.update(Action::NotesPage);
    app.update(Action::Cancel);

    assert_eq!(app.page(), Page::Home);
}

#[test]
fn a_makes_a_note_and_puts_the_cursor_on_it() {
    let mut app = started();
    app.update(Action::NotesPage);
    app.update(Action::Add);

    let first = note_cursor(&app).expect("the note just made");
    assert_eq!(app.notes().count, 1);
    assert_eq!(hint(&app), "Added a note");
    assert!(app.editor().is_none(), "a note is not a title being typed");

    // The newest note is at the top of the list, and the cursor follows.
    app.update(Action::Add);
    let second = note_cursor(&app).expect("the second note");
    assert_ne!(second, first);
    assert_eq!(
        app.rows_of(List::Notes)
            .into_iter()
            .filter_map(|(id, _)| id.note())
            .collect::<Vec<_>>(),
        [second, first]
    );
}

#[test]
fn x_throws_a_note_away_and_u_brings_it_back() {
    let mut app = started();
    app.update(Action::NotesPage);
    app.update(Action::Add);
    app.update(Action::Add);
    let top = note_cursor(&app).expect("a note");

    app.update(Action::Delete);
    assert_eq!(app.notes().count, 1);
    assert_eq!(hint(&app), "Deleted a note");
    assert!(app.message().is_some_and(|message| message.undo));
    assert_ne!(note_cursor(&app), Some(top), "the cursor steps on");

    app.update(Action::Undo);
    assert_eq!(app.notes().count, 2);
}

/// A note made, opened and typed into, which is the whole of writing one.
fn note_saying(app: &mut App, body: &str) -> Id {
    app.update(Action::Add);
    type_in(app, body);
    note_cursor(app).expect("the note just made")
}

#[test]
fn a_new_note_opens_for_typing_straight_away() {
    let mut app = started();
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "remember to mention X");

    assert_eq!(app.notes_pane(), NotesPane::Note);
    assert_eq!(
        app.key_context(),
        KeyContext::Notes {
            pane: NotesPane::Note,
            text_field: true
        },
        "every letter types in an open note"
    );
    let draft = app.draft().expect("the note being typed");
    assert_eq!(
        (draft.note, draft.text.as_str()),
        (note, "remember to mention X")
    );
}

#[test]
fn a_note_body_is_written_on_the_next_tick() {
    let mut app = started();
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "remember to mention X");

    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some(""),
        "the typing is in the application until the pause"
    );

    app.update(Action::Tick);
    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some("remember to mention X"),
        "a quarter of a second of typing at most is ever at risk"
    );
    assert!(app.message().is_none(), "a saved body is not news");
}

#[test]
fn leaving_a_note_writes_what_was_typed() {
    let mut app = started();
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "remember to mention X");
    app.update(Action::Cancel);

    assert_eq!(app.notes_pane(), NotesPane::List);
    assert!(app.draft().is_none());
    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some("remember to mention X")
    );

    // And again, reopened: the caret waits at the end of what is there.
    app.update(Action::Confirm);
    let draft = app.draft().expect("the note open again");
    assert_eq!(draft.text, "remember to mention X");
    assert_eq!(draft.caret, 21);
}

#[test]
fn a_note_survives_the_page_being_left_and_the_program_quitting() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    note_saying(&mut app, "first");
    app.update(Action::NotesPage);
    app.update(Action::NotesPage);
    note_saying(&mut app, "second");
    assert_eq!(app.update(Action::Quit), Flow::Quit);

    // What another window loads is what was typed, both times.
    let again = app_at(store, NOW);
    let bodies: Vec<&str> = again
        .notes()
        .rows
        .iter()
        .filter_map(|row| again.model().note(row.note))
        .map(|note| note.body.as_str())
        .collect();
    assert_eq!(bodies, ["second", "first"]);
}

#[test]
fn enter_is_a_line_of_the_note_and_the_caret_walks_it() {
    let mut app = started();
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "one");
    app.update(Action::Insert('\n'));
    type_in(&mut app, "two");

    // Up keeps the column it can; Home and End are the line's own ends.
    app.update(Action::Up);
    assert_eq!(app.draft().expect("the note").caret, 3);
    app.update(Action::LineStart);
    assert_eq!(app.draft().expect("the note").caret, 0);
    app.update(Action::Down);
    assert_eq!(app.draft().expect("the note").caret, 4);
    app.update(Action::LineEnd);
    assert_eq!(app.draft().expect("the note").caret, 7);

    app.update(Action::Tick);
    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some("one\ntwo")
    );
}

#[test]
fn the_note_the_cursor_is_on_is_the_one_that_opens() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "first");
    app.update(Action::Cancel);
    let second = note_saying(&mut app, "second");
    app.update(Action::Cancel);

    // The newest is at the top, and the list cursor is on it.
    app.update(Action::Down);
    app.update(Action::Confirm);
    let draft = app.draft().expect("a note");
    assert_ne!(draft.note, second);
    assert_eq!(draft.text, "first");
}

#[test]
fn tab_walks_from_the_list_into_the_note_and_back() {
    let mut app = started();
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "half a thought");
    app.update(Action::PaneLeft);

    assert_eq!(app.notes_pane(), NotesPane::List);
    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some("half a thought"),
        "leaving by the pane key writes it too"
    );

    app.update(Action::PaneRight);
    assert_eq!(app.notes_pane(), NotesPane::Note);
    assert!(app.draft().is_some());
}

#[test]
fn a_note_another_window_threw_away_stops_being_typed_into() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    let mut elsewhere = app_at(store, NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "half a thought");

    elsewhere.update(Action::NotesPage);
    elsewhere.update(Action::Tick);
    elsewhere.update(Action::Delete);
    assert_eq!(
        elsewhere.notes().count,
        0,
        "the other window did throw it away"
    );

    app.update(Action::Tick);
    assert!(app.draft().is_none(), "there is nothing left to write to");
    assert_eq!(app.notes_pane(), NotesPane::List);
    assert!(app.model().note(note).is_some_and(|note| !note.is_live()));
    assert!(app.message().is_none(), "and no complaint about it");
}

#[test]
fn a_key_for_tasks_says_so_on_the_notes_page() {
    let mut app = started();
    app.update(Action::NotesPage);
    app.update(Action::Close);

    assert_eq!(hint(&app), "That key is for tasks, and this page is notes.");
}

#[test]
fn x_on_an_empty_notes_page_says_there_is_nothing_there() {
    let mut app = started();
    app.update(Action::NotesPage);
    app.update(Action::Delete);

    assert_eq!(hint(&app), "There is no note here yet.");
}

// ---- popups ----------------------------------------------------------

#[test]
fn the_hint_bar_has_a_context_to_draw_from() {
    let app = started();

    assert_eq!(
        app.key_context(),
        KeyContext::Home {
            pane: Pane::Day,
            day: Shown::Today,
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
            day: Shown::Today,
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
    let was = cursor(&app, List::Day);
    app.update(Action::Commands);

    app.update(Action::Down);
    app.update(Action::Down);
    assert_eq!(app.popup().map(|popup| popup.selected), Some(2));
    assert_eq!(cursor(&app, List::Day), was, "the pane did not move");

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
    let was = cursor(&app, List::Backlog);

    app.update(Action::MouseDown {
        column: 70,
        row: 30,
    });

    assert_eq!(app.pane(), Pane::Backlog);
    assert_eq!(cursor(&app, List::Backlog), was);
}

#[test]
fn the_wheel_moves_the_cursor() {
    let mut app = started();
    add(&mut app, "One");
    add(&mut app, "Two");
    app.update(Action::Up);
    let first = cursor(&app, List::Day);

    app.update(Action::Scroll {
        column: 4,
        row: 6,
        down: true,
    });

    assert_ne!(cursor(&app, List::Day), first);
}

#[test]
fn a_reorder_marks_the_row_it_is_carrying_until_the_next_key() {
    let mut app = started();
    add(&mut app, "One");
    let two = add(&mut app, "Two");

    app.update(Action::MoveUp);
    assert_eq!(app.moving(), Some(two));

    app.update(Action::Down);
    assert_eq!(app.moving(), None, "the mark goes with the message");
}

#[test]
fn dragging_a_row_carries_it_the_way_the_keys_do() {
    let mut app = started();
    let one = add(&mut app, "One");
    add(&mut app, "Two");
    let three = add(&mut app, "Three");

    let rows: Vec<RowArea> = app
        .rows_of(List::Day)
        .into_iter()
        .enumerate()
        .map(|(at, (id, _))| RowArea {
            list: List::Day,
            id,
            area: Rect {
                x: 0,
                y: 5 + at as u16,
                width: 59,
                height: 1,
            },
        })
        .collect();
    app.set_layout(Layout {
        narrow: false,
        lists: vec![ListArea {
            list: List::Day,
            area: Rect {
                x: 0,
                y: 5,
                width: 59,
                height: 28,
            },
        }],
        rows,
    });

    // Take hold of the third row and drag it over the first.
    app.update(Action::MouseDown { column: 4, row: 7 });
    app.update(Action::MouseDrag { column: 4, row: 6 });
    app.update(Action::MouseDrag { column: 4, row: 5 });
    app.update(Action::MouseUp { column: 4, row: 5 });

    assert_eq!(titles(&app, List::Day), ["Three", "One", "Two"]);
    assert_eq!(cursor(&app, List::Day), Some(three));
    assert_eq!(
        app.model().task(one).map(|task| task.position),
        Some(1),
        "and the rows it passed shifted, once each"
    );
}

#[test]
fn a_change_that_cannot_be_saved_is_a_hint_and_not_a_crash() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    add(&mut app, "Book dentist");

    let mut app = App::new(Box::new(Broken(store)), &at(NOW)).expect("an app");
    app.update(Action::Delete);

    assert_eq!(hint(&app), "The change could not be saved.");
    assert_eq!(
        titles(&app, List::Day),
        ["Book dentist"],
        "the model is left as it was"
    );
}

#[test]
fn an_undo_that_no_longer_applies_is_dropped_and_says_so() {
    // An entry left by an instance that has since been overtaken: the
    // task it would put back is not gone at all.
    let mut app = started();
    add(&mut app, "Book dentist");
    let id = cursor(&app, List::Day).expect("the task");
    let mut model = app.model().clone();
    model.undo.push(domain::UndoEntry {
        id: 99,
        at: at(NOW),
        label: "Deleted \"Book dentist\"".to_owned(),
        inverse: Command::RestoreTask {
            task: id,
            position: 0,
        },
    });
    let mut app = app_at(MemStore::holding(model), NOW);

    app.update(Action::Undo);

    assert_eq!(
        hint(&app),
        "Deleted \"Book dentist\" could not be undone: That task is already back."
    );
    assert_eq!(titles(&app, List::Day), ["Book dentist"]);
    assert_eq!(
        app.model().undo.last().map(|entry| entry.label.as_str()),
        Some("Added \"Book dentist\""),
        "the entry that could not be undone is dropped off the stack"
    );
}

#[test]
fn j_and_k_reorder_the_backlog_too() {
    let mut app = started();
    app.update(Action::PaneRight);
    add(&mut app, "One");
    add(&mut app, "Two");
    let three = add(&mut app, "Three");

    app.update(Action::MoveUp);
    assert_eq!(titles(&app, List::Backlog), ["One", "Three", "Two"]);
    assert_eq!(
        cursor(&app, List::Backlog),
        Some(three),
        "the cursor goes with it"
    );
    app.update(Action::MoveDown);
    assert_eq!(titles(&app, List::Backlog), ["One", "Two", "Three"]);
}

/// Backspace, Delete and the arrow keys took a code point at a time, so
/// one press could leave half a family emoji or an accent with nothing
/// to sit on. The unit is the grapheme cluster (F6).
#[test]
fn the_editing_keys_take_a_whole_cluster_at_a_time() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    let mut app = started();
    app.update(Action::Add);
    type_in(&mut app, &format!("cafe\u{301}{family}!"));

    // Three clusters back from the end is the `f`, not the middle of the
    // family.
    for _ in 0..3 {
        app.update(Action::Left);
    }
    app.update(Action::Backspace);
    app.update(Action::DeleteForward);
    app.update(Action::Confirm);

    let title = app
        .model()
        .task(cursor(&app, app.focused()).expect("the task"))
        .map(|task| task.title.clone());
    assert_eq!(
        title,
        Some(format!("ca{family}!")),
        "Backspace took the `f` and Delete the whole `e` with its accent"
    );
}
