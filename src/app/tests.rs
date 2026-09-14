use super::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jiff::civil::Date;

use crate::domain::tests::MemStore;
use crate::domain::{
    Change, Context, Placement, Rule, Schedule, Task, WeekStart, Weekday, WorkDays, Write,
};

/// A window manager a test can question: what it was told, what it was
/// asked to show, and whether it was there to be told at all.
#[derive(Clone)]
struct Desk {
    here: bool,
    /// Whether there is a compositor running to resize a window, which a
    /// configuration to write the rule into does not promise.
    shows: bool,
    told: Rc<RefCell<Vec<(bool, WindowSize)>>>,
    shown: Rc<RefCell<Vec<WindowSize>>>,
}

impl Desk {
    fn here() -> Desk {
        Desk {
            here: true,
            shows: true,
            told: Rc::new(RefCell::new(Vec::new())),
            shown: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn absent() -> Desk {
        Desk {
            here: false,
            shows: false,
            ..Desk::here()
        }
    }

    /// A configuration to write the rule into, with nothing running to
    /// show it in.
    fn unattended() -> Desk {
        Desk {
            shows: false,
            ..Desk::here()
        }
    }

    fn told(&self) -> Vec<(bool, WindowSize)> {
        self.told.borrow().clone()
    }

    fn shown(&self) -> Vec<WindowSize> {
        self.shown.borrow().clone()
    }
}

impl Desktop for Desk {
    fn available(&self) -> bool {
        self.here
    }

    fn apply_window(&self, floating: bool, size: WindowSize) -> Result<(), String> {
        self.told.borrow_mut().push((floating, size));
        if self.here {
            Ok(())
        } else {
            Err("Hyprland is not here; the setting is kept for when it is.".to_owned())
        }
    }

    fn preview(&self, size: WindowSize) -> Result<bool, String> {
        self.shown.borrow_mut().push(size);
        Ok(self.shows)
    }
}

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
        model.settings.working_day(&at(NOW)).to_string(),
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
    App::new(
        Box::new(store),
        Box::new(Desk::here()),
        Locale::default(),
        &at(now),
    )
    .expect("an app")
}

/// An app on a window manager the test holds a second handle on.
fn app_on(store: MemStore, desk: &Desk, now: &str) -> App {
    App::new(
        Box::new(store),
        Box::new(desk.clone()),
        Locale::default(),
        &at(now),
    )
    .expect("an app")
}

fn app_in(store: MemStore, locale: Locale, now: &str) -> App {
    App::new(Box::new(store), Box::new(Desk::here()), locale, &at(now)).expect("an app")
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
    let today = Settings::default().working_day(&now);
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
fn the_review_waits_for_m_when_it_is_set_not_to_open_itself() {
    let mut model = left_behind(&[("Order new office chair", "2025-09-01")])
        .load()
        .expect("the store");
    model.settings.set_review_opens_itself(false);
    let mut app = app_at(MemStore::holding(model), NOW);

    assert!(app.review().is_none());
    assert_eq!(app.review_count(), 1, "the pile is counted all the same");
    assert_eq!(
        app.model().meta.get("review_on"),
        None,
        "the gate is left for the day the review is asked for"
    );

    app.update(Action::OpenReview);

    assert!(app.review().is_some());
}

#[test]
fn the_pile_horizon_takes_an_older_day_out_of_the_review_count() {
    let mut model = left_behind(&[
        ("Order new office chair", "2025-08-01"),
        ("Chase the invoice", "2025-09-04"),
    ])
    .load()
    .expect("the store");
    model.settings.set_pile_horizon_days(7);
    let app = app_at(MemStore::holding(model), NOW);

    assert_eq!(app.review_count(), 1);
    assert_eq!(
        app.review()
            .and_then(|review| review.pile())
            .map(|pile| pile.total),
        Some(1),
        "the review opens on what the horizon leaves"
    );
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

/// The setting turned on, which is the whole of what `confirm_delete`
/// changes about `x`.
fn asks_first(app: &mut App) {
    let mut settings = app.settings().clone();
    settings.set_confirm_delete(true);
    app.change_settings(settings);
}

#[test]
fn x_asks_first_when_the_setting_is_on_and_esc_keeps_the_row() {
    let mut app = started();
    add(&mut app, "Book dentist");
    asks_first(&mut app);

    app.update(Action::Delete);
    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::DeleteQuestion,
            text_field: false
        }
    );
    assert_eq!(
        titles(&app, List::Day),
        ["Book dentist"],
        "the question is asked before anything is done"
    );

    app.update(Action::Cancel);
    assert!(app.popup().is_none());
    assert_eq!(titles(&app, List::Day), ["Book dentist"]);
    assert_eq!(hint(&app), "", "keeping a row is not news");
}

#[test]
fn enter_on_the_question_deletes_the_row_it_was_asked_about() {
    let mut app = started();
    add(&mut app, "Book dentist");
    let kept = add(&mut app, "Review Anna's PR");
    app.update(Action::Up);
    asks_first(&mut app);

    app.update(Action::Delete);
    app.update(Action::Confirm);

    assert!(app.popup().is_none());
    assert_eq!(titles(&app, List::Day), ["Review Anna's PR"]);
    assert_eq!(hint(&app), "Deleted \"Book dentist\"");
    assert!(app.message().is_some_and(|message| message.undo));
    assert_eq!(cursor(&app, List::Day), Some(kept), "the cursor steps on");
}

#[test]
fn a_note_is_asked_about_the_same_way() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember to mention X");
    asks_first(&mut app);

    app.update(Action::Delete);
    app.update(Action::Cancel);
    assert_eq!(app.notes().count, 1, "the note is still there");

    app.update(Action::Delete);
    app.update(Action::Confirm);
    assert_eq!(app.notes().count, 0);
    assert_eq!(hint(&app), "Deleted a note");
}

#[test]
fn a_pile_row_is_asked_about_before_the_review_lets_it_go() {
    let mut app = app_at(left_behind(&[("One", "2025-09-04")]), NOW);
    asks_first(&mut app);

    app.update(Action::Delete);
    assert_eq!(
        app.review().expect("the review").decision(1),
        None,
        "nothing is decided while the question is up"
    );

    app.update(Action::Confirm);
    assert_eq!(
        app.review().expect("the review").decision(1),
        Some(Decided::Deleted)
    );
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
fn a_message_nobody_types_past_goes_after_a_few_seconds() {
    let mut app = started();
    add(&mut app, "Book dentist");
    app.update(Action::Delete);

    app.clock = app
        .clock
        .checked_add(Span::new().seconds(3))
        .expect("a time");
    app.update(Action::Tick);
    assert!(
        app.message().is_some(),
        "three seconds is a pause to read it"
    );

    app.clock = app
        .clock
        .checked_add(Span::new().seconds(2))
        .expect("a time");
    app.update(Action::Tick);
    assert!(
        app.message().is_none(),
        "five seconds is a pause in front of the wrong line"
    );
}

#[test]
fn a_message_that_stands_for_no_seconds_waits_for_the_next_key() {
    let mut app = started();
    let mut settings = app.settings().clone();
    settings.set_message_seconds(0);
    app.change_settings(settings);
    add(&mut app, "Book dentist");
    app.update(Action::Delete);

    app.clock = app
        .clock
        .checked_add(Span::new().minutes(5))
        .expect("a time");
    app.update(Action::Tick);
    assert!(app.message().is_some(), "no tick ever takes it away");

    app.update(Action::Down);
    assert!(app.message().is_none(), "and the next key does");
}

#[test]
fn a_longer_setting_holds_a_message_past_the_default() {
    let mut app = started();
    let mut settings = app.settings().clone();
    settings.set_message_seconds(10);
    app.change_settings(settings);
    add(&mut app, "Book dentist");
    app.update(Action::Delete);

    app.clock = app
        .clock
        .checked_add(Span::new().seconds(6))
        .expect("a time");
    app.update(Action::Tick);
    assert!(app.message().is_some(), "six of the ten seconds");

    app.clock = app
        .clock
        .checked_add(Span::new().seconds(5))
        .expect("a time");
    app.update(Action::Tick);
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

// ---- the open note on screen: its rows, its caret and its mouse ------

/// The pane the note was drawn in, as the frame in front of the writer
/// would have left it: `width` cells of body and `height` rows of them,
/// at a corner no other part of these tests uses.
///
/// The application never draws; a frame is `ui::draw` and then
/// `set_layout`, and this is the second half of that on its own.
fn note_pane(app: &mut App, width: u16, height: u16) {
    let note = app
        .draft()
        .map(|draft| draft.note)
        .or_else(|| note_cursor(app))
        .expect("a note to draw");
    app.set_layout(Layout {
        note: Some(NoteArea {
            note,
            // One column more than the body is wrapped at, which is the
            // caret's own.
            area: Rect {
                x: 4,
                y: 10,
                width: width + 1,
                height,
            },
        }),
        ..Layout::default()
    });
}

/// A note of `lines` numbered lines, open with a pane around it.
fn lined_note(width: u16, height: u16) -> App {
    let mut app = started();
    app.update(Action::NotesPage);
    let body: Vec<String> = (0..8).map(|at| format!("line {at}")).collect();
    note_saying(&mut app, &body.join("\n"));
    note_pane(&mut app, width, height);
    app
}

fn caret(app: &App) -> usize {
    app.draft().expect("the open note").caret
}

fn first_row(app: &App) -> usize {
    app.draft().expect("the open note").first
}

#[test]
fn up_from_the_end_of_a_long_note_walks_the_rows_on_screen_before_they_move() {
    // Eight rows in a pane three high, opened at the end of the body, so
    // the last three rows are the ones on screen.
    let mut app = lined_note(20, 3);
    assert_eq!(first_row(&app), 5, "rows five, six and seven");

    // Two steps up the rows already on screen move nothing.
    app.update(Action::Up);
    assert_eq!(first_row(&app), 5, "the rows stayed where they were");
    app.update(Action::Up);
    assert_eq!(first_row(&app), 5);
    assert_eq!(
        caret(&app),
        "line 0\nline 1\nline 2\nline 3\nline 4\nline 5".len()
    );

    // The third reaches the top row, and only the fourth moves them.
    app.update(Action::Up);
    assert_eq!(first_row(&app), 4, "one row, and one row only");
    app.update(Action::Up);
    assert_eq!(first_row(&app), 3);
}

#[test]
fn down_from_the_top_of_a_long_note_does_the_same_the_other_way() {
    let mut app = lined_note(20, 3);
    app.update(Action::LineStart);
    for _ in 0..7 {
        app.update(Action::Up);
    }
    assert_eq!(first_row(&app), 0, "back at the top of the body");

    app.update(Action::Down);
    app.update(Action::Down);
    assert_eq!(first_row(&app), 0, "the rows on screen have the caret");
    app.update(Action::Down);
    assert_eq!(first_row(&app), 1);
}

#[test]
fn the_caret_steps_by_the_rows_the_pane_wrapped_and_not_the_lines_typed() {
    let mut app = started();
    app.update(Action::NotesPage);
    // One line the writer typed, four rows the pane drew: "one two ",
    // "three ", "four " and "five".
    note_saying(&mut app, "one two three four five");
    note_pane(&mut app, 8, 10);
    app.update(Action::LineStart);
    assert_eq!(caret(&app), 19, "the start of the row, not of the line");

    app.update(Action::Up);
    assert_eq!(caret(&app), 14, "the row above, not the start of the body");
    app.update(Action::Up);
    assert_eq!(caret(&app), 8);
    app.update(Action::Up);
    assert_eq!(caret(&app), 0);
    app.update(Action::Up);
    assert_eq!(caret(&app), 0, "and no further");
}

#[test]
fn the_end_of_a_row_the_pane_broke_stays_on_that_row() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "one two three four");
    note_pane(&mut app, 10, 10);

    // The rows are "one two " and "three four"; End on the first is the
    // character the second starts at, drawn at the end of the first.
    app.update(Action::Up);
    app.update(Action::LineEnd);
    let draft = app.draft().expect("the open note");
    assert_eq!(draft.caret, 8);
    assert_eq!(draft.affinity, Affinity::BeforeTheBreak);

    // A step to either side is a body being walked through, and forgets
    // which side of the break the caret was on.
    app.update(Action::Left);
    app.update(Action::Right);
    let draft = app.draft().expect("the open note");
    assert_eq!(draft.caret, 8);
    assert_eq!(draft.affinity, Affinity::AfterTheBreak);
}

#[test]
fn a_step_onto_a_row_too_short_for_the_column_keeps_it_for_the_next_one() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "12345\n1\n12345");
    note_pane(&mut app, 20, 10);
    app.update(Action::Up);
    app.update(Action::Up);
    app.update(Action::LineStart);
    for _ in 0..4 {
        app.update(Action::Right);
    }
    assert_eq!(caret(&app), 4, "the fifth cell of the first row");

    // Down onto a row with one character on it, and down again: the cell
    // the steps started in is the cell they come back out in.
    app.update(Action::Down);
    assert_eq!(caret(&app), 7, "the end of the short row");
    app.update(Action::Down);
    assert_eq!(caret(&app), 12, "the fifth cell again, not the second");
}

#[test]
fn a_column_kept_across_rows_is_forgotten_by_every_other_way_of_moving() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "12345\n1\n12345");
    note_pane(&mut app, 20, 10);
    app.update(Action::Up);
    app.update(Action::Up);
    app.update(Action::LineStart);
    for _ in 0..4 {
        app.update(Action::Right);
    }
    app.update(Action::Down);
    // A step to the side on the short row is where the next step down
    // takes its column from.
    app.update(Action::Left);
    app.update(Action::Down);
    assert_eq!(caret(&app), 8, "the start of the last row");
}

#[test]
fn a_click_puts_the_caret_on_the_character_it_landed_on() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "one two three four");
    note_pane(&mut app, 10, 10);

    // The second row is "three four"; its fourth cell is the "e".
    app.update(Action::MouseDown {
        column: 4 + 3,
        row: 10 + 1,
    });
    assert_eq!(caret(&app), 11);
    assert_eq!(app.notes_pane(), NotesPane::Note);
}

#[test]
fn a_click_reads_the_row_it_landed_on_as_it_was_drawn() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "one two three four");
    note_pane(&mut app, 10, 10);
    // The caret overlays the first character of the second row.
    app.update(Action::Up);
    app.update(Action::LineStart);
    app.update(Action::Down);
    app.update(Action::LineStart);
    assert_eq!(caret(&app), 8);

    app.update(Action::MouseDown {
        column: 4 + 3,
        row: 10 + 1,
    });
    assert_eq!(caret(&app), 11, "the character drawn in that cell");
}

#[test]
fn a_click_in_the_blank_below_the_last_row_goes_to_the_end_of_the_note() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "one two three four");
    note_pane(&mut app, 10, 10);
    app.update(Action::LineStart);
    app.update(Action::Up);

    app.update(Action::MouseDown {
        column: 4 + 6,
        row: 10 + 7,
    });
    assert_eq!(caret(&app), 18, "the end of the body, not a column of it");
}

#[test]
fn a_click_on_the_note_beside_the_list_opens_it_where_it_was_clicked() {
    let mut app = lined_note(20, 20);
    app.update(Action::Cancel);
    assert_eq!(
        app.notes_pane(),
        NotesPane::List,
        "the list has the keyboard"
    );
    note_pane(&mut app, 20, 20);

    // The third row of the note the list is on, three cells along.
    app.update(Action::MouseDown {
        column: 4 + 3,
        row: 10 + 2,
    });

    assert_eq!(app.notes_pane(), NotesPane::Note);
    assert_eq!(caret(&app), "line 0\nline 1\nlin".len());
    assert_eq!(first_row(&app), 0, "the rows the click was aimed at");
}

#[test]
fn a_click_on_a_note_beside_the_list_lands_where_the_frame_showed_it() {
    // A note longer than the pane, which the list has the keyboard on:
    // the pane shows it from its first row, whatever the caret would be
    // when it opens.
    let mut app = lined_note(20, 3);
    app.update(Action::Cancel);
    note_pane(&mut app, 20, 3);

    app.update(Action::MouseDown {
        column: 4 + 2,
        row: 10 + 1,
    });

    assert_eq!(caret(&app), "line 0\nli".len(), "the second row on screen");
    assert_eq!(first_row(&app), 0, "and the rows have not moved under it");
}

#[test]
fn a_click_after_the_note_has_been_scrolled_reads_the_rows_on_screen() {
    // Opened at the end of eight rows in a pane three high, so the top
    // row on screen is the sixth of the body.
    let mut app = lined_note(20, 3);
    assert_eq!(first_row(&app), 5);

    app.update(Action::MouseDown {
        column: 4 + 2,
        row: 10,
    });
    assert_eq!(
        caret(&app),
        "line 0\nline 1\nline 2\nline 3\nline 4\nli".len()
    );
    assert_eq!(first_row(&app), 5, "the click moved no rows");
}

#[test]
fn a_pane_that_shrank_keeps_the_caret_on_screen() {
    let mut app = lined_note(20, 6);
    assert_eq!(first_row(&app), 2, "six of the eight rows");

    // The window is drawn again, two rows shorter.
    note_pane(&mut app, 20, 2);
    assert_eq!(first_row(&app), 6, "the caret is on the last row");
    assert_eq!(
        caret(&app),
        "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7".len()
    );
}

#[test]
fn a_pane_that_grew_shows_as_much_of_the_note_as_it_can() {
    let mut app = lined_note(20, 3);
    assert_eq!(first_row(&app), 5);

    note_pane(&mut app, 20, 6);
    assert_eq!(first_row(&app), 2, "rows came back rather than blank ones");
    note_pane(&mut app, 20, 12);
    assert_eq!(
        first_row(&app),
        0,
        "and a pane taller than the note shows it all"
    );
}

#[test]
fn a_window_that_changed_width_steps_by_the_rows_it_drew() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "one two three four five");
    // Wide enough for one row, so up has nowhere to go.
    note_pane(&mut app, 40, 10);
    app.update(Action::Up);
    assert_eq!(caret(&app), 23, "one row, and the caret at the end of it");

    // Narrow enough for four, the caret four cells along the last of
    // them.
    note_pane(&mut app, 8, 10);
    app.update(Action::Up);
    assert_eq!(caret(&app), 18, "the row the narrower pane drew");
    app.update(Action::Up);
    assert_eq!(caret(&app), 12);
}

#[test]
fn the_rows_of_a_note_include_the_empty_lines_in_it() {
    let mut app = started();
    app.update(Action::NotesPage);
    // A line, an empty one, and the empty line a trailing newline opens.
    note_saying(&mut app, "one\n\n");
    note_pane(&mut app, 20, 10);
    assert_eq!(caret(&app), 5, "the end of the body");

    app.update(Action::Up);
    assert_eq!(caret(&app), 4, "the empty line between them");
    app.update(Action::Up);
    assert_eq!(caret(&app), 0);
    app.update(Action::Down);
    app.update(Action::Down);
    assert_eq!(caret(&app), 5);

    // And a click on one of them is that line, not the one with words.
    app.update(Action::MouseDown {
        column: 4 + 6,
        row: 10 + 1,
    });
    assert_eq!(caret(&app), 4);
}

#[test]
fn a_note_of_wide_characters_steps_and_is_clicked_by_cells() {
    let mut app = started();
    app.update(Action::NotesPage);
    // Three characters of two cells each, and two of one.
    note_saying(&mut app, "\u{65e5}\u{672c}\u{8a9e}\nab");
    note_pane(&mut app, 20, 10);
    assert_eq!(caret(&app), 6, "the end of the body");

    // The second cell of the row above is the middle of the first
    // character, so the step lands in front of the second.
    app.update(Action::Up);
    assert_eq!(caret(&app), 1);
    app.update(Action::Down);
    assert_eq!(
        caret(&app),
        6,
        "and the column it kept is past both of them"
    );

    // The fifth cell belongs to the third character, regardless of the caret.
    app.update(Action::Up);
    app.update(Action::MouseDown {
        column: 4 + 4,
        row: 10,
    });
    assert_eq!(caret(&app), 2);
}

#[test]
fn another_note_opens_at_its_own_end_with_none_of_the_last_one_showing() {
    let mut app = lined_note(20, 3);
    app.update(Action::Cancel);
    // A second note, which the list puts above the first.
    note_saying(&mut app, "short");
    app.update(Action::Cancel);

    // The long one, scrolled to the end of its body.
    app.update(Action::Down);
    app.update(Action::Confirm);
    note_pane(&mut app, 20, 3);
    assert_eq!(first_row(&app), 5);

    // And the short one, which has rows of its own.
    app.update(Action::Cancel);
    app.update(Action::Up);
    app.update(Action::Confirm);
    assert_eq!(first_row(&app), 0, "a note of its own, from its first row");
    assert_eq!(caret(&app), 5, "at the end of what is written in it");
}

#[test]
fn a_column_kept_across_rows_is_kept_across_a_resize_too() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "abcdefgh\nij\nabcdefgh");
    note_pane(&mut app, 20, 10);
    app.update(Action::Up);
    app.update(Action::Up);
    app.update(Action::LineStart);
    for _ in 0..5 {
        app.update(Action::Right);
    }
    app.update(Action::Down);
    assert_eq!(caret(&app), 11, "the end of the short line");

    // The window is drawn again at another width, and the step that
    // follows is still aiming at the cell the first one was.
    note_pane(&mut app, 30, 10);
    app.update(Action::Down);
    assert_eq!(caret(&app), 17, "the sixth cell of the last row");
}

#[test]
fn a_click_after_a_resize_lands_on_the_rows_the_new_window_drew() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "one two three four five");
    note_pane(&mut app, 40, 10);
    // Narrower: three rows where there was one.
    note_pane(&mut app, 8, 10);

    app.update(Action::MouseDown {
        column: 4 + 2,
        row: 10 + 1,
    });
    assert_eq!(caret(&app), 10, "the third cell of \"three \"");
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

// ---- what another window did -----------------------------------------

/// A change another window made, committed through a second handle on
/// the same data, which is what makes it another connection.
fn elsewhere(store: &MemStore, command: Command) -> Id {
    let mut handle = store.clone();
    let model = handle.load().expect("load");
    let change = domain::apply(
        &model,
        command,
        &Context {
            now: at(NOW),
            undo_cap: 50,
            dates: DateOrder::DayFirst,
        },
    )
    .expect("the other window's change");
    handle.commit(&change).expect("the other window's change");
    model.tasks.keys().next_back().copied().unwrap_or(0)
}

/// When the other window gets in: after this one has taken its model or
/// committed its change, and before it has asked what the version is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overtake {
    Load,
    Commit,
}

/// A store another window writes to at that one moment. A version read
/// after it has already seen a write the model has not, so a window that
/// records it then would mark as seen a change it has not got, and would
/// go on showing rows nobody has any more until the next write moved the
/// version again.
struct Overtaken {
    shared: MemStore,
    when: Overtake,
    done: Rc<Cell<bool>>,
}

impl Overtaken {
    fn at(when: Overtake) -> (Overtaken, MemStore) {
        let shared = MemStore::new();
        (
            Overtaken {
                shared: shared.clone(),
                when,
                done: Rc::new(Cell::new(false)),
            },
            shared,
        )
    }

    fn overtake(&self) {
        if self.done.replace(true) {
            return;
        }
        elsewhere(
            &self.shared,
            Command::AddTask {
                title: "From the other window".to_owned(),
                place: Place::Backlog,
            },
        );
    }
}

impl Store for Overtaken {
    fn load(&self) -> Result<Model, StoreError> {
        let model = self.shared.load()?;
        if self.when == Overtake::Load {
            self.overtake();
        }
        Ok(model)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.shared.commit(change)?;
        if self.when == Overtake::Commit {
            self.overtake();
        }
        Ok(())
    }

    fn version(&self) -> Result<u64, StoreError> {
        self.shared.version()
    }
}

fn app_over(store: Overtaken) -> App {
    App::new(
        Box::new(store),
        Box::new(Desk::here()),
        Locale::default(),
        &at(NOW),
    )
    .expect("an app")
}

#[test]
fn a_write_that_landed_while_the_model_was_being_read_is_still_picked_up() {
    let (store, _shared) = Overtaken::at(Overtake::Load);
    let mut app = app_over(store);

    assert!(
        !titles(&app, List::Backlog)
            .iter()
            .any(|it| it == "From the other window")
    );

    app.update(Action::Tick);

    assert_eq!(
        titles(&app, List::Backlog),
        ["From the other window"],
        "the version was read before the model, so it cannot have seen more than it"
    );
}

#[test]
fn a_write_that_landed_just_after_this_windows_commit_is_still_picked_up() {
    let (store, _shared) = Overtaken::at(Overtake::Commit);
    let mut app = app_over(store);

    add(&mut app, "Mine");
    assert!(
        !titles(&app, List::Backlog)
            .iter()
            .any(|it| it == "From the other window")
    );

    app.update(Action::Tick);

    assert_eq!(
        titles(&app, List::Backlog),
        ["From the other window"],
        "a commit of this window's own does not move the version, so none is recorded"
    );
    assert_eq!(titles(&app, List::Day), ["Mine"]);
}

/// The hour a day starts at is a setting, so the day a tick lands on has
/// to be worked out from the settings as they are after the reload and
/// not as they were before it.
#[test]
fn the_day_a_tick_lands_on_follows_the_settings_another_window_changed() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    assert_eq!(app.today().to_string(), NOW_DAY);

    let mut handle = store.clone();
    let model = handle.load().expect("load");
    let mut settings = model.settings.clone();
    // Nine in the morning is before a day that starts at ten, so today
    // is still the day before.
    settings.set_day_starts_at(10);
    let change = domain::change_settings(&model, settings).expect("the settings");
    handle.commit(&change).expect("the other window's settings");

    app.update(Action::Tick);

    assert_eq!(app.today().to_string(), "2025-09-04");
}

#[test]
fn an_open_note_nobody_typed_in_follows_the_body_another_window_wrote() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "the first line");
    app.update(Action::Tick);

    elsewhere(
        &store,
        Command::ReplaceNote {
            note,
            body: "what the command wrote".to_owned(),
        },
    );
    app.update(Action::Tick);

    assert_eq!(
        app.draft().map(|draft| draft.text.as_str()),
        Some("what the command wrote"),
        "the open note follows the row"
    );
    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some("what the command wrote"),
        "and the body it was opened with is not written back over it"
    );
}

/// Typed here and changed elsewhere: neither text is written over the
/// other and neither is thrown away.
#[test]
fn a_note_typed_in_here_and_changed_elsewhere_keeps_both() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "the first line");
    app.update(Action::Tick);

    elsewhere(
        &store,
        Command::ReplaceNote {
            note,
            body: "what the command wrote".to_owned(),
        },
    );
    type_in(&mut app, " and more");
    app.update(Action::Tick);

    let draft = app.draft().expect("the note still being typed");
    assert_ne!(draft.note, note, "into a note of its own");
    assert_eq!(draft.text, "the first line and more");
    assert_eq!(
        app.model().note(draft.note).map(|note| note.body.as_str()),
        Some("the first line and more"),
        "what was typed here reached a row"
    );
    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some("what the command wrote"),
        "and the other window's words are as it left them"
    );
    assert_eq!(
        hint(&app),
        "Another window changed that note. What you typed is here, in a note of its own."
    );
    assert_eq!(note_cursor(&app), Some(draft.note));

    // One operation, so one `u` takes the recovery note back off again.
    let recovered = draft.note;
    app.update(Action::Undo);
    assert!(
        app.model()
            .note(recovered)
            .is_none_or(|note| !note.is_live())
    );
}

/// A note that reached no row at all keeps the keyboard where it is: the
/// page does not turn and the window does not close on the first ask,
/// so nothing is dropped without being said.
#[test]
fn what_could_not_be_written_anywhere_is_not_dropped_by_leaving() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    note_saying(&mut app, "the first line");
    app.update(Action::Tick);

    // The disk fills up under the window, and something is typed after.
    let mut app = App::new(
        Box::new(Broken(store.clone())),
        Box::new(Desk::here()),
        Locale::default(),
        &at(NOW),
    )
    .expect("an app");
    app.update(Action::NotesPage);
    app.update(Action::Confirm);
    type_in(&mut app, " and more");

    app.update(Action::Cancel);
    assert_eq!(
        app.draft().map(|draft| draft.text.as_str()),
        Some("the first line and more"),
        "the keyboard stays where the text is"
    );

    app.update(Action::NotesPage);
    assert_eq!(app.page(), Page::Notes, "and the page does not turn");

    assert_eq!(app.update(Action::Quit), Flow::Continue);
    assert_eq!(
        hint(&app),
        "What is typed in this note could not be saved. Quit again to leave it."
    );
    assert_eq!(
        app.update(Action::Quit),
        Flow::Quit,
        "asked twice by somebody who has read that, the window closes"
    );
}

/// The bodies of the live notes, which is how these tests count what
/// survived.
fn note_bodies(app: &App) -> Vec<String> {
    let mut bodies: Vec<String> = app
        .model()
        .notes
        .values()
        .filter(|note| note.is_live())
        .map(|note| note.body.clone())
        .collect();
    bodies.sort();
    bodies
}

/// Escape reaches the save with no tick in front of it, so the reload
/// belongs to the save rather than to the caller. Without it the body
/// the note was opened with is compared with a row that has already
/// moved, and the write that follows puts it back over the words the
/// command wrote.
#[test]
fn leaving_a_note_with_no_tick_first_still_sees_what_another_window_wrote() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "the first line");
    app.update(Action::Tick);

    elsewhere(
        &store,
        Command::ReplaceNote {
            note,
            body: "what the command wrote".to_owned(),
        },
    );
    type_in(&mut app, " and more");

    app.update(Action::Cancel);

    assert!(app.draft().is_none(), "the note was left");
    assert_eq!(
        app.model().note(note).map(|note| note.body.as_str()),
        Some("what the command wrote"),
        "the words the command wrote are still the words in that note"
    );
    assert_eq!(
        note_bodies(&app),
        ["the first line and more", "what the command wrote"],
        "and what was typed here is in a note of its own"
    );
}

#[test]
fn quitting_on_a_note_another_window_changed_writes_neither_over_the_other() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "the first line");
    app.update(Action::Tick);

    elsewhere(
        &store,
        Command::ReplaceNote {
            note,
            body: "what the command wrote".to_owned(),
        },
    );
    type_in(&mut app, " and more");

    assert_eq!(
        app.update(Action::Quit),
        Flow::Quit,
        "both texts reached a row, so there is nothing to hold the window open for"
    );
    assert_eq!(
        note_bodies(&app),
        ["the first line and more", "what the command wrote"]
    );
}

/// The same window, nothing typed in it: leaving takes the other
/// window's body rather than writing the opened one back.
#[test]
fn leaving_a_note_nobody_typed_in_writes_nothing_back_over_it() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "the first line");
    app.update(Action::Tick);

    elsewhere(
        &store,
        Command::ReplaceNote {
            note,
            body: "what the command wrote".to_owned(),
        },
    );

    app.update(Action::Cancel);

    assert_eq!(note_bodies(&app), ["what the command wrote"]);
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
        text_cells: Vec::new(),
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
        note: None,
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
        text_cells: Vec::new(),
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
        note: None,
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

    let mut app = App::new(
        Box::new(Broken(store)),
        Box::new(Desk::here()),
        Locale::default(),
        &at(NOW),
    )
    .expect("an app");
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

// ---- settings --------------------------------------------------------

/// A settings change with one field moved, the way the page will make it.
fn changed(app: &App, change: impl FnOnce(&mut Settings)) -> Settings {
    let mut settings = app.settings().clone();
    change(&mut settings);
    settings
}

#[test]
fn a_later_day_start_puts_the_morning_back_on_yesterday() {
    let mut app = app_at(
        MemStore::holding(reviewed(Model::empty())),
        "2025-09-05T07:00:00+02:00[Europe/Copenhagen]",
    );
    assert_eq!(app.today(), on("2025-09-05"));

    app.change_settings(changed(&app, |settings| settings.set_day_starts_at(8)));

    assert_eq!(app.today(), on("2025-09-04"));
    assert_eq!(app.showing(), on("2025-09-04"), "the pane follows today");
    assert_eq!(app.settings().day_starts_at(), 8);
}

#[test]
fn the_move_cards_next_work_day_follows_the_work_days_setting() {
    let mut app = started();
    add(&mut app, "Clean out the garage");
    app.change_settings(changed(&app, |settings| {
        settings.set_work_days(WorkDays::of([
            Weekday::Sun,
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
        ]))
    }));

    app.update(Action::MoveToDay);
    let days: Vec<MoveTarget> = app
        .move_choices()
        .iter()
        .map(|choice| choice.target)
        .collect();

    // Today is a Friday, so the next work day of a Sunday-to-Thursday
    // week is the Sunday rather than the Monday next Monday is.
    assert_eq!(days[2], MoveTarget::Day(on("2025-09-07")));
    assert_eq!(days[3], MoveTarget::Day(on("2025-09-08")));
}

#[test]
fn a_week_with_no_work_day_in_it_is_refused_and_says_why() {
    let mut app = started();

    app.change_settings(changed(&app, |settings| {
        settings.set_work_days(WorkDays::of([]))
    }));

    assert_eq!(
        hint(&app),
        "At least one day of the week must be a work day."
    );
    assert_eq!(app.settings(), &Settings::default());
}

#[test]
fn a_changed_window_setting_reaches_the_window_manager_on_the_next_tick() {
    let desk = Desk::here();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);

    app.change_settings(changed(&app, |settings| {
        settings.set_window_size(WindowSize::new(1200, 800))
    }));
    assert!(
        desk.told().is_empty(),
        "not while the keys are still coming"
    );
    app.update(Action::Tick);
    assert_eq!(desk.told(), [(true, WindowSize::new(1200, 800))]);

    // A setting that is nothing to do with the window leaves it alone.
    app.change_settings(changed(&app, |settings| settings.set_confirm_delete(true)));
    app.update(Action::Tick);
    assert_eq!(desk.told().len(), 1);
}

#[test]
fn a_size_held_down_reaches_the_window_manager_once_and_as_the_last_one() {
    let desk = Desk::here();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);

    for _ in 0..20 {
        app.update(Action::Right);
    }
    assert!(
        desk.told().is_empty(),
        "a key repeat is not a window manager's business"
    );

    app.update(Action::Tick);
    assert_eq!(desk.told(), [(true, WindowSize::PRESETS[4])]);

    // And nothing is owed once it has been paid.
    app.update(Action::Tick);
    assert_eq!(desk.told().len(), 1);
}

#[test]
fn a_floating_window_is_shown_the_size_it_was_given() {
    let desk = Desk::here();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);

    app.update(Action::Right);
    app.update(Action::Tick);
    assert_eq!(desk.shown(), [WindowSize::PRESETS[2]]);

    // A window that tiles has asked to be the size the tiling gives it.
    cursor_to(&mut app, SettingRow::FloatingWindow);
    app.update(Action::Pick);
    app.update(Action::Tick);
    assert_eq!(desk.told().len(), 2);
    assert_eq!(desk.shown().len(), 1);
}

#[test]
fn a_window_manager_that_is_not_there_says_so_in_the_hint_bar() {
    let desk = Desk::absent();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);

    app.change_settings(changed(&app, |settings| {
        settings.set_floating_window(false)
    }));
    app.update(Action::Tick);

    assert_eq!(
        hint(&app),
        "Hyprland is not here; the setting is kept for when it is."
    );
    assert!(
        !app.settings().floating_window(),
        "the setting is kept for when there is a window manager to take it"
    );
}

#[test]
fn the_date_order_is_the_locale_until_a_setting_says_otherwise() {
    let month_first = Locale {
        dates: DateOrder::MonthFirst,
    };
    let mut app = app_in(
        MemStore::holding(reviewed(Model::empty())),
        month_first,
        NOW,
    );
    assert_eq!(app.dates(), DateOrder::MonthFirst);

    app.change_settings(changed(&app, |settings| {
        settings.set_date_style(domain::DateStyle::DayFirst)
    }));
    assert_eq!(app.dates(), DateOrder::DayFirst);
}

#[test]
fn the_desktop_command_saves_the_window_and_tells_the_window_manager() {
    let desk = Desk::here();
    let mut store = MemStore::new();

    let said = set_window(
        &mut store,
        &desk,
        Some(false),
        Some(WindowSize::new(1000, 700)),
    )
    .expect("a window manager that took the rule");

    assert_eq!(said, "the window tiles");
    assert_eq!(desk.told(), [(false, WindowSize::new(1000, 700))]);
    let saved = store.load().expect("the model").settings;
    assert!(!saved.floating_window());
    assert_eq!(saved.window_size(), WindowSize::new(1000, 700));
}

#[test]
fn the_desktop_command_without_flags_writes_the_rule_the_settings_already_say() {
    let desk = Desk::here();
    let mut store = MemStore::new();

    let said = set_window(&mut store, &desk, None, None).expect("a window manager");

    assert_eq!(said, "the window floats at 870x650");
    assert_eq!(desk.told(), [(true, WindowSize::default())]);
}

#[test]
fn the_desktop_command_keeps_the_settings_where_there_is_no_window_manager() {
    let desk = Desk::absent();
    let mut store = MemStore::new();

    let line = set_window(&mut store, &desk, Some(false), None).expect("kept for later");

    assert_eq!(
        line,
        "Hyprland is not here; the settings are kept for when it is."
    );
    assert!(desk.told().is_empty());
    assert!(!store.load().expect("the model").settings.floating_window());
}

// ---- the settings page -----------------------------------------------

/// The cursor down the settings list to the row with this label, which
/// is how a test says which setting it is about without an index.
fn cursor_to(app: &mut App, row: SettingRow) {
    // From the top, because the list stops at both ends rather than
    // wrapping.
    for _ in 0..setting_rows().len() {
        app.update(Action::Up);
    }
    for _ in 0..setting_rows().len() {
        if app.cursor(List::Settings) == Some(RowId::Setting(row)) {
            return;
        }
        app.update(Action::Down);
    }
    panic!("{row:?} is not a row of the settings page");
}

#[test]
fn a_comma_opens_the_settings_and_the_same_key_brings_the_page_back() {
    let mut app = started();
    app.update(Action::NotesPage);
    assert_eq!(app.page(), Page::Notes);

    app.update(Action::SettingsPage);
    assert_eq!(app.page(), Page::Settings);
    assert_eq!(app.focused(), List::Settings);
    assert_eq!(
        app.cursor(List::Settings),
        Some(RowId::Setting(SettingRow::DayStartsAt)),
        "the page opens on its first row"
    );

    app.update(Action::SettingsPage);
    assert_eq!(app.page(), Page::Notes, "back to the page it was opened on");

    // And `esc` is the other way off it, back to wherever it came from.
    app.update(Action::NotesPage);
    app.update(Action::SettingsPage);
    app.update(Action::Cancel);
    assert_eq!(app.page(), Page::Home);
}

#[test]
fn every_kind_of_row_is_changed_by_the_same_two_keys() {
    let mut app = started();
    app.update(Action::SettingsPage);

    // A toggle: `l` turns it on, `h` off, and `space` turns it over.
    cursor_to(&mut app, SettingRow::ConfirmDelete);
    app.update(Action::Right);
    assert!(app.settings().confirm_delete());
    app.update(Action::Left);
    assert!(!app.settings().confirm_delete());
    app.update(Action::Pick);
    assert!(app.settings().confirm_delete());

    // One of the seven work days, which is a toggle of its own.
    cursor_to(&mut app, SettingRow::WorkDay(Weekday::Sat));
    app.update(Action::Pick);
    assert!(app.settings().work_days().contains(Weekday::Sat));

    // A row of two named values.
    cursor_to(&mut app, SettingRow::WeekStartsOn);
    app.update(Action::Confirm);
    assert_eq!(app.settings().week_starts_on(), WeekStart::Sunday);

    // A row of three, which `l` steps through and stops at the end of.
    cursor_to(&mut app, SettingRow::DateOrder);
    app.update(Action::Right);
    assert_eq!(app.settings().date_style(), DateStyle::DayFirst);
    app.update(Action::Right);
    app.update(Action::Right);
    assert_eq!(app.settings().date_style(), DateStyle::MonthFirst);

    // A number, held to its range whatever the key asks for.
    cursor_to(&mut app, SettingRow::DueAheadDays);
    app.update(Action::Left);
    assert_eq!(app.settings().due_ahead_days(), 0, "and no further");
    app.update(Action::Right);
    assert_eq!(app.settings().due_ahead_days(), 1);

    // The notes spell check, which is a toggle like any other.
    cursor_to(&mut app, SettingRow::SpellCheckNotes);
    app.update(Action::Left);
    assert!(!app.settings().spell_check_notes());
    app.update(Action::Right);
    assert!(app.settings().spell_check_notes());
    app.update(Action::Pick);
    assert!(!app.settings().spell_check_notes());

    // The window size, which steps from one preset to the next.
    cursor_to(&mut app, SettingRow::WindowSize);
    app.update(Action::Right);
    assert_eq!(app.settings().window_size(), WindowSize::PRESETS[2]);
}

#[test]
fn the_notes_spell_check_is_off_until_enabled_and_stays_on() {
    let store = MemStore::holding(reviewed(Model::empty()));
    let mut app = app_at(store.clone(), NOW);
    assert!(
        !app.settings().spell_check_notes(),
        "a database with no preference leaves spell checking off"
    );

    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::SpellCheckNotes);
    app.update(Action::Pick);

    assert!(app.settings().spell_check_notes());
    assert!(
        store
            .load()
            .expect("the model")
            .settings
            .spell_check_notes(),
        "and the row is written, so the next launch opens with it on"
    );

    // A second window reads it back the way it reads any other setting.
    let next = app_at(store.clone(), NOW);
    assert!(next.settings().spell_check_notes());
}

#[test]
fn the_window_size_walks_the_presets_and_stops_at_both_ends() {
    let mut app = started();
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);
    assert_eq!(app.settings().window_size(), WindowSize::PRESETS[1]);

    for _ in 0..10 {
        app.update(Action::Right);
    }
    assert_eq!(app.settings().window_size(), WindowSize::PRESETS[4]);

    for _ in 0..10 {
        app.update(Action::Left);
    }
    assert_eq!(app.settings().window_size(), WindowSize::PRESETS[0]);
}

#[test]
fn a_typed_size_steps_to_the_preset_on_the_side_the_key_asked_for() {
    let mut app = started();
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);
    app.change_settings(changed(&app, |settings| {
        settings.set_window_size(WindowSize::new(900, 700))
    }));

    app.update(Action::Right);
    assert_eq!(app.settings().window_size(), WindowSize::PRESETS[2]);

    app.change_settings(changed(&app, |settings| {
        settings.set_window_size(WindowSize::new(900, 700))
    }));
    app.update(Action::Left);
    assert_eq!(app.settings().window_size(), WindowSize::PRESETS[1]);
}

#[test]
fn a_number_is_typed_into_the_row_it_belongs_to() {
    let mut app = started();
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::PileHorizonDays);

    // Enter opens the field on the value that is there.
    app.update(Action::Confirm);
    assert_eq!(
        app.setting_draft().map(|draft| draft.text.clone()),
        Some("0".to_owned())
    );

    app.update(Action::Backspace);
    for typed in "90".chars() {
        app.update(Action::Insert(typed));
    }
    app.update(Action::Confirm);
    assert_eq!(app.settings().pile_horizon_days(), 90);
    assert!(app.setting_draft().is_none(), "the field is done with");

    // A line the field cannot read leaves it open and says so.
    app.update(Action::Confirm);
    app.update(Action::Insert('x'));
    app.update(Action::Confirm);
    assert_eq!(hint(&app), "That is not a number I can read.");
    assert!(app.setting_draft().is_some(), "so it can be typed again");
    assert_eq!(app.settings().pile_horizon_days(), 90);

    // Escape keeps what was there.
    app.update(Action::Cancel);
    assert!(app.setting_draft().is_none());
    assert_eq!(app.page(), Page::Settings, "and leaves the page open");
    assert_eq!(app.settings().pile_horizon_days(), 90);
}

#[test]
fn the_size_is_typed_as_the_row_writes_it() {
    let mut app = started();
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);
    app.update(Action::Confirm);
    assert_eq!(
        app.setting_draft().map(|draft| draft.text.clone()),
        Some("870x650".to_owned())
    );

    for _ in 0..7 {
        app.update(Action::Backspace);
    }
    for typed in "1200x800".chars() {
        app.update(Action::Insert(typed));
    }
    app.update(Action::Confirm);
    assert_eq!(app.settings().window_size(), WindowSize::new(1200, 800));
}

#[test]
fn a_week_the_domain_refuses_says_why_and_leaves_the_row_as_it_was() {
    let mut app = started();
    app.update(Action::SettingsPage);
    // Every work day off but the last, which is the one that is refused.
    for day in [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
    ] {
        cursor_to(&mut app, SettingRow::WorkDay(day));
        app.update(Action::Pick);
    }

    assert_eq!(
        hint(&app),
        "At least one day of the week must be a work day."
    );
    assert!(
        app.settings().work_days().contains(Weekday::Fri),
        "the last day stays a work day"
    );
}

#[test]
fn a_changed_window_setting_says_what_the_window_manager_answered() {
    let desk = Desk::here();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::FloatingWindow);

    app.update(Action::Pick);
    assert!(!app.settings().floating_window());
    app.update(Action::Tick);
    assert_eq!(desk.told(), [(false, WindowSize::default())]);
    assert_eq!(
        hint(&app),
        "The window rule is written. It applies the next time the app opens."
    );
}

#[test]
fn a_window_owed_at_quitting_time_is_paid_before_the_program_goes() {
    let desk = Desk::here();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);
    app.update(Action::Right);

    assert_eq!(app.update(Action::Quit), Flow::Quit);
    assert_eq!(desk.told(), [(true, WindowSize::PRESETS[2])]);
    assert!(
        desk.shown().is_empty(),
        "there is nothing to show in a window that is closing"
    );
}

#[test]
fn a_window_the_person_just_watched_resize_is_not_told_to_wait_for_a_launch() {
    let desk = Desk::here();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);

    app.update(Action::Right);
    app.update(Action::Tick);
    assert_eq!(
        hint(&app),
        "The window rule is written and the window is shown at that size."
    );
}

#[test]
fn a_rule_written_with_nothing_running_to_show_it_waits_for_the_next_launch() {
    let desk = Desk::unattended();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::WindowSize);

    app.update(Action::Right);
    app.update(Action::Tick);
    assert_eq!(desk.told(), [(true, WindowSize::PRESETS[2])]);
    assert_eq!(
        hint(&app),
        "The window rule is written. It applies the next time the app opens."
    );
}

#[test]
fn a_tick_with_no_window_owed_says_nothing_to_the_window_manager() {
    let desk = Desk::here();
    let mut app = app_on(MemStore::holding(reviewed(Model::empty())), &desk, NOW);

    app.update(Action::Tick);
    app.update(Action::Tick);

    assert!(desk.told().is_empty());
    assert!(desk.shown().is_empty());
}

#[test]
fn copying_a_note_uses_unsaved_text_and_preserves_the_editor() {
    let mut app = started();
    app.update(Action::NotesPage);
    let text = "First line\n\n  æøå 🦀\n";
    let note = note_saying(&mut app, text);
    app.update(Action::Left);
    let caret = app.draft().unwrap().caret;
    assert_eq!(app.model().note(note).unwrap().body, "");
    assert_eq!(
        app.update(Action::CopyNote),
        Flow::CopyNote(text.to_owned())
    );
    assert_eq!(app.draft().unwrap().caret, caret);
    assert_eq!(app.notes_pane(), NotesPane::Note);
    assert!(app.message().is_none(), "wait for the clipboard result");
    app.copied_note(Ok(()));
    assert_eq!(app.message().unwrap().text, "Note copied");
    app.update(Action::Cancel);
    assert_eq!(
        app.update(Action::CopyNote),
        Flow::CopyNote(text.to_owned())
    );
    assert_eq!(app.notes_pane(), NotesPane::List);
    app.copied_note(Err("Could not copy note: test failure".to_owned()));
    assert_eq!(
        app.message().unwrap().text,
        "Could not copy note: test failure"
    );
}

#[test]
fn copying_without_a_note_leaves_the_clipboard_alone() {
    let mut app = started();
    assert_eq!(app.update(Action::CopyNote), Flow::Continue);
    app.update(Action::NotesPage);
    assert_eq!(app.update(Action::CopyNote), Flow::Continue);
    assert_eq!(app.message().unwrap().text, "There is no note here yet.");
}

#[test]
fn selection_copy_and_cut_preserve_whole_graphemes_and_failed_cuts() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "Ae\u{301}界Z");
    app.update(Action::SelectLeft);
    app.update(Action::SelectLeft);

    assert_eq!(
        app.update(Action::CopySelection),
        Flow::CopySelection("界Z".to_owned())
    );
    app.copied_selection(false, Ok(()));
    assert_eq!(app.draft().unwrap().text, "Ae\u{301}界Z");
    assert_eq!(app.message().unwrap().text, "Selection copied");

    assert_eq!(
        app.update(Action::CutSelection),
        Flow::CutSelection("界Z".to_owned())
    );
    app.copied_selection(true, Err("clipboard refused".to_owned()));
    assert_eq!(app.draft().unwrap().text, "Ae\u{301}界Z");
    assert_eq!(app.selection(), Some(2..4));

    app.copied_selection(true, Ok(()));
    assert_eq!(app.draft().unwrap().text, "Ae\u{301}");
    assert_eq!(app.selection(), None);
    assert_eq!(app.message().unwrap().text, "Selection cut");
}

#[test]
fn a_successful_cut_settles_spelling_before_the_next_frame() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "teh ");
    assert_eq!(misspelt(&app), ["teh"]);
    for _ in 0..4 {
        app.update(Action::SelectLeft);
    }

    assert_eq!(
        app.update(Action::CutSelection),
        Flow::CutSelection("teh ".to_owned())
    );
    app.copied_selection(true, Ok(()));

    assert!(app.misspellings().is_empty());
    assert!(app.draft().unwrap().text.is_empty());
}

#[test]
fn paste_is_one_edit_that_replaces_selection_and_preserves_note_lines() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "one XX three");
    for _ in 0..8 {
        app.update(Action::Left);
    }
    app.update(Action::SelectRight);
    app.update(Action::SelectRight);

    assert_eq!(app.update(Action::Paste), Flow::ReadClipboard);
    app.paste(Ok("界\r\nline".to_owned()));
    assert_eq!(app.draft().unwrap().text, "one 界\nline three");
    assert_eq!(app.selection(), None);
}

#[test]
fn an_empty_paste_keeps_the_selection_and_text() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "keep this");
    app.update(Action::SelectLeft);

    app.paste(Ok(String::new()));

    assert_eq!(app.draft().unwrap().text, "keep this");
    assert_eq!(app.selection(), Some(8..9));
}

#[test]
fn a_failed_paste_keeps_the_selection_and_text() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "keep this");
    app.update(Action::SelectLeft);

    app.paste(Err("clipboard unavailable".to_owned()));

    assert_eq!(app.draft().unwrap().text, "keep this");
    assert_eq!(app.selection(), Some(8..9));
    assert_eq!(app.message().unwrap().text, "clipboard unavailable");
}

#[test]
fn paste_flattens_lines_in_single_line_fields_and_ignores_lists() {
    let mut app = started();
    app.paste(Ok("q\nquit".to_owned()));
    assert!(app.active_text().is_none());

    app.update(Action::Search);
    app.paste(Ok("one\r\ntwo\nthree".to_owned()));
    assert_eq!(
        app.popup().map(|popup| popup.text.as_str()),
        Some("one two three")
    );
}

#[test]
fn default_off_does_not_load_a_dictionary_or_mark_a_note() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    app.update(Action::Cancel);
    assert!(!app.settings().spell_check_notes());
    assert!(app.misspellings().is_empty());
    assert!(app.spelling.checker.is_none());
}

/// Spelling scenarios explicitly opt in, just as a user does in Settings.
fn enable_spelling(app: &mut App) {
    let mut settings = app.settings().clone();
    settings.set_spell_check_notes(true);
    app.change_settings(settings);
}

fn spell_started() -> App {
    let mut app = started();
    enable_spelling(&mut app);
    app
}

// ---- the spell check ----------------------------------------------

/// The words the open note has marked, read back as the text they cover,
/// so that a test names words rather than counting clusters.
fn misspelt(app: &App) -> Vec<String> {
    let note = note_cursor(app).expect("an open note");
    let body = match app.draft().filter(|draft| draft.note == note) {
        Some(draft) => draft.text.clone(),
        None => app
            .model()
            .note(note)
            .map_or_else(String::new, |note| note.body.clone()),
    };
    let glyphs: Vec<&str> = body.graphemes(true).collect();
    app.misspellings()
        .iter()
        .map(|word| glyphs[word.clone()].concat())
        .collect()
}

#[test]
fn a_word_the_checker_does_not_know_is_marked_where_it_sits() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting ");

    assert_eq!(
        app.misspellings().to_vec(),
        vec![9..12],
        "in clusters from the start of the body, as the caret is"
    );
    assert_eq!(misspelt(&app), ["teh"]);
}

#[test]
fn the_word_the_caret_is_in_waits_until_the_caret_has_left_it() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh");

    assert!(
        app.misspellings().is_empty(),
        "a word is not called wrong while it is still being typed, and \
         the caret at its end is where a word spends its whole writing"
    );

    // The space finishes it, and the mark appears where it was typed.
    app.update(Action::Insert(' '));
    assert_eq!(misspelt(&app), ["teh"]);

    // Going back to it takes the mark off again, for the same reason.
    app.update(Action::Left);
    assert!(app.misspellings().is_empty(), "the caret is at its end");
    app.update(Action::Left);
    assert!(app.misspellings().is_empty(), "and then inside it");
    app.update(Action::LineEnd);
    assert_eq!(misspelt(&app), ["teh"], "and off it again");
}

#[test]
fn a_note_being_looked_at_shows_every_word_in_it() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember the mistayk");

    assert!(
        app.misspellings().is_empty(),
        "the last word typed is still under the caret"
    );

    // Escape writes the body and puts the keyboard back on the list.
    // Nothing is being typed any more, so nothing is held back.
    app.update(Action::Cancel);
    assert!(app.draft().is_none());
    assert_eq!(misspelt(&app), ["mistayk"]);

    // Enter answers `update` before the end of it, so the note it opens
    // is checked all the same: the caret is at the end of the body.
    app.update(Action::Confirm);
    assert!(app.draft().is_some());
    assert!(
        app.misspellings().is_empty(),
        "the caret is at the end of the word again"
    );
}

#[test]
fn the_body_is_read_once_and_a_caret_a_tick_and_a_save_reuse_it() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting ");
    let read = app.spelling.runs;

    for _ in 0..4 {
        app.update(Action::Left);
    }
    app.update(Action::Right);
    // The first tick writes the body, which changes the model and not
    // the text on screen; the second has nothing left to do.
    app.update(Action::Tick);
    app.update(Action::Tick);

    assert_eq!(
        app.spelling.runs, read,
        "nothing was typed, so nothing was read again"
    );
    assert_eq!(misspelt(&app), ["teh"], "and the marks are still there");

    app.update(Action::Insert('s'));
    assert!(app.spelling.runs > read, "a keystroke is a new body");
}

#[test]
fn nothing_is_read_until_there_is_a_note_with_something_in_it() {
    let mut app = spell_started();
    assert!(
        app.spelling.checker.is_none(),
        "a program that only ever looks at tasks builds no dictionary"
    );

    app.update(Action::NotesPage);
    app.update(Action::Add);
    assert!(
        app.spelling.checker.is_none(),
        "and a note with nothing written in it has nothing to check"
    );

    type_in(&mut app, "teh ");
    assert!(app.spelling.checker.is_some());
    assert_eq!(misspelt(&app), ["teh"]);
}

#[test]
fn turning_the_setting_off_takes_the_marks_away_and_on_brings_them_back() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    assert_eq!(misspelt(&app), ["teh"]);

    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::SpellCheckNotes);
    app.update(Action::Pick);
    assert!(!app.settings().spell_check_notes());

    app.update(Action::SettingsPage);
    assert_eq!(app.page(), Page::Notes);
    assert!(
        app.misspellings().is_empty(),
        "a note nobody asked to have checked is not marked"
    );

    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::SpellCheckNotes);
    app.update(Action::Pick);
    app.update(Action::SettingsPage);
    assert_eq!(misspelt(&app), ["teh"]);
}

#[test]
fn the_setting_changed_without_a_key_takes_effect_at_once() {
    // `change_settings` is a way into the application of its own, so
    // what is on screen cannot wait for the next key to catch up.
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    assert_eq!(misspelt(&app), ["teh"]);

    let mut off = app.settings().clone();
    off.set_spell_check_notes(false);
    app.change_settings(off);
    assert!(app.misspellings().is_empty());

    let mut on = app.settings().clone();
    on.set_spell_check_notes(true);
    app.change_settings(on);
    assert_eq!(misspelt(&app), ["teh"]);
}

#[test]
fn the_marks_follow_the_cursor_from_one_note_to_another() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    app.update(Action::Cancel);
    app.update(Action::Add);
    type_in(&mut app, "the mistayk");
    app.update(Action::Cancel);

    // The newest note is at the top of the list, and the cursor is on it.
    assert_eq!(misspelt(&app), ["mistayk"]);
    app.update(Action::Down);
    assert_eq!(misspelt(&app), ["teh"]);
}

#[test]
fn a_note_another_window_rewrote_is_read_again_on_the_tick() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    enable_spelling(&mut app);
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    app.update(Action::Cancel);
    assert_eq!(misspelt(&app), ["teh"]);

    // Another window writes over the beginning of the same note.
    let mut other = app_at(store, NOW);
    other.update(Action::NotesPage);
    other.update(Action::Confirm);
    other.update(Action::LineStart);
    type_in(&mut other, "a mistayk ");
    other.update(Action::Tick);

    app.update(Action::Tick);
    assert_eq!(
        misspelt(&app),
        ["mistayk", "teh"],
        "the body that came back, in the order it is drawn"
    );
}

#[test]
fn a_note_thrown_away_leaves_nothing_marked() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    app.update(Action::Cancel);
    assert_eq!(misspelt(&app), ["teh"]);

    app.update(Action::Delete);
    assert_eq!(app.notes().count, 0);
    assert!(
        app.misspellings().is_empty(),
        "there is no note left to mark"
    );
}

// ---- what the dictionary offers instead -----------------------------

/// A note with the caret put inside the misspelt word of it, which is
/// where `alt-s` is pressed.
fn note_with_the_caret_in_teh() -> App {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    // Off the end of "meeting" and back onto the end of "teh", which is
    // where a word that has just been typed leaves the caret.
    for _ in 0..8 {
        app.update(Action::Left);
    }
    app
}

/// The card that is open, or what the hint bar said instead of opening
/// one.
fn spelling_card(app: &App) -> &SpellingDraft {
    app.popup()
        .and_then(Popup::spelling)
        .unwrap_or_else(|| panic!("no spelling card: {:?}", hint(app)))
}

/// The body of the open note as it stands, typed or saved.
fn note_body(app: &App) -> String {
    let note = note_cursor(app).expect("an open note");
    match app.draft().filter(|draft| draft.note == note) {
        Some(draft) => draft.text.clone(),
        None => app
            .model()
            .note(note)
            .map_or_else(String::new, |note| note.body.clone()),
    }
}

#[test]
fn alt_s_offers_the_dictionary_words_for_the_one_at_the_caret() {
    let mut app = note_with_the_caret_in_teh();
    app.update(Action::FixSpelling);

    let card = spelling_card(&app);
    assert_eq!(card.word, "teh", "the word the caret was in");
    assert_eq!(
        card.at,
        9..12,
        "the range the checker found, in clusters from the start of the body"
    );
    assert!(
        !card.suggestions.is_empty(),
        "a word the dictionary can better is offered what it can"
    );
    assert!(
        !card.suggestions.contains(&"teh".to_owned()),
        "and it does not offer the word back: {:?}",
        card.suggestions
    );
    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::Spelling,
            text_field: false
        },
        "the card has the keyboard, so the note's letters stop typing"
    );
}

#[test]
fn the_word_the_caret_is_inside_is_offered_as_readily_as_the_one_it_ends() {
    let mut app = note_with_the_caret_in_teh();
    // One more step left, which puts the caret between the t and the e.
    app.update(Action::Left);
    app.update(Action::FixSpelling);
    assert_eq!(spelling_card(&app).word, "teh");

    // And the far end of it, which the underlines hold back and this key
    // is entirely about.
    app.update(Action::Cancel);
    app.update(Action::Right);
    assert!(app.misspellings().is_empty(), "nothing is underlined here");
    app.update(Action::FixSpelling);
    assert_eq!(spelling_card(&app).word, "teh");
}

#[test]
fn enter_writes_the_chosen_word_over_that_word_and_over_nothing_else() {
    let mut app = note_with_the_caret_in_teh();
    app.update(Action::FixSpelling);
    let chosen = spelling_card(&app).suggestions[0].clone();

    app.update(Action::Confirm);
    assert!(app.popup().is_none(), "the card is answered and gone");
    assert_eq!(note_body(&app), format!("remember {chosen} meeting"));
    assert_eq!(
        app.draft().expect("the note is still open").caret,
        9 + chosen.graphemes(true).count(),
        "the caret comes to rest at the end of the word that was written"
    );
    assert_eq!(hint(&app), format!("teh became {chosen}."));
}

#[test]
fn up_and_down_choose_which_word_goes_in() {
    let mut app = note_with_the_caret_in_teh();
    app.update(Action::FixSpelling);
    let offered = spelling_card(&app).suggestions.clone();
    let add = spelling_card(&app).add_row();

    app.update(Action::Down);
    assert_eq!(
        selected(&app),
        1,
        "one row down, which the card opened on the first of"
    );

    // The card goes round: the row that adds the word is under the
    // suggestions, and a step up from the first of them.
    app.update(Action::Up);
    app.update(Action::Up);
    assert_eq!(selected(&app), add, "up from the first row is the offer");
    app.update(Action::Down);
    assert_eq!(selected(&app), 0, "and down from the offer is the first");

    app.update(Action::Down);
    let chosen = offered[selected(&app)].clone();
    app.update(Action::Confirm);
    assert_eq!(note_body(&app), format!("remember {chosen} meeting"));
}

/// Which row of the open card is selected.
fn selected(app: &App) -> usize {
    app.popup().expect("the card").selected
}

/// A note with a word no dictionary was ever going to know, with the
/// caret left at the end of it.
fn note_with_a_name() -> App {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "Zqxjkv rang about Zqxjkv");
    app
}

#[test]
fn the_card_opens_on_the_offer_where_the_dictionary_has_nothing_to_say() {
    let mut app = note_with_a_name();
    app.update(Action::FixSpelling);

    let card = spelling_card(&app);
    assert_eq!(card.word, "Zqxjkv");
    assert!(
        card.suggestions.is_empty(),
        "the dictionary has nothing to put in its place: {:?}",
        card.suggestions
    );
    assert_eq!(
        selected(&app),
        card.add_row(),
        "so the card opens on the one row it has, which adds the word"
    );
}

#[test]
fn enter_on_the_offer_keeps_the_word_and_takes_its_marks_off_the_note() {
    let mut app = note_with_a_name();
    // The one the caret is at the end of is held back while it is being
    // typed; the one before it is marked.
    assert_eq!(misspelt(&app), ["Zqxjkv"]);
    let before = note_body(&app);
    let caret = app.draft().expect("the note").caret;

    app.update(Action::FixSpelling);
    app.update(Action::Confirm);

    assert!(app.popup().is_none(), "the card is answered and gone");
    assert_eq!(
        app.model().personal_dictionary.get("zqxjkv"),
        Some(&"Zqxjkv".to_owned()),
        "the word is held under its key, written the way it was typed"
    );
    assert!(
        app.misspellings().is_empty(),
        "and the mark goes from the far end of the note as well as from \
         the word the card was about: {:?}",
        misspelt(&app)
    );
    assert_eq!(note_body(&app), before, "the note says what it said");
    assert_eq!(
        app.draft().expect("the note is still open").caret,
        caret,
        "and the caret did not move"
    );
    assert_eq!(hint(&app), "Zqxjkv is in your dictionary from now on.");
}

#[test]
fn a_word_the_card_adds_twice_over_is_refused_and_nothing_is_written() {
    let mut app = note_with_a_name();
    app.update(Action::FixSpelling);
    app.update(Action::Confirm);
    // The word is known now, so `alt-s` has nothing to say about it. It
    // is added again through the manager, which is where a duplicate can
    // be asked for at all.
    let held = app.model().personal_dictionary.clone();

    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::PersonalDictionary);
    app.update(Action::Confirm);
    app.update(Action::Add);
    type_in(&mut app, "zqxjkv");
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "That word is already in your dictionary.");
    assert_eq!(
        app.model().personal_dictionary,
        held,
        "the same word in another case is the same word"
    );
}

#[test]
fn a_word_another_window_adds_stops_being_marked_here() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    enable_spelling(&mut app);
    app.update(Action::NotesPage);
    note_saying(&mut app, "Zqxjkv rang");
    app.update(Action::Tick);
    assert_eq!(misspelt(&app), ["Zqxjkv"]);

    let mut other = app_at(store, NOW);
    other.update(Action::SettingsPage);
    cursor_to(&mut other, SettingRow::PersonalDictionary);
    other.update(Action::Confirm);
    other.update(Action::Add);
    type_in(&mut other, "Zqxjkv");
    other.update(Action::Confirm);

    // The tick reloads the model, which is where the word arrives; the
    // note itself has not changed, so nothing but the dictionary would
    // ask for it to be checked again.
    app.update(Action::Tick);
    assert!(
        app.misspellings().is_empty(),
        "the mark goes with the reload: {:?}",
        misspelt(&app)
    );
}

// ---- the personal dictionary ----------------------------------------

/// The manager, opened the way somebody opens it: the settings page, the
/// notes group's last row, Enter.
fn dictionary_manager() -> App {
    let mut app = started();
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::PersonalDictionary);
    app.update(Action::Confirm);
    assert!(app.popup().is_some(), "the manager did not open");
    app
}

/// `a`, the word, Enter: one word in the dictionary, the way somebody
/// puts one there.
fn add_a_word(app: &mut App, word: &str) {
    app.update(Action::Add);
    type_in(app, word);
    app.update(Action::Confirm);
}

/// The words the manager lists, in the order it lists them.
fn listed(app: &App) -> Vec<String> {
    app.dictionary_rows()
        .iter()
        .map(|(_, word)| (*word).to_owned())
        .collect()
}

#[test]
fn the_notes_settings_open_the_dictionary_and_escape_gives_the_page_back() {
    let mut app = dictionary_manager();
    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::Dictionary,
            text_field: false
        },
        "the manager is a list until a word is being written"
    );

    app.update(Action::Cancel);
    assert!(app.popup().is_none());
    assert_eq!(app.page(), Page::Settings);
    assert_eq!(
        app.cursor(List::Settings),
        Some(RowId::Setting(SettingRow::PersonalDictionary)),
        "on the row it was opened from"
    );
}

#[test]
fn a_word_is_added_kept_as_it_was_typed_and_listed_where_it_sorts() {
    let mut app = dictionary_manager();
    add_a_word(&mut app, "Overgaard");
    assert_eq!(listed(&app), ["Overgaard"]);
    assert_eq!(hint(&app), "Overgaard is in your dictionary from now on.");
    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::Dictionary,
            text_field: false
        },
        "the field is put away once the word is saved"
    );

    add_a_word(&mut app, "jira");
    assert_eq!(
        listed(&app),
        ["jira", "Overgaard"],
        "sorted by the key, which is the word folded to one case"
    );
    assert_eq!(
        app.popup().expect("the manager").selected,
        0,
        "and the cursor is on the word just written"
    );
}

#[test]
fn a_word_that_is_not_one_word_is_refused_and_the_field_keeps_it() {
    let mut app = dictionary_manager();
    app.update(Action::Add);
    type_in(&mut app, "two words");
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "A dictionary word is one word.");
    assert!(listed(&app).is_empty(), "nothing was written");
    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::Dictionary,
            text_field: true
        },
        "the field is still open"
    );
    assert_eq!(
        app.popup().expect("the manager").text,
        "two words",
        "with what was typed still in it, to be corrected"
    );

    // Corrected in the field, saved from the field.
    for _ in 0..6 {
        app.update(Action::Backspace);
    }
    app.update(Action::Confirm);
    assert_eq!(listed(&app), ["two"]);
}

#[test]
fn e_opens_the_word_for_changing_and_escape_leaves_it_as_it_was() {
    let mut app = dictionary_manager();
    add_a_word(&mut app, "Jria");

    app.update(Action::Edit);
    assert_eq!(
        app.popup().expect("the manager").text,
        "Jria",
        "the field opens on the word it is about"
    );
    assert_eq!(
        app.popup().expect("the manager").caret,
        4,
        "with the caret at the end of it"
    );

    app.update(Action::Cancel);
    assert_eq!(listed(&app), ["Jria"], "Escape wrote nothing");
    assert!(
        app.popup().is_some(),
        "and the first Escape leaves the field, not the manager"
    );
    app.update(Action::Cancel);
    assert!(app.popup().is_none(), "the second leaves the manager");
}

#[test]
fn a_word_written_again_replaces_the_one_it_was() {
    let mut app = dictionary_manager();
    add_a_word(&mut app, "Jria");
    app.update(Action::Edit);
    for _ in 0..4 {
        app.update(Action::Backspace);
    }
    type_in(&mut app, "Jira");
    app.update(Action::Confirm);

    assert_eq!(listed(&app), ["Jira"], "one entry, written the new way");
    assert_eq!(
        app.model().personal_dictionary.keys().collect::<Vec<_>>(),
        ["jira"],
        "and under the key the new word makes"
    );
    assert_eq!(hint(&app), "The word is written Jira now.");
}

#[test]
fn x_removes_the_word_the_cursor_is_on_and_not_one_being_written() {
    let mut app = dictionary_manager();
    add_a_word(&mut app, "Jira");
    add_a_word(&mut app, "Overgaard");
    assert_eq!(listed(&app), ["Jira", "Overgaard"]);

    // A field open over the list is where every key types, so `x` there
    // is a letter and no word goes by accident.
    app.update(Action::Add);
    app.update(Action::Delete);
    assert_eq!(listed(&app), ["Jira", "Overgaard"], "nothing was removed");
    app.update(Action::Cancel);

    app.update(Action::Down);
    app.update(Action::Delete);
    assert_eq!(listed(&app), ["Jira"]);
    assert_eq!(hint(&app), "Overgaard is out of your dictionary.");
    assert_eq!(
        app.popup().expect("the manager").selected,
        0,
        "the cursor comes back onto the row that is left"
    );

    app.update(Action::Delete);
    assert!(listed(&app).is_empty());
    assert_eq!(
        app.popup().expect("the manager").selected,
        0,
        "and an empty list leaves it at the top"
    );
}

#[test]
fn the_dictionary_is_kept_and_read_back_by_the_next_window() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::PersonalDictionary);
    app.update(Action::Confirm);
    add_a_word(&mut app, "Overgaard");

    let next = app_at(store, NOW);
    assert_eq!(
        next.model().personal_dictionary.get("overgaard"),
        Some(&"Overgaard".to_owned()),
        "the words are rows of the database like everything else"
    );
}

#[test]
fn the_manager_works_while_the_spell_check_is_off() {
    let mut app = started();
    app.update(Action::SettingsPage);
    assert!(!app.settings().spell_check_notes());

    cursor_to(&mut app, SettingRow::PersonalDictionary);
    app.update(Action::Confirm);
    add_a_word(&mut app, "Overgaard");

    assert_eq!(
        listed(&app),
        ["Overgaard"],
        "the list of words is kept whether or not anything is checked against it"
    );
}

#[test]
fn the_dictionary_row_holds_no_value_for_h_and_l_to_step() {
    let mut app = started();
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::PersonalDictionary);
    let settings = app.settings().clone();

    app.update(Action::Right);
    app.update(Action::Left);
    assert_eq!(app.settings(), &settings, "nothing was changed");
    assert!(app.popup().is_none(), "and nothing was opened");
}

#[test]
fn a_failed_write_leaves_the_dictionary_and_the_field_as_they_were() {
    let mut app = App::new(
        Box::new(Broken(MemStore::new())),
        Box::new(Desk::here()),
        Locale::default(),
        &at(NOW),
    )
    .expect("an app");
    app.update(Action::SettingsPage);
    cursor_to(&mut app, SettingRow::PersonalDictionary);
    app.update(Action::Confirm);
    app.update(Action::Add);
    type_in(&mut app, "Overgaard");
    app.update(Action::Confirm);

    assert_eq!(hint(&app), "The change could not be saved.");
    assert!(app.model().personal_dictionary.is_empty());
    assert_eq!(
        app.popup().expect("the manager").text,
        "Overgaard",
        "and what was typed is still there to try again"
    );
}

#[test]
fn escape_leaves_the_word_as_it_was_and_gives_the_note_back() {
    let mut app = note_with_the_caret_in_teh();
    let before = note_body(&app);
    let caret = app.draft().expect("the note").caret;
    app.update(Action::FixSpelling);

    app.update(Action::Cancel);
    assert!(app.popup().is_none());
    assert_eq!(note_body(&app), before, "nothing was written");
    assert_eq!(
        app.draft().expect("the note is still open").caret,
        caret,
        "and the caret did not move"
    );
    assert_eq!(
        app.key_context(),
        KeyContext::Notes {
            pane: NotesPane::Note,
            text_field: true
        },
        "one Escape backs out of the card, not out of the note"
    );
}

#[test]
fn a_word_the_dictionary_knows_says_so_and_opens_nothing() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    // The caret is at the end of "meeting", which is a word.
    app.update(Action::FixSpelling);

    assert!(app.popup().is_none());
    assert_eq!(hint(&app), "There is no misspelt word at the caret.");
}

#[test]
fn the_setting_turned_off_says_why_there_is_nothing_to_offer() {
    let mut app = note_with_the_caret_in_teh();
    let mut off = app.settings().clone();
    off.set_spell_check_notes(false);
    app.change_settings(off);

    app.update(Action::FixSpelling);
    assert!(app.popup().is_none());
    assert_eq!(
        hint(&app),
        "Notes are not spell-checked while that setting is off."
    );
    assert_eq!(note_body(&app), "remember teh meeting");
}

#[test]
fn a_note_thrown_away_under_the_card_is_not_written_back() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    enable_spelling(&mut app);
    app.update(Action::NotesPage);
    note_saying(&mut app, "remember teh meeting");
    app.update(Action::Tick);
    for _ in 0..8 {
        app.update(Action::Left);
    }
    app.update(Action::FixSpelling);
    assert!(app.popup().is_some());

    // Another window throws the note away while the card stands over it.
    let mut other = app_at(store, NOW);
    other.update(Action::NotesPage);
    other.update(Action::Delete);
    assert_eq!(other.notes().count, 0);

    // The tick reloads, finds the note gone and drops the draft; the
    // card is then about text that is not open anywhere.
    app.update(Action::Tick);
    app.update(Action::Confirm);
    assert!(app.popup().is_none());
    assert_eq!(
        hint(&app),
        "That note changed while the card was open. Nothing was replaced."
    );
    assert_eq!(app.notes().count, 0, "and nothing was written back");
}

#[test]
fn the_word_written_in_keeps_every_byte_around_it() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    // A decomposed accent and a family emoji on either side of the
    // misspelt word: clusters that a range counted in anything else
    // would cut.
    let before = "cafe\u{301} \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466} ";
    note_saying(&mut app, &format!("{before}teh \u{1F600} cafe\u{301}"));
    // Back onto the end of "teh", over the accented word, the two
    // spaces around the emoji, and the emoji itself: seven clusters.
    for _ in 0..7 {
        app.update(Action::Left);
    }
    app.update(Action::FixSpelling);
    let chosen = spelling_card(&app).suggestions[0].clone();
    app.update(Action::Confirm);

    assert_eq!(
        note_body(&app),
        format!("{before}{chosen} \u{1F600} cafe\u{301}"),
        "only the word's own bytes were replaced"
    );
    app.update(Action::Tick);
    let note = note_cursor(&app).expect("the note");
    assert_eq!(
        app.model().note(note).expect("the row").body,
        note_body(&app),
        "and the tick wrote what is on screen, byte for byte"
    );
}

#[test]
fn word_steps_handle_spaces_punctuation_unicode_and_edges() {
    for (text, stops) in [
        ("", vec![0]),
        ("   ", vec![0, 3]),
        ("one  two\nthree", vec![0, 5, 9, 14]),
        ("2026-09-07", vec![0, 4, 5, 7, 8, 10]),
        ("cafe\u{301} 👨‍👩‍👧‍👦 blå", vec![0, 5, 7, 10]),
    ] {
        for pair in stops.windows(2) {
            assert_eq!(word_caret(text, pair[0], true), pair[1], "{text:?}");
            assert_eq!(word_caret(text, pair[1], false), pair[0], "{text:?}");
        }
        assert_eq!(word_caret(text, 0, false), 0);
        assert_eq!(word_caret(text, glyphs(text), true), glyphs(text));
    }
    assert_eq!(word_caret("hello world", 2, false), 0);
    assert_eq!(word_caret("hello world", 2, true), 6);
    assert_eq!(word_caret("one   two", 4, false), 0);
    assert_eq!(word_caret("one   two", 4, true), 6);
}

#[test]
fn deleting_words_preserves_boundaries_unicode_and_text_after_the_caret() {
    for (text, caret, expected, next) in [
        ("one two", 7, "one ", 4),
        ("one   ", 6, "", 0),
        ("one/two", 4, "onetwo", 3),
        ("hello world", 2, "llo world", 0),
        ("cafe\u{301} 👨‍👩‍👧‍👦", 6, "cafe\u{301} ", 5),
        ("cafe\u{301}", 4, "", 0),
        ("one\ntwo", 4, "two", 0),
        ("one", 0, "one", 0),
        ("", 0, "", 0),
    ] {
        let mut app = started();
        app.update(Action::Add);
        for ch in text.chars() {
            app.update(Action::Insert(ch));
        }
        app.set_caret(caret);
        app.update(Action::DeleteWordBackward);
        let editor = app.editor().unwrap();
        assert_eq!(editor.text, expected, "{text:?}");
        assert_eq!(editor.caret, next, "{text:?}");
    }
}

#[test]
fn deleting_a_word_removes_only_the_selection_and_updates_popup_filtering() {
    let mut app = started();
    app.update(Action::Search);
    for ch in "one two".chars() {
        app.update(Action::Insert(ch));
    }
    app.popup.as_mut().unwrap().selected = 3;
    app.update(Action::SelectLeft);
    app.update(Action::DeleteWordBackward);
    assert_eq!(app.popup.as_ref().unwrap().text, "one tw");
    assert_eq!(app.popup.as_ref().unwrap().caret, 6);
    assert_eq!(app.popup.as_ref().unwrap().selected, 0);
    assert!(app.selection().is_none());
    app.update(Action::DeleteWordBackward);
    assert_eq!(app.popup.as_ref().unwrap().text, "one ");
}

#[test]
fn word_navigation_moves_task_popup_and_note_carets_without_editing() {
    let mut app = started();
    app.update(Action::Add);
    for ch in "one two".chars() {
        app.update(Action::Insert(ch));
    }
    app.update(Action::WordLeft);
    assert_eq!(app.editor().unwrap().caret, 4);
    app.update(Action::Insert('X'));
    assert_eq!(app.editor().unwrap().text, "one Xtwo");
    app.update(Action::Cancel);

    app.update(Action::Search);
    for ch in "one two".chars() {
        app.update(Action::Insert(ch));
    }
    app.update(Action::WordLeft);
    assert_eq!(app.popup.as_ref().unwrap().caret, 4);
    app.update(Action::WordRight);
    assert_eq!(app.popup.as_ref().unwrap().caret, 7);
    assert_eq!(app.popup.as_ref().unwrap().text, "one two");
    app.update(Action::Cancel);

    app.update(Action::NotesPage);
    note_saying(&mut app, "one\ntwo three");
    note_pane(&mut app, 6, 2);
    app.update(Action::WordLeft);
    assert_eq!(app.draft().unwrap().caret, 8);
    app.update(Action::WordLeft);
    assert_eq!(app.draft().unwrap().caret, 4);
    app.update(Action::WordLeft);
    assert_eq!(app.draft().unwrap().caret, 0);
    app.update(Action::WordRight);
    let draft = app.draft().unwrap();
    assert_eq!(draft.caret, 4);
    assert_eq!(draft.wanted, None);
    assert_eq!(draft.affinity, Affinity::AfterTheBreak);
    assert_eq!(draft.text, "one\ntwo three");
}

// ---- UX polish regressions ------------------------------------------

#[test]
fn informational_rows_in_mixed_review_cannot_be_acknowledged() {
    let (original, copy) = with_a_recurring_copy();
    let mut model = original.model().clone();
    let mut due = model.task(copy).unwrap().clone();
    due.id = 99;
    due.title = "Due backlog task".to_owned();
    due.day = None;
    due.due_on = Some(on(NOW_DAY));
    due.schedule_id = None;
    due.scheduled_on = None;
    model.tasks.insert(due.id, due);
    model.meta.remove("review_on");
    let mut app = app_at(MemStore::holding(model), NOW);
    assert_eq!(app.review().unwrap().progress(), (0, 1));
    app.update(Action::Down);
    assert_eq!(app.cursor(List::Review), Some(RowId::Task(copy)));
    assert!(matches!(
        app.page_context(),
        KeyContext::Review { asks: false, .. }
    ));
    app.update(Action::Keep);
    assert_eq!(app.review().unwrap().decision(copy), None);
    assert_eq!(app.cursor(List::Review), Some(RowId::Task(copy)));
    app.update(Action::Up);
    assert!(matches!(
        app.page_context(),
        KeyContext::Review { asks: true, .. }
    ));
    app.update(Action::Keep);
    assert_eq!(app.review().unwrap().decision(99), Some(Decided::Kept));
}

#[test]
fn failed_quick_add_keeps_the_title_and_caret() {
    let mut app = App::new(
        Box::new(Broken(MemStore::new())),
        Box::new(Desk::here()),
        Locale::default(),
        &at(NOW),
    )
    .unwrap();
    app.update(Action::Add);
    type_in(&mut app, "Call café");
    app.update(Action::Left);
    let before = app.editor().unwrap().clone();
    app.update(Action::Confirm);
    assert_eq!(app.editor(), Some(&before));
    assert!(app.message().is_some());
    assert!(app.model().tasks.is_empty());
}

#[test]
fn note_undo_survives_autosave_and_reopen_without_undoing_task_decisions() {
    let mut app = started();
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "original");
    app.update(Action::Tick);
    app.update(Action::Cancel);
    app.update(Action::Confirm);
    let decisions = app.model().undo.len();
    type_in(&mut app, " changed");
    app.update(Action::Tick);
    app.update(Action::UndoText);
    assert_eq!(app.draft().unwrap().text, "original");
    app.update(Action::Tick);
    assert_eq!(app.model().note(note).unwrap().body, "original");
    app.update(Action::Cancel);
    app.update(Action::Confirm);
    app.update(Action::RedoText);
    assert_eq!(app.draft().unwrap().text, "original changed");
    assert_eq!(app.model().undo.len(), decisions);
}

#[test]
fn note_replacement_cut_and_new_typing_have_safe_text_history() {
    let mut app = started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "café 界");
    app.update(Action::SelectAll);
    app.paste(Ok("replacement\nline".to_owned()));
    app.update(Action::Tick);
    app.update(Action::UndoText);
    assert_eq!(app.draft().unwrap().text, "café 界");
    assert_eq!(app.selection(), Some(0..6));
    app.update(Action::RedoText);
    assert_eq!(app.draft().unwrap().text, "replacement\nline");
    app.update(Action::SelectAll);
    app.copied_selection(true, Ok(()));
    app.update(Action::UndoText);
    assert_eq!(app.draft().unwrap().text, "replacement\nline");
    app.update(Action::Insert('X'));
    app.update(Action::RedoText);
    assert_eq!(app.draft().unwrap().text, "X", "a new edit discards redo");
}

#[test]
fn external_note_changes_reset_local_text_history() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "mine");
    app.update(Action::Tick);
    elsewhere(
        &store,
        Command::EditNote {
            note,
            body: "external".to_owned(),
        },
    );
    app.update(Action::Tick);
    app.update(Action::UndoText);
    assert_eq!(app.draft().unwrap().text, "external");
    assert!(app.message().unwrap().text.contains("No note edit"));
}

#[test]
fn recovery_note_inherits_local_undo_without_overwriting_the_external_version() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "baseline");
    app.update(Action::Tick);
    app.update(Action::Left);
    app.update(Action::Right);
    type_in(&mut app, " local");
    elsewhere(
        &store,
        Command::EditNote {
            note,
            body: "external".to_owned(),
        },
    );
    app.update(Action::Tick);
    let recovery = app.draft().unwrap().note;
    assert_ne!(recovery, note);
    app.update(Action::UndoText);
    app.update(Action::Tick);
    assert_eq!(app.model().note(recovery).unwrap().body, "baseline");
    assert_eq!(app.model().note(note).unwrap().body, "external");
    app.update(Action::RedoText);
    assert_eq!(app.draft().unwrap().text, "baseline local");
}

#[test]
fn spelling_replacements_are_single_undoable_note_edits() {
    let mut app = spell_started();
    app.update(Action::NotesPage);
    note_saying(&mut app, "teh");
    app.update(Action::FixSpelling);
    let replacement = spelling_card(&app).suggestions[0].clone();
    app.update(Action::Confirm);
    assert_eq!(app.draft().unwrap().text, replacement);
    app.update(Action::Tick);
    app.update(Action::UndoText);
    assert_eq!(app.draft().unwrap().text, "teh");
    app.update(Action::RedoText);
    assert_eq!(app.draft().unwrap().text, replacement);
}
