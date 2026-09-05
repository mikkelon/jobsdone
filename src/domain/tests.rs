use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/// An in-memory `Store`: a `Model` and `Model::apply`.
///
/// A clone is another handle on the same data, which is how a test plays
/// the part of a second window. It is `pub(crate)` because the tests of
/// every module that holds a `Box<dyn Store>` drive it through this.
#[derive(Clone, Default)]
pub(crate) struct MemStore {
    shared: Rc<RefCell<Shared>>,
}

#[derive(Default)]
struct Shared {
    model: Model,
    version: u64,
}

impl MemStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A store that already holds something, for a test of loading.
    pub(crate) fn holding(model: Model) -> Self {
        MemStore {
            shared: Rc::new(RefCell::new(Shared { model, version: 0 })),
        }
    }
}

impl Store for MemStore {
    fn load(&self) -> Result<Model, StoreError> {
        Ok(self.shared.borrow().model.clone())
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        let mut shared = self.shared.borrow_mut();
        shared.model.apply(change);
        // SQLite moves data_version only for a write from another
        // connection; every handle here shares one counter, so a commit
        // always moves it. The application re-reads the version after its
        // own commit either way.
        shared.version += 1;
        Ok(())
    }

    fn version(&self) -> Result<u64, StoreError> {
        Ok(self.shared.borrow().version)
    }
}

// ---- the harness -----------------------------------------------------

/// The length the application holds the undo stack to. The domain does
/// not choose the number; a test has to pick one.
const UNDO_CAP: usize = 50;

fn at(text: &str) -> Zoned {
    text.parse().expect("a zoned timestamp")
}

fn on(text: &str) -> Date {
    text.parse().expect("a civil date")
}

/// A clock, a store and the model loaded from it: what the application
/// holds, minus the screen.
struct World {
    store: MemStore,
    model: Model,
    now: Zoned,
}

impl World {
    /// `now` is a bare local time; the zone is fixed so that a test never
    /// depends on the machine it runs on.
    fn at(now: &str) -> World {
        World {
            store: MemStore::new(),
            model: Model::empty(),
            now: at(&format!("{now}+02:00[Europe/Copenhagen]")),
        }
    }

    fn clock(&mut self, now: &str) {
        self.now = at(&format!("{now}+02:00[Europe/Copenhagen]"));
    }

    fn today(&self) -> Date {
        working_day(&self.now)
    }

    /// One command, committed and loaded back, the way the application
    /// does it.
    fn run(&mut self, command: Command) -> Result<(), Rejected> {
        let change = apply(&self.model, command, &self.now, UNDO_CAP)?;
        self.commit(&change);
        Ok(())
    }

    fn must(&mut self, command: Command) {
        if let Err(Rejected(why)) = self.run(command.clone()) {
            panic!("{command:?} was refused: {why}");
        }
    }

    fn refuse(&mut self, command: Command) -> String {
        match self.run(command.clone()) {
            Ok(()) => panic!("{command:?} was allowed"),
            Err(Rejected(why)) => why,
        }
    }

    fn undo(&mut self) -> Undone {
        let undone = undo(&self.model, &self.now).expect("something to undo");
        self.commit(&undone.change);
        undone
    }

    fn generate(&mut self) {
        let change = generate_copies(&self.model, &self.now);
        self.commit(&change);
    }

    fn start_review(&mut self) -> bool {
        match start_review(&self.model, self.today()) {
            Some(change) => {
                self.commit(&change);
                true
            }
            None => false,
        }
    }

    fn commit(&mut self, change: &Change) {
        self.store.commit(change).expect("the in-memory store");
        self.model = self.store.load().expect("the in-memory store");
    }

    // ---- reading back ------------------------------------------------

    fn add(&mut self, title: &str, place: Place) -> Id {
        self.must(Command::AddTask {
            title: title.to_owned(),
            place,
        });
        self.id(title.trim())
    }

    /// The newest live task with a title, which is how a test names one.
    fn id(&self, title: &str) -> Id {
        self.model
            .tasks
            .values()
            .filter(|task| task.is_live() && task.title == title)
            .map(|task| task.id)
            .next_back()
            .unwrap_or_else(|| panic!("no live task called {title:?}"))
    }

    fn task(&self, id: Id) -> &Task {
        self.model.task(id).expect("a task")
    }

    fn day(&self, day: &str) -> DayView {
        day_view(&self.model, on(day), self.today())
    }

    fn backlog(&self) -> BacklogView {
        backlog_view(&self.model, self.today())
    }

    fn pile(&self) -> Pile {
        pile(&self.model, self.today())
    }

    fn surfaced(&self) -> Surfaced {
        surfaced(&self.model, self.today())
    }

    fn pile_again(&self, opened: &Pile) -> Pile {
        pile_again(&self.model, self.today(), opened)
    }

    fn surfaced_again(&self, opened: &Surfaced) -> Surfaced {
        surfaced_again(&self.model, self.today(), opened)
    }
}

fn titles(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.title.as_str()).collect()
}

fn day(date: &str) -> Place {
    Place::Day(on(date))
}

/// The model as every view sees it. Undo restores this; the rows a
/// delete leaves behind are invisible either way (DOMAIN.md section 11).
fn visible(model: &Model) -> Model {
    let mut model = model.clone();
    let gone: Vec<Id> = model
        .tasks
        .values()
        .filter(|task| !task.is_live())
        .map(|task| task.id)
        .collect();
    model.tasks.retain(|_, task| task.is_live());
    model.placements.retain(|(task, _), _| !gone.contains(task));
    model.notes.retain(|_, note| note.is_live());
    model
}

// ---- time ------------------------------------------------------------

#[test]
fn a_day_begins_at_five() {
    let late = at("2026-09-05T01:30:00+02:00[Europe/Copenhagen]");
    assert_eq!(working_day(&late).to_string(), "2026-09-04");

    let early = at("2026-09-05T05:00:00+02:00[Europe/Copenhagen]");
    assert_eq!(working_day(&early).to_string(), "2026-09-05");
}

#[test]
fn a_task_closed_after_midnight_belongs_to_the_evening_it_started_in() {
    let mut world = World::at("2026-09-04T22:00:00");
    let id = world.add("Ship the release", day("2026-09-04"));

    world.clock("2026-09-05T01:30:00");
    world.must(Command::Close { task: id });

    // The close is on Friday's day, at Saturday's small hours.
    let friday = world.day("2026-09-04");
    assert_eq!(titles(&friday.done), ["Ship the release"]);
    assert!(friday.done[0].closed_on_this_day);
}

#[test]
fn the_model_takes_a_committed_change() {
    let mut store = MemStore::new();
    let change = Change {
        writes: vec![Write::SetMeta {
            key: "review_on".into(),
            value: "2026-09-05".into(),
        }],
    };

    store.commit(&change).expect("commit");

    let model = store.load().expect("load");
    assert_eq!(
        model.meta.get("review_on").map(String::as_str),
        Some("2026-09-05")
    );
    assert_eq!(store.version().expect("version"), 1);
}

// ---- tasks -----------------------------------------------------------

#[test]
fn a_task_is_a_title_and_where_it_lives() {
    let mut world = World::at("2026-09-07T09:00:00");
    let planned = world.add("Reply to the tender questions", day("2026-09-07"));
    let parked = world.add("Clean out the garage", Place::Backlog);

    assert_eq!(world.task(planned).day, Some(on("2026-09-07")));
    assert_eq!(world.task(parked).day, None);
    assert!(world.task(planned).is_open());
    assert!(!world.task(planned).focus);
}

#[test]
fn a_title_is_trimmed_and_one_line() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("   Book dentist  ", Place::Backlog);
    assert_eq!(world.task(id).title, "Book dentist");

    assert_eq!(
        world.refuse(Command::AddTask {
            title: "   ".to_owned(),
            place: Place::Backlog,
        }),
        "A task needs a title."
    );
    assert_eq!(
        world.refuse(Command::AddTask {
            title: "two\nlines".to_owned(),
            place: Place::Backlog,
        }),
        "A title is one line."
    );
}

#[test]
fn a_task_lives_in_exactly_one_place_and_moves_freely() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Clean out the garage", Place::Backlog);

    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });
    assert_eq!(world.task(id).day, Some(on("2026-09-07")));
    assert!(world.backlog().ordinary.is_empty());

    world.must(Command::Move {
        task: id,
        place: Place::Backlog,
    });
    assert_eq!(world.task(id).day, None);
    assert_eq!(titles(&world.day("2026-09-07").focus), Vec::<&str>::new());
    assert_eq!(titles(&world.backlog().ordinary), ["Clean out the garage"]);

    assert_eq!(
        world.refuse(Command::Move {
            task: id,
            place: Place::Backlog,
        }),
        "The task is already there."
    );
}

#[test]
fn a_closed_task_stays_where_it_was_done() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Pay the electricity bill", day("2026-09-07"));
    world.must(Command::Close { task: id });

    assert_eq!(
        world.refuse(Command::Move {
            task: id,
            place: Place::Backlog,
        }),
        "A closed task stays where it was done."
    );
}

#[test]
fn closing_a_backlog_task_puts_it_on_today_first() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Chase the hosting invoice", Place::Backlog);

    world.must(Command::Close { task: id });

    assert_eq!(world.task(id).day, Some(on("2026-09-07")));
    let today = world.day("2026-09-07");
    assert_eq!(titles(&today.done), ["Chase the hosting invoice"]);
    assert!(world.model.placement(id, on("2026-09-07")).is_some());
}

#[test]
fn focus_is_a_flag_on_a_day_with_no_cap() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("Ship invoice export", day("2026-09-07"));
    let second = world.add("Reply to the tender questions", day("2026-09-07"));
    world.add("Review Anna's PR", day("2026-09-07"));

    world.must(Command::SetFocus {
        task: first,
        focus: true,
    });
    world.must(Command::SetFocus {
        task: second,
        focus: true,
    });

    let today = world.day("2026-09-07");
    assert_eq!(
        titles(&today.focus),
        ["Ship invoice export", "Reply to the tender questions"]
    );
    assert_eq!(titles(&today.plan), ["Review Anna's PR"]);

    let parked = world.add("Clean out the garage", Place::Backlog);
    assert_eq!(
        world.refuse(Command::SetFocus {
            task: parked,
            focus: true,
        }),
        "Focus is for tasks on a day."
    );
}

#[test]
fn closing_keeps_the_focus_flag_and_reopening_returns_the_task_to_focus() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Reply to the tender questions", day("2026-09-07"));
    world.must(Command::SetFocus {
        task: id,
        focus: true,
    });

    world.must(Command::Close { task: id });
    let closed = world.day("2026-09-07");
    assert!(closed.focus.is_empty());
    assert!(closed.done[0].was_focus);

    world.must(Command::Reopen { task: id });
    let reopened = world.day("2026-09-07");
    assert_eq!(titles(&reopened.focus), ["Reply to the tender questions"]);
}

// ---- places and order ------------------------------------------------

#[test]
fn a_task_arriving_in_a_place_goes_to_the_end() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("First", day("2026-09-07"));
    world.add("Second", day("2026-09-07"));
    let moved = world.add("Third", Place::Backlog);

    world.must(Command::Move {
        task: moved,
        place: day("2026-09-07"),
    });

    assert_eq!(
        titles(&world.day("2026-09-07").plan),
        ["First", "Second", "Third"]
    );
    assert_eq!(world.task(moved).position, 2);
}

#[test]
fn the_order_of_a_day_is_manual_and_remembered() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("First", day("2026-09-07"));
    let second = world.add("Second", day("2026-09-07"));
    world.add("Third", day("2026-09-07"));

    world.must(Command::Reorder {
        task: second,
        position: 0,
    });
    assert_eq!(
        titles(&world.day("2026-09-07").plan),
        ["Second", "First", "Third"]
    );

    world.must(Command::Reorder {
        task: second,
        position: 2,
    });
    assert_eq!(
        titles(&world.day("2026-09-07").plan),
        ["First", "Third", "Second"]
    );
}

#[test]
fn a_reorder_past_the_end_is_clamped_to_it() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("First", day("2026-09-07"));
    world.add("Second", day("2026-09-07"));

    world.must(Command::Reorder {
        task: first,
        position: 99,
    });

    assert_eq!(titles(&world.day("2026-09-07").plan), ["Second", "First"]);
}

#[test]
fn delete_and_move_leave_the_place_they_left_dense() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("First", day("2026-09-07"));
    let second = world.add("Second", day("2026-09-07"));
    let third = world.add("Third", day("2026-09-07"));

    world.must(Command::DeleteTask { task: first });
    assert_eq!(world.task(second).position, 0);
    assert_eq!(world.task(third).position, 1);

    world.must(Command::Move {
        task: second,
        place: Place::Backlog,
    });
    assert_eq!(world.task(third).position, 0);
    assert_eq!(world.task(second).position, 0);
}

#[test]
fn a_closed_task_keeps_its_position_and_reopening_sends_it_to_the_end() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("First", day("2026-09-07"));
    world.add("Second", day("2026-09-07"));
    world.add("Third", day("2026-09-07"));

    world.must(Command::Close { task: first });
    assert_eq!(world.task(first).position, 0);

    world.must(Command::Reopen { task: first });
    assert_eq!(world.task(first).position, 2);
    assert_eq!(
        titles(&world.day("2026-09-07").plan),
        ["Second", "Third", "First"]
    );
}

#[test]
fn positions_are_one_sequence_whatever_the_groups_a_screen_draws() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("First", day("2026-09-07"));
    let second = world.add("Second", day("2026-09-07"));
    let third = world.add("Third", day("2026-09-07"));

    world.must(Command::SetFocus {
        task: second,
        focus: true,
    });
    world.must(Command::Close { task: third });

    let positions: Vec<usize> = [first, second, third]
        .iter()
        .map(|id| world.task(*id).position)
        .collect();
    assert_eq!(positions, [0, 1, 2]);
}

// ---- the day view ----------------------------------------------------

#[test]
fn closed_tasks_drop_to_done_in_the_order_they_were_closed() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("First", day("2026-09-07"));
    let second = world.add("Second", day("2026-09-07"));

    world.clock("2026-09-07T11:00:00");
    world.must(Command::Close { task: second });
    world.clock("2026-09-07T12:00:00");
    world.must(Command::Close { task: first });

    assert_eq!(titles(&world.day("2026-09-07").done), ["Second", "First"]);
}

#[test]
fn a_task_moved_off_a_day_leaves_its_pointer_the_moment_it_is_moved() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Chase the hosting invoice", day("2026-09-07"));

    world.must(Command::Move {
        task: id,
        place: day("2026-09-08"),
    });

    let monday = world.day("2026-09-07");
    assert!(monday.plan.is_empty());
    assert_eq!(titles(&monday.moved), ["Chase the hosting invoice"]);
    assert_eq!(monday.moved[0].place, day("2026-09-08"));
    assert_eq!(monday.counts.moved, 1);
}

#[test]
fn moving_a_task_never_rewrites_what_was_planned() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Book the venue", day("2026-09-07"));

    world.must(Command::Move {
        task: id,
        place: day("2026-09-08"),
    });
    world.clock("2026-09-08T09:00:00");
    world.must(Command::Move {
        task: id,
        place: day("2026-09-09"),
    });

    // Monday and Tuesday both point at where the task is now.
    for date in ["2026-09-07", "2026-09-08"] {
        let view = world.day(date);
        assert_eq!(titles(&view.moved), ["Book the venue"]);
        assert_eq!(view.moved[0].place, day("2026-09-09"));
    }
}

#[test]
fn a_day_is_laid_out_the_same_way_whether_or_not_it_is_today() {
    let mut world = World::at("2026-09-07T09:00:00");
    let focus = world.add("Ship invoice export", day("2026-09-07"));
    world.add("Review Anna's PR", day("2026-09-07"));
    let done = world.add("Morning review", day("2026-09-07"));
    let moved = world.add("Chase the hosting invoice", day("2026-09-07"));
    world.must(Command::SetFocus {
        task: focus,
        focus: true,
    });
    world.must(Command::Close { task: done });
    world.must(Command::Move {
        task: moved,
        place: day("2026-09-08"),
    });

    let as_today = world.day("2026-09-07");
    world.clock("2026-09-11T09:00:00");
    let as_history = world.day("2026-09-07");

    assert_eq!(titles(&as_today.focus), titles(&as_history.focus));
    assert_eq!(titles(&as_today.plan), titles(&as_history.plan));
    assert_eq!(titles(&as_today.done), titles(&as_history.done));
    assert_eq!(titles(&as_today.moved), titles(&as_history.moved));
    assert_eq!(as_today.counts, as_history.counts);

    // Only the annotations that speak of today move.
    assert!(!as_today.plan[0].on_the_pile);
    assert!(as_history.plan[0].on_the_pile);
}

#[test]
fn a_day_counts_what_was_planned_kept_done_and_moved() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Open one", day("2026-09-07"));
    world.add("Open two", day("2026-09-07"));
    let done = world.add("Closed one", day("2026-09-07"));
    let moved = world.add("Moved one", day("2026-09-07"));
    world.must(Command::Close { task: done });
    world.must(Command::Move {
        task: moved,
        place: Place::Backlog,
    });

    let counts = world.day("2026-09-07").counts;
    assert_eq!(
        counts,
        DayCounts {
            planned: 4,
            open: 2,
            done: 1,
            moved: 1,
        }
    );
}

#[test]
fn a_task_pulled_from_the_backlog_today_is_marked_as_such() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Fix the flaky migration test", Place::Backlog);
    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });

    assert!(world.day("2026-09-07").plan[0].from_backlog);

    // The same row on a day it was placed for in advance says nothing.
    let later = world.add("Renew passport", Place::Backlog);
    world.must(Command::Move {
        task: later,
        place: day("2026-09-11"),
    });
    assert!(!world.day("2026-09-11").plan[0].from_backlog);
}

#[test]
fn a_task_closed_from_the_review_shows_the_date_not_the_time() {
    let mut world = World::at("2026-09-04T09:00:00");
    let id = world.add("Send the invoice to Nordic Ltd", day("2026-09-04"));

    world.clock("2026-09-07T08:12:00");
    world.must(Command::Close { task: id });

    let friday = world.day("2026-09-04");
    assert!(!friday.done[0].closed_on_this_day);
    assert!(world.day("2026-09-07").done.is_empty());
}

// ---- placements ------------------------------------------------------

#[test]
fn a_placement_is_written_once_and_keeps_the_first_arrival() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Fix the flaky migration test", Place::Backlog);

    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });
    let first = world
        .model
        .placement(id, on("2026-09-07"))
        .expect("a placement")
        .clone();
    assert_eq!(first.from_place, FromPlace::Backlog);

    world.must(Command::Move {
        task: id,
        place: day("2026-09-08"),
    });
    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });

    assert_eq!(world.model.placement(id, on("2026-09-07")), Some(&first));
}

#[test]
fn the_backlog_has_no_placements() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Clean out the garage", Place::Backlog);
    assert!(world.model.placements.is_empty());

    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });
    world.must(Command::Move {
        task: id,
        place: Place::Backlog,
    });

    // The day it left keeps its row; the backlog writes none.
    assert_eq!(world.model.placements.len(), 1);
    assert_eq!(
        titles(&world.day("2026-09-07").moved),
        ["Clean out the garage"]
    );
}

#[test]
fn the_day_list_is_the_days_that_have_a_placement() {
    let mut world = World::at("2026-09-07T09:00:00");
    let done = world.add("Weekly planning", day("2026-09-07"));
    world.add("Call the accountant about VAT", day("2026-09-07"));
    let moved = world.add("Order new office chair", day("2026-09-07"));
    world.add("Reply to Anna", day("2026-09-04"));
    world.must(Command::Close { task: done });
    world.must(Command::Move {
        task: moved,
        place: Place::Backlog,
    });

    let list = day_list(&world.model);
    let days: Vec<String> = list.days.iter().map(|row| row.day.to_string()).collect();
    assert_eq!(days, ["2026-09-07", "2026-09-04"]);
    assert_eq!(
        (list.days[0].kept, list.days[0].done, list.days[0].open),
        (2, 1, 1)
    );
}

// ---- the backlog, dates and waiting ----------------------------------

#[test]
fn the_backlog_shows_waiting_tasks_apart() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Migrate CI to the new runners", Place::Backlog);
    let waiting = world.add("Quote from the electrician", Place::Backlog);
    world.must(Command::SetWaiting {
        task: waiting,
        waiting: true,
    });

    let backlog = world.backlog();
    assert_eq!(titles(&backlog.ordinary), ["Migrate CI to the new runners"]);
    assert_eq!(titles(&backlog.waiting), ["Quote from the electrician"]);
    assert_eq!((backlog.open, backlog.waiting_count), (2, 1));
}

#[test]
fn flagging_a_task_on_a_day_sends_it_to_the_backlog_as_waiting() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Ship invoice export", day("2026-09-07"));
    let id = world.add("Feedback on the proposal", day("2026-09-07"));

    world.must(Command::SetWaiting {
        task: id,
        waiting: true,
    });

    assert_eq!(world.task(id).day, None);
    assert!(world.task(id).waiting);
    assert_eq!(
        titles(&world.backlog().waiting),
        ["Feedback on the proposal"]
    );
    // A move like any other: the day keeps its pointer.
    assert_eq!(
        titles(&world.day("2026-09-07").moved),
        ["Feedback on the proposal"]
    );

    // One command, one undo entry.
    assert_eq!(world.model.undo.len(), 3);
    world.undo();
    assert_eq!(world.task(id).day, Some(on("2026-09-07")));
    assert!(!world.task(id).waiting);
    assert!(world.day("2026-09-07").moved.is_empty());
}

#[test]
fn pulling_a_waiting_task_onto_a_day_clears_the_flag() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Parcel from the supplier", Place::Backlog);
    world.must(Command::SetWaiting {
        task: id,
        waiting: true,
    });

    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });

    assert!(!world.task(id).waiting);
}

#[test]
fn clearing_waiting_in_place_leaves_the_task_in_the_backlog() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Parcel from the supplier", Place::Backlog);
    world.must(Command::SetWaiting {
        task: id,
        waiting: true,
    });

    world.must(Command::SetWaiting {
        task: id,
        waiting: false,
    });

    assert!(!world.task(id).waiting);
    assert_eq!(world.task(id).day, None);
    assert_eq!(
        titles(&world.backlog().ordinary),
        ["Parcel from the supplier"]
    );
}

#[test]
fn a_date_surfaces_a_task_and_never_moves_it() {
    let mut world = World::at("2026-09-07T09:00:00");
    let due = world.add("Submit the expense report", Place::Backlog);
    let remind = world.add("Book dentist", Place::Backlog);
    world.must(Command::SetDue {
        task: due,
        date: Some(on("2026-09-07")),
    });
    world.must(Command::SetRemind {
        task: remind,
        date: Some(on("2026-09-07")),
    });

    let surfaced = world.surfaced();
    assert_eq!(titles(&surfaced.due), ["Submit the expense report"]);
    assert_eq!(titles(&surfaced.reminders), ["Book dentist"]);
    assert_eq!(surfaced.total, 2);

    // Surfacing is a prompt, not a move.
    assert_eq!(world.task(due).day, None);
    assert_eq!(world.task(remind).day, None);
    assert_eq!(world.backlog().ordinary.len(), 2);
}

#[test]
fn a_due_task_surfaces_at_every_review_until_it_is_acted_on() {
    let mut world = World::at("2026-09-04T09:00:00");
    let id = world.add("Migrate CI to the new runners", Place::Backlog);
    world.must(Command::SetDue {
        task: id,
        date: Some(on("2026-09-03")),
    });

    world.start_review();
    assert_eq!(world.surfaced().due.len(), 1);
    assert!(world.surfaced().due[0].due.expect("a due chip").overdue);

    world.clock("2026-09-07T09:00:00");
    world.start_review();
    assert_eq!(world.surfaced().due.len(), 1);

    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });
    assert!(world.surfaced().due.is_empty());
}

#[test]
fn a_reminder_surfaces_once_and_a_weekend_one_is_seen_on_monday() {
    let mut world = World::at("2026-09-04T09:00:00");
    let id = world.add("Feedback on the proposal", Place::Backlog);
    world.must(Command::SetRemind {
        task: id,
        date: Some(on("2026-09-05")),
    });

    // Friday's review is before the reminder's Saturday.
    world.start_review();
    assert!(world.surfaced().reminders.is_empty());

    world.clock("2026-09-07T09:00:00");
    world.start_review();
    assert_eq!(
        titles(&world.surfaced().reminders),
        ["Feedback on the proposal"]
    );

    // Tuesday's review has seen it.
    world.clock("2026-09-08T09:00:00");
    world.start_review();
    assert!(world.surfaced().reminders.is_empty());
}

#[test]
fn waiting_suppresses_a_due_date_but_not_a_reminder() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Feedback on the proposal", Place::Backlog);
    world.must(Command::SetDue {
        task: id,
        date: Some(on("2026-09-07")),
    });
    world.must(Command::SetRemind {
        task: id,
        date: Some(on("2026-09-07")),
    });
    world.must(Command::SetWaiting {
        task: id,
        waiting: true,
    });

    let surfaced = world.surfaced();
    assert!(surfaced.due.is_empty());
    assert_eq!(titles(&surfaced.reminders), ["Feedback on the proposal"]);
    assert!(surfaced.reminders[0].waiting);
}

#[test]
fn a_dated_task_on_a_day_is_already_planned_and_prompts_again_in_the_backlog() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Submit the expense report", Place::Backlog);
    world.must(Command::SetDue {
        task: id,
        date: Some(on("2026-09-07")),
    });

    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });
    assert!(world.surfaced().due.is_empty());
    assert_eq!(world.task(id).due_on, Some(on("2026-09-07")));

    world.must(Command::Move {
        task: id,
        place: Place::Backlog,
    });
    assert_eq!(titles(&world.surfaced().due), ["Submit the expense report"]);
}

#[test]
fn overdue_due_dates_surface_before_the_rest() {
    let mut world = World::at("2026-09-07T09:00:00");
    let today = world.add("Submit the expense report", Place::Backlog);
    let over = world.add("Migrate CI to the new runners", Place::Backlog);
    world.must(Command::SetDue {
        task: today,
        date: Some(on("2026-09-07")),
    });
    world.must(Command::SetDue {
        task: over,
        date: Some(on("2026-09-03")),
    });

    assert_eq!(
        titles(&world.surfaced().due),
        ["Migrate CI to the new runners", "Submit the expense report"]
    );
}

// ---- the pile --------------------------------------------------------

#[test]
fn the_pile_is_every_open_task_from_a_day_that_has_passed() {
    let mut world = World::at("2026-09-01T09:00:00");
    world.add("Call the accountant about VAT", day("2026-09-01"));
    let done = world.add("Weekly planning", day("2026-09-01"));
    world.must(Command::Close { task: done });
    world.clock("2026-09-04T09:00:00");
    world.add("Fix the flaky migration test", day("2026-09-04"));
    world.add("Not yet", day("2026-09-11"));
    world.add("Nor this", Place::Backlog);

    world.clock("2026-09-07T09:00:00");
    let pile = world.pile();

    assert_eq!(pile.total, 2);
    let days: Vec<String> = pile.days.iter().map(|day| day.day.to_string()).collect();
    assert_eq!(days, ["2026-09-04", "2026-09-01"]);
    assert_eq!(pile.days[0].age, 3);
    assert_eq!(pile.days[1].age, 6);
    assert_eq!(
        titles(&pile.days[1].rows),
        ["Call the accountant about VAT"]
    );
}

#[test]
fn nothing_is_carried_over_and_a_pile_row_leaves_only_when_it_is_dealt_with() {
    let mut world = World::at("2026-09-01T09:00:00");
    let id = world.add("Order new office chair", day("2026-09-01"));

    // Weeks later, the task is still on the day it was planned for.
    world.clock("2026-09-21T09:00:00");
    world.generate();
    assert_eq!(world.task(id).day, Some(on("2026-09-01")));
    assert_eq!(world.pile().total, 1);
    assert_eq!(world.pile().days[0].age, 20);
    assert!(world.day("2026-09-01").plan[0].on_the_pile);

    world.must(Command::Move {
        task: id,
        place: day("2026-09-21"),
    });
    assert_eq!(world.pile().total, 0);
}

#[test]
fn a_waiting_task_is_never_on_the_pile() {
    let mut world = World::at("2026-09-01T09:00:00");
    let id = world.add("Parcel from the supplier", day("2026-09-01"));
    world.must(Command::SetWaiting {
        task: id,
        waiting: true,
    });

    world.clock("2026-09-07T09:00:00");
    assert_eq!(world.pile().total, 0);
}

// ---- schedules and copies --------------------------------------------

fn workdays() -> Rule {
    Rule::Workdays
}

#[test]
fn a_schedule_takes_its_title_from_the_task_and_links_it_as_the_first_copy() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));

    world.must(Command::CreateSchedule {
        task: id,
        rule: workdays(),
    });

    let task = world.task(id).clone();
    let schedule = world
        .model
        .schedule(task.schedule_id.expect("a schedule"))
        .expect("the schedule");
    assert_eq!(schedule.title, "Write standup notes");
    assert_eq!(schedule.generated_through, on("2026-09-07"));
    assert_eq!(task.scheduled_on, Some(on("2026-09-07")));
    assert_eq!(task.day, Some(on("2026-09-07")));

    assert_eq!(
        world.refuse(Command::CreateSchedule {
            task: id,
            rule: Rule::Daily,
        }),
        "That task already repeats."
    );
}

#[test]
fn a_schedule_created_on_a_backlog_task_starts_from_today() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Sort photo backups", Place::Backlog);
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });

    assert_eq!(world.task(id).scheduled_on, Some(on("2026-09-07")));
    assert_eq!(world.task(id).day, None);
}

#[test]
fn generation_makes_one_copy_for_every_scheduled_date_since_the_last_launch() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: workdays(),
    });

    // Away for a week and a half: Tue to Fri, then Mon and Tue.
    world.clock("2026-09-15T09:00:00");
    world.generate();

    let copies: Vec<String> = world
        .model
        .tasks
        .values()
        .filter(|task| task.title == "Write standup notes")
        .filter_map(|task| task.scheduled_on)
        .map(|date| date.to_string())
        .collect();
    assert_eq!(
        copies,
        [
            "2026-09-07",
            "2026-09-08",
            "2026-09-09",
            "2026-09-10",
            "2026-09-11",
            "2026-09-14",
            "2026-09-15",
        ]
    );
    // Each on its own day, all of the past ones on the pile.
    assert_eq!(world.pile().total, 6);
    assert_eq!(world.day("2026-09-12").counts.planned, 0);
}

#[test]
fn generation_is_idempotent_and_pushes_nothing() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });
    let entries = world.model.undo.len();

    world.clock("2026-09-09T09:00:00");
    world.generate();
    let after = world.model.clone();
    world.generate();

    assert_eq!(world.model, after);
    assert_eq!(world.model.undo.len(), entries);
}

#[test]
fn a_deleted_copy_is_never_made_again() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });

    world.clock("2026-09-08T09:00:00");
    world.generate();
    let copy = world.id("Write standup notes");
    world.must(Command::DeleteTask { task: copy });

    world.clock("2026-09-09T09:00:00");
    world.generate();

    let tuesday = world.day("2026-09-08");
    assert_eq!(tuesday.counts.planned, 0);
    assert_eq!(world.day("2026-09-09").counts.planned, 1);
}

#[test]
fn a_copy_is_an_ordinary_task_from_then_on() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: first,
        rule: Rule::Daily,
    });
    world.clock("2026-09-08T09:00:00");
    world.generate();
    let second = world.id("Write standup notes");

    world.must(Command::SetFocus {
        task: second,
        focus: true,
    });
    world.must(Command::Move {
        task: second,
        place: Place::Backlog,
    });

    // The schedule and the other copy are untouched.
    assert_eq!(world.task(first).day, Some(on("2026-09-07")));
    assert!(!world.task(first).focus);
    let schedule = world.model.schedules.values().next().expect("a schedule");
    assert_eq!(schedule.generated_through, on("2026-09-08"));
    assert!(!schedule.is_stopped());
    // It keeps its link, so search still marks it.
    assert_eq!(world.task(second).schedule_id, schedule.id.into());
}

#[test]
fn an_unclosed_copy_lands_on_the_pile() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });

    world.clock("2026-09-09T09:00:00");
    world.generate();

    assert_eq!(world.pile().total, 2);
    assert!(world.pile().days[0].rows[0].repeat.is_some());
}

#[test]
fn editing_a_copys_title_renames_the_copy_or_the_schedule_too() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: first,
        rule: Rule::Daily,
    });

    // "This copy" renames the task.
    world.must(Command::EditTitle {
        task: first,
        title: "Write standup notes (long version)".to_owned(),
    });
    let schedule = world.model.schedules.values().next().expect("a schedule");
    assert_eq!(schedule.title, "Write standup notes");

    world.clock("2026-09-08T09:00:00");
    world.generate();
    assert_eq!(
        world.id("Write standup notes"),
        world.id("Write standup notes")
    );

    // "This and future copies" renames the schedule as well.
    let second = world.id("Write standup notes");
    world.must(Command::EditTitleAndFuture {
        task: second,
        title: "Standup".to_owned(),
        schedule_title: "Standup".to_owned(),
    });
    world.clock("2026-09-09T09:00:00");
    world.generate();

    assert_eq!(
        world.day("2026-09-07").plan[0].title,
        "Write standup notes (long version)"
    );
    assert_eq!(world.day("2026-09-08").plan[0].title, "Standup");
    assert_eq!(world.day("2026-09-09").plan[0].title, "Standup");
}

#[test]
fn a_title_edit_for_future_copies_needs_a_copy() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Clean out the garage", Place::Backlog);

    assert_eq!(
        world.refuse(Command::EditTitleAndFuture {
            task: id,
            title: "Tidy the garage".to_owned(),
            schedule_title: "Tidy the garage".to_owned(),
        }),
        "That task is not a recurring copy."
    );
}

#[test]
fn changing_the_rule_takes_effect_from_the_next_generation() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });
    let schedule = world.task(id).schedule_id.expect("a schedule");

    world.clock("2026-09-08T09:00:00");
    world.generate();
    world.must(Command::SetRule {
        schedule,
        rule: Rule::Weekly {
            weekdays: vec![Weekday::Fri],
        },
    });

    world.clock("2026-09-11T09:00:00");
    world.generate();

    // Tuesday's copy stays; Wednesday and Thursday never come.
    assert_eq!(world.day("2026-09-08").counts.planned, 1);
    assert_eq!(world.day("2026-09-09").counts.planned, 0);
    assert_eq!(world.day("2026-09-11").counts.planned, 1);
}

#[test]
fn stopping_a_schedule_ends_new_copies_and_leaves_the_old_ones() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });
    let schedule = world.task(id).schedule_id.expect("a schedule");

    world.clock("2026-09-08T09:00:00");
    world.generate();
    world.must(Command::StopSchedule { schedule });

    world.clock("2026-09-10T09:00:00");
    world.generate();

    assert_eq!(world.day("2026-09-08").counts.planned, 1);
    assert_eq!(world.day("2026-09-10").counts.planned, 0);
    assert!(world.backlog().schedules.is_empty());
    assert_eq!(
        world
            .model
            .schedule(schedule)
            .and_then(|schedule| schedule.stopped_on),
        Some(on("2026-09-08"))
    );
    assert_eq!(
        world.refuse(Command::StopSchedule { schedule }),
        "That repeat has stopped."
    );
}

#[test]
fn the_backlog_lists_the_live_unstopped_schedules() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: workdays(),
    });

    let schedules = world.backlog().schedules;
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].title, "Write standup notes");
    assert_eq!(schedules[0].rule, Rule::Workdays);
}

#[test]
fn a_repeat_that_never_comes_round_is_refused() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));

    assert_eq!(
        world.refuse(Command::CreateSchedule {
            task: id,
            rule: Rule::Weekly { weekdays: vec![] },
        }),
        "That repeat never comes round."
    );
}

// ---- rule dates ------------------------------------------------------

fn dates(rule: &Rule, after: &str, count: usize) -> Vec<String> {
    next_dates(rule, on(after), count)
        .iter()
        .map(|date| date.to_string())
        .collect()
}

#[test]
fn work_days_are_monday_to_friday_with_no_holidays() {
    assert_eq!(
        dates(&Rule::Workdays, "2026-09-10", 4),
        ["2026-09-11", "2026-09-14", "2026-09-15", "2026-09-16"]
    );
}

#[test]
fn every_day_is_every_day() {
    assert_eq!(
        dates(&Rule::Daily, "2026-09-10", 3),
        ["2026-09-11", "2026-09-12", "2026-09-13"]
    );
}

#[test]
fn weekly_is_a_set_of_weekdays() {
    let rule = Rule::Weekly {
        weekdays: vec![Weekday::Mon, Weekday::Thu],
    };
    assert_eq!(
        dates(&rule, "2026-09-07", 4),
        ["2026-09-10", "2026-09-14", "2026-09-17", "2026-09-21"]
    );
}

#[test]
fn monthly_clamps_a_day_to_the_months_last() {
    let rule = Rule::Monthly {
        day: MonthDay::Day(31),
    };
    assert_eq!(
        dates(&rule, "2026-01-31", 3),
        ["2026-02-28", "2026-03-31", "2026-04-30"]
    );

    let first = Rule::Monthly {
        day: MonthDay::Day(1),
    };
    assert_eq!(dates(&first, "2026-09-05", 2), ["2026-10-01", "2026-11-01"]);
}

#[test]
fn monthly_on_the_last_is_the_last_day_of_each_month() {
    let rule = Rule::Monthly {
        day: MonthDay::Last,
    };
    assert_eq!(
        dates(&rule, "2026-01-01", 3),
        ["2026-01-31", "2026-02-28", "2026-03-31"]
    );
}

#[test]
fn every_n_weeks_is_one_weekday_counted_from_a_date() {
    let rule = Rule::EveryNWeeks {
        n: 2,
        from: on("2026-09-04"),
    };
    assert_eq!(
        dates(&rule, "2026-09-04", 3),
        ["2026-09-18", "2026-10-02", "2026-10-16"]
    );
    // Asked from before the start, the start itself is the first date.
    assert_eq!(dates(&rule, "2026-08-01", 1), ["2026-09-04"]);
}

#[test]
fn a_rule_that_never_comes_round_has_no_dates() {
    assert!(dates(&Rule::Weekly { weekdays: vec![] }, "2026-09-07", 3).is_empty());
    assert!(
        dates(
            &Rule::EveryNWeeks {
                n: 0,
                from: on("2026-09-04")
            },
            "2026-09-07",
            3
        )
        .is_empty()
    );
}

#[test]
fn the_rule_shapes_are_the_json_of_domain_section_10() {
    let shapes = [
        (Rule::Workdays, r#"{"kind":"workdays"}"#),
        (Rule::Daily, r#"{"kind":"daily"}"#),
        (
            Rule::Weekly {
                weekdays: vec![Weekday::Mon, Weekday::Thu],
            },
            r#"{"kind":"weekly","weekdays":["mon","thu"]}"#,
        ),
        (
            Rule::Monthly {
                day: MonthDay::Day(15),
            },
            r#"{"kind":"monthly","day":15}"#,
        ),
        (
            Rule::Monthly {
                day: MonthDay::Last,
            },
            r#"{"kind":"monthly","day":"last"}"#,
        ),
        (
            Rule::EveryNWeeks {
                n: 2,
                from: on("2026-09-05"),
            },
            r#"{"kind":"every_n_weeks","n":2,"from":"2026-09-05"}"#,
        ),
    ];

    for (rule, json) in shapes {
        assert_eq!(serde_json::to_string(&rule).expect("json"), json);
        assert_eq!(
            serde_json::from_str::<Rule>(json).expect("a rule"),
            rule,
            "{json}"
        );
    }
}

// ---- reading a typed date --------------------------------------------

/// The date card's field, read on Friday 5 September 2025.
fn typed(text: &str) -> Option<String> {
    parse_date(text, on("2025-09-05")).map(|date| date.to_string())
}

#[test]
fn a_typed_date_is_read_the_few_ways_a_date_is_written() {
    for (text, wanted) in [
        ("2025-09-30", "2025-09-30"),
        ("30 sep", "2025-09-30"),
        ("30 september", "2025-09-30"),
        ("sep 30", "2025-09-30"),
        ("30/9", "2025-09-30"),
        ("30.9", "2025-09-30"),
        ("1/10/2026", "2026-10-01"),
        ("30 sep 2027", "2027-09-30"),
        ("  30   SEP  ", "2025-09-30"),
    ] {
        assert_eq!(typed(text).as_deref(), Some(wanted), "{text}");
    }
}

#[test]
fn a_typed_date_with_no_year_is_the_next_one_that_has_not_passed() {
    // 1 September has gone; 30 September has not.
    assert_eq!(typed("1 sep").as_deref(), Some("2026-09-01"));
    assert_eq!(typed("30 sep").as_deref(), Some("2025-09-30"));
    // A bare day is the next month that has such a day: February is
    // skipped for a 30th, not clamped to it.
    assert_eq!(typed("30").as_deref(), Some("2025-09-30"));
    assert_eq!(typed("3").as_deref(), Some("2025-10-03"));
    assert_eq!(
        parse_date("30", on("2026-01-31")).map(|date| date.to_string()),
        Some("2026-03-30".to_owned())
    );
}

#[test]
fn a_weekday_a_word_and_a_count_of_days_are_dates_too() {
    assert_eq!(typed("today").as_deref(), Some("2025-09-05"));
    assert_eq!(typed("tomorrow").as_deref(), Some("2025-09-06"));
    assert_eq!(typed("+3").as_deref(), Some("2025-09-08"));
    assert_eq!(typed("mon").as_deref(), Some("2025-09-08"));
    assert_eq!(typed("monday").as_deref(), Some("2025-09-08"));
    // A weekday is the next one, so the day it is typed on is a week
    // away rather than today.
    assert_eq!(typed("fri").as_deref(), Some("2025-09-12"));
}

#[test]
fn what_cannot_be_read_as_a_date_is_nothing() {
    for text in ["", "   ", "someday", "31 feb", "40", "9 13", "1 2 3 4", "+"] {
        assert_eq!(typed(text), None, "{text}");
    }
}

// ---- the review ------------------------------------------------------

#[test]
fn the_gate_is_written_once_a_day() {
    let mut world = World::at("2026-09-07T09:00:00");
    assert!(world.start_review());
    assert_eq!(world.model.meta_date(REVIEW_ON), Some(on("2026-09-07")));
    assert_eq!(world.model.meta_date(REVIEW_BEFORE), None);

    // A second window on the same day changes nothing.
    assert!(!world.start_review());

    world.clock("2026-09-08T09:00:00");
    assert!(world.start_review());
    assert_eq!(world.model.meta_date(REVIEW_ON), Some(on("2026-09-08")));
    assert_eq!(world.model.meta_date(REVIEW_BEFORE), Some(on("2026-09-07")));
}

#[test]
fn starting_a_review_pushes_nothing() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Something", Place::Backlog);
    let entries = world.model.undo.len();

    world.start_review();

    assert_eq!(world.model.undo.len(), entries);
}

#[test]
fn the_previous_review_date_is_the_last_one_before_today() {
    let mut world = World::at("2026-09-04T09:00:00");
    world.start_review();
    assert_eq!(previous_review(&world.model, world.today()), None);

    // Before today's gate is written, the last review is still Friday's.
    world.clock("2026-09-07T09:00:00");
    assert_eq!(
        previous_review(&world.model, world.today()),
        Some(on("2026-09-04"))
    );
    world.start_review();
    assert_eq!(
        previous_review(&world.model, world.today()),
        Some(on("2026-09-04"))
    );
}

#[test]
fn the_surfaced_step_lists_todays_copies_for_information() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });
    world.clock("2026-09-08T09:00:00");
    world.generate();

    let surfaced = world.surfaced();
    assert_eq!(
        titles(&surfaced.also_starting_today),
        ["Write standup notes"]
    );
    // Nothing is decided about them, so they are outside the count.
    assert_eq!(surfaced.total, 0);
    assert!(!surfaced.is_empty());

    // Yesterday's copy is not listed again.
    assert_eq!(surfaced.also_starting_today[0].place, day("2026-09-08"));
}

#[test]
fn a_review_with_nothing_in_it_is_empty_on_both_steps() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Clean out the garage", Place::Backlog);
    world.add("Review Anna's PR", day("2026-09-07"));

    assert_eq!(world.pile().total, 0);
    assert!(world.surfaced().is_empty());
}

#[test]
fn the_review_keeps_the_pile_it_opened_with() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Send the invoice", day("2026-09-04"));
    let slides = world.add("Prepare slides", day("2026-09-04"));
    let chair = world.add("Order new office chair", day("2026-09-01"));
    let opened = world.pile();
    assert_eq!(opened.total, 3);

    // Closed on its old day, moved to today, and deleted: three ways off
    // the pile, and all three rows stay where the review found them.
    world.must(Command::Close { task: slides });
    world.must(Command::Move {
        task: chair,
        place: day("2026-09-07"),
    });
    world.must(Command::DeleteTask {
        task: world.id("Send the invoice"),
    });
    assert_eq!(world.pile().total, 0);

    let again = world.pile_again(&opened);
    assert_eq!(again.total, 3);
    assert_eq!(
        titles(&again.days[0].rows),
        ["Send the invoice", "Prepare slides"]
    );
    assert_eq!(titles(&again.days[1].rows), ["Order new office chair"]);
    // Every row says what became of the task.
    assert!(again.days[0].rows[1].closed_at.is_some());
    assert_eq!(again.days[1].rows[0].place, day("2026-09-07"));
}

#[test]
fn the_pile_ages_again_when_the_day_rolls_over_under_the_review() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Order new office chair", day("2026-09-04"));
    let opened = world.pile();
    assert_eq!(opened.days[0].age, 3);

    world.clock("2026-09-08T09:00:00");

    assert_eq!(world.pile_again(&opened).days[0].age, 4);
}

#[test]
fn the_surfaced_step_keeps_the_rows_it_opened_with() {
    let mut world = World::at("2026-09-07T09:00:00");
    let ci = world.add("Migrate CI", Place::Backlog);
    world.must(Command::SetDue {
        task: ci,
        date: Some(on("2026-09-05")),
    });
    let opened = world.surfaced();
    assert_eq!(opened.total, 1);

    world.must(Command::Move {
        task: ci,
        place: day("2026-09-07"),
    });
    assert!(world.surfaced().due.is_empty());

    let again = world.surfaced_again(&opened);
    assert_eq!(titles(&again.due), ["Migrate CI"]);
    assert_eq!(again.due[0].place, day("2026-09-07"));
    assert_eq!(again.total, 1);
}

// ---- search ----------------------------------------------------------

#[test]
fn search_finds_live_tasks_by_title_across_all_history() {
    let mut world = World::at("2026-09-04T09:00:00");
    let closed = world.add("Send the invoice to Nordic Ltd", day("2026-09-04"));
    world.must(Command::Close { task: closed });
    world.clock("2026-09-07T09:00:00");
    let open = world.add("Chase the unpaid invoices", Place::Backlog);
    world.must(Command::SetWaiting {
        task: open,
        waiting: true,
    });
    let deleted = world.add("Invoice template v2", Place::Backlog);
    world.must(Command::DeleteTask { task: deleted });

    let results = search(&world.model, "INVOICE", world.today());

    assert_eq!(titles(&results.open), ["Chase the unpaid invoices"]);
    assert_eq!(titles(&results.closed), ["Send the invoice to Nordic Ltd"]);
    assert_eq!(results.total, 2);
    assert!(results.open[0].waiting);
    assert_eq!(results.closed[0].place, day("2026-09-04"));
}

#[test]
fn each_recurring_copy_is_its_own_row_in_search() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.must(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });
    world.clock("2026-09-09T09:00:00");
    world.generate();

    let results = search(&world.model, "standup", world.today());

    assert_eq!(results.total, 3);
    assert!(results.open.iter().all(|row| row.repeat.is_some()));
}

// ---- delete ----------------------------------------------------------

#[test]
fn a_deleted_task_is_invisible_everywhere_and_comes_back_whole() {
    let mut world = World::at("2026-09-04T09:00:00");
    let id = world.add("Reply to Anna", day("2026-09-04"));
    world.add("Weekly planning", day("2026-09-04"));
    world.must(Command::Close { task: id });

    world.clock("2026-09-07T09:00:00");
    let before = world.model.clone();
    world.must(Command::DeleteTask { task: id });

    let friday = world.day("2026-09-04");
    assert_eq!(friday.counts.planned, 1);
    assert!(friday.done.is_empty());
    assert_eq!(search(&world.model, "Anna", world.today()).total, 0);
    assert_eq!(day_list(&world.model).days[0].kept, 1);

    world.undo();
    assert_eq!(world.model.tasks, before.tasks);
    assert_eq!(titles(&world.day("2026-09-04").done), ["Reply to Anna"]);
}

// ---- undo ------------------------------------------------------------

/// Every user command, and the model it must leave behind when undone.
#[test]
fn every_command_has_an_inverse_that_puts_the_model_back() {
    let mut world = World::at("2026-09-07T09:00:00");
    let planned = world.add("Ship invoice export", day("2026-09-07"));
    let parked = world.add("Clean out the garage", Place::Backlog);
    world.add("Sort photo backups", Place::Backlog);
    world.must(Command::CreateSchedule {
        task: planned,
        rule: Rule::Daily,
    });
    let schedule = world.task(planned).schedule_id.expect("a schedule");
    world.must(Command::CreateNote);
    let note = world
        .model
        .notes
        .keys()
        .copied()
        .next_back()
        .expect("a note");

    let commands = [
        Command::AddTask {
            title: "A new one".to_owned(),
            place: day("2026-09-07"),
        },
        Command::EditTitle {
            task: parked,
            title: "Tidy the garage".to_owned(),
        },
        Command::EditTitleAndFuture {
            task: planned,
            title: "Ship it".to_owned(),
            schedule_title: "Ship it".to_owned(),
        },
        Command::Close { task: planned },
        Command::Close { task: parked },
        Command::SetFocus {
            task: planned,
            focus: true,
        },
        Command::Move {
            task: parked,
            place: day("2026-09-08"),
        },
        Command::Move {
            task: planned,
            place: Place::Backlog,
        },
        Command::Reorder {
            task: parked,
            position: 0,
        },
        Command::SetWaiting {
            task: parked,
            waiting: true,
        },
        Command::SetWaiting {
            task: planned,
            waiting: true,
        },
        Command::SetDue {
            task: parked,
            date: Some(on("2026-09-12")),
        },
        Command::SetRemind {
            task: parked,
            date: Some(on("2026-09-12")),
        },
        Command::DeleteTask { task: parked },
        Command::CreateSchedule {
            task: parked,
            rule: workdays(),
        },
        Command::SetRule {
            schedule,
            rule: workdays(),
        },
        Command::StopSchedule { schedule },
        Command::CreateNote,
        Command::DeleteNote { note },
    ];

    for command in commands {
        let before = world.model.clone();
        world.must(command.clone());
        assert_ne!(world.model, before, "{command:?} changed nothing");

        let undone = world.undo();
        assert_eq!(undone.dropped, None, "{command:?} could not be undone");
        assert_eq!(
            visible(&world.model),
            visible(&before),
            "{command:?} was not put back"
        );
    }
}

#[test]
fn undoing_a_move_drops_only_the_placement_the_move_wrote() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Book the venue", day("2026-09-07"));

    world.must(Command::Move {
        task: id,
        place: day("2026-09-08"),
    });
    assert!(world.model.placement(id, on("2026-09-08")).is_some());

    world.undo();
    assert!(world.model.placement(id, on("2026-09-08")).is_none());
    assert!(world.model.placement(id, on("2026-09-07")).is_some());
}

#[test]
fn undoing_a_move_back_to_a_day_leaves_the_first_arrivals_row() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Book the venue", day("2026-09-07"));
    world.must(Command::Move {
        task: id,
        place: Place::Backlog,
    });
    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });

    world.undo();

    assert_eq!(world.task(id).day, None);
    assert!(world.model.placement(id, on("2026-09-07")).is_some());
}

#[test]
fn an_inverse_whose_precondition_no_longer_holds_is_dropped() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Book the venue", day("2026-09-07"));

    // An entry another window wrote, for a task that has since gone.
    world.commit(&Change {
        writes: vec![Write::PushUndo(UndoEntry {
            id: 99,
            at: world.now.clone(),
            label: "Closed \"Chase the hosting invoice\"".to_owned(),
            inverse: Command::MoveBack {
                task: 404,
                place: day("2026-09-07"),
                position: 0,
                waiting: false,
                drop_placement: None,
            },
        })],
    });

    let undone = world.undo();

    assert_eq!(
        undone.dropped,
        Some(Rejected("That task is gone.".to_owned()))
    );
    assert_eq!(undone.label, "Closed \"Chase the hosting invoice\"");
    assert!(world.model.undo.iter().all(|entry| entry.id != 99));
    assert!(world.task(id).is_live());
}

#[test]
fn there_is_nothing_to_undo_on_an_empty_stack() {
    let world = World::at("2026-09-07T09:00:00");
    assert_eq!(
        undo(&world.model, &world.now),
        Err(Rejected("There is nothing to undo.".to_owned()))
    );
}

#[test]
fn the_undo_stack_is_capped_at_the_length_the_application_passes_in() {
    let mut world = World::at("2026-09-07T09:00:00");
    for index in 0..4 {
        let change = apply(
            &world.model,
            Command::AddTask {
                title: format!("Task {index}"),
                place: Place::Backlog,
            },
            &world.now,
            2,
        )
        .expect("an add");
        world.commit(&change);
    }

    let labels: Vec<&str> = world
        .model
        .undo
        .iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(labels, ["Added \"Task 2\"", "Added \"Task 3\""]);
}

#[test]
fn undo_itself_pushes_nothing_and_there_is_no_redo() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Book dentist", Place::Backlog);
    assert_eq!(world.model.undo.len(), 1);

    world.undo();

    assert!(world.model.undo.is_empty());
}

#[test]
fn a_note_body_edit_pushes_nothing() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.must(Command::CreateNote);
    let note = world
        .model
        .notes
        .keys()
        .copied()
        .next_back()
        .expect("a note");
    let entries = world.model.undo.len();

    world.must(Command::EditNote {
        note,
        body: "remember to mention X".to_owned(),
    });

    assert_eq!(world.model.undo.len(), entries);
    assert_eq!(
        world.model.note(note).expect("the note").body,
        "remember to mention X"
    );
}

#[test]
fn the_undo_label_names_the_task_and_where_it_went() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Book dentist", Place::Backlog);
    let labels = |world: &World| world.model.undo.last().expect("an entry").label.clone();

    assert_eq!(labels(&world), "Added \"Book dentist\"");

    world.must(Command::Move {
        task: id,
        place: day("2026-09-07"),
    });
    assert_eq!(labels(&world), "Moved \"Book dentist\" to today");

    world.must(Command::Move {
        task: id,
        place: day("2026-09-14"),
    });
    assert_eq!(labels(&world), "Moved \"Book dentist\" to Mon 14 Sep");

    world.must(Command::Move {
        task: id,
        place: Place::Backlog,
    });
    assert_eq!(labels(&world), "Moved \"Book dentist\" to the backlog");

    world.must(Command::DeleteTask { task: id });
    assert_eq!(labels(&world), "Deleted \"Book dentist\"");
}

// ---- notes -----------------------------------------------------------

#[test]
fn a_note_is_created_edited_and_thrown_away_on_its_own() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.must(Command::CreateNote);
    world.must(Command::CreateNote);
    let notes: Vec<Id> = world.model.notes.keys().copied().collect();
    assert_eq!(notes.len(), 2);

    world.must(Command::EditNote {
        note: notes[0],
        body: "draft a message".to_owned(),
    });
    world.must(Command::DeleteNote { note: notes[0] });

    assert!(!world.model.note(notes[0]).expect("the note").is_live());
    assert!(world.model.note(notes[1]).expect("the note").is_live());

    // A note stays until it is deleted; nothing expires it.
    world.clock("2027-09-07T09:00:00");
    assert!(world.model.note(notes[1]).expect("the note").is_live());
}

#[test]
fn a_deleted_note_comes_back() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.must(Command::CreateNote);
    let note = world
        .model
        .notes
        .keys()
        .copied()
        .next_back()
        .expect("a note");
    world.must(Command::EditNote {
        note,
        body: "remember to mention X".to_owned(),
    });
    world.must(Command::DeleteNote { note });

    world.undo();

    let restored = world.model.note(note).expect("the note");
    assert!(restored.is_live());
    assert_eq!(restored.body, "remember to mention X");
}

#[test]
fn the_note_list_is_newest_first_and_shows_the_first_line() {
    let mut world = World::at("2026-09-07T09:00:00");
    let made = world.now.clone();
    world.must(Command::CreateNote);
    let first = world
        .model
        .notes
        .keys()
        .copied()
        .next_back()
        .expect("a note");
    world.clock("2026-09-07T10:00:00");
    world.must(Command::CreateNote);
    let second = world
        .model
        .notes
        .keys()
        .copied()
        .next_back()
        .expect("a note");

    world.must(Command::EditNote {
        note: first,
        body: "Mention to Anna:\n- CI runner budget".to_owned(),
    });

    let view = notes(&world.model);
    assert_eq!(view.count, 2);
    assert_eq!(
        view.rows.iter().map(|row| row.note).collect::<Vec<_>>(),
        [second, first],
        "the newest note is at the top, and editing did not move it"
    );
    assert_eq!(view.rows[1].first_line, "Mention to Anna:");
    assert_eq!(
        view.rows[1].created_at, made,
        "the row carries when the note was made, not when it was last typed in"
    );

    world.must(Command::DeleteNote { note: second });
    let view = notes(&world.model);
    assert_eq!(view.count, 1, "a deleted note is out of the list");
    assert_eq!(view.rows[0].note, first);
}

// ---- several instances -----------------------------------------------

#[test]
fn the_ids_of_new_rows_come_from_the_model() {
    let mut world = World::at("2026-09-07T09:00:00");
    let first = world.add("First", Place::Backlog);
    let second = world.add("Second", Place::Backlog);

    assert_eq!((first, second), (1, 2));

    world.must(Command::DeleteTask { task: second });
    let third = world.add("Third", Place::Backlog);
    assert_eq!(third, 3);
}

#[test]
fn a_second_window_sees_the_same_pile_gate_and_stack() {
    let mut world = World::at("2026-09-01T09:00:00");
    world.add("Call the accountant about VAT", day("2026-09-01"));
    world.clock("2026-09-07T09:00:00");
    world.start_review();

    let elsewhere = world.store.load().expect("the in-memory store");

    assert_eq!(pile(&elsewhere, world.today()).total, 1);
    assert_eq!(elsewhere.meta_date(REVIEW_ON), Some(on("2026-09-07")));
    assert_eq!(elsewhere.undo.len(), world.model.undo.len());
}

// ---- the rules that need one more angle -------------------------------

#[test]
fn a_task_leaves_the_pile_by_being_closed_moved_or_deleted() {
    let mut world = World::at("2026-09-01T09:00:00");
    let closed = world.add("Weekly planning", day("2026-09-01"));
    let moved = world.add("Order new office chair", day("2026-09-01"));
    let deleted = world.add("Book the team dinner", day("2026-09-01"));
    let kept = world.add("Call the accountant about VAT", day("2026-09-01"));

    world.clock("2026-09-07T09:00:00");
    assert_eq!(world.pile().total, 4);

    world.must(Command::Close { task: closed });
    world.must(Command::Move {
        task: moved,
        place: Place::Backlog,
    });
    world.must(Command::DeleteTask { task: deleted });

    assert_eq!(
        titles(&world.pile().days[0].rows),
        ["Call the accountant about VAT"]
    );
    assert_eq!(world.task(kept).day, Some(on("2026-09-01")));
}

#[test]
fn closing_and_reopening_are_refused_when_they_have_nothing_to_do() {
    let mut world = World::at("2026-09-07T09:00:00");
    let id = world.add("Review Anna's PR", day("2026-09-07"));

    assert_eq!(
        world.refuse(Command::Reopen { task: id }),
        "That task is not closed."
    );
    world.must(Command::Close { task: id });
    assert_eq!(
        world.refuse(Command::Close { task: id }),
        "That task is already closed."
    );

    world.must(Command::DeleteTask { task: id });
    assert_eq!(
        world.refuse(Command::EditTitle {
            task: id,
            title: "Anything".to_owned(),
        }),
        "That task is gone."
    );
}

#[test]
fn a_day_with_nothing_planned_is_empty_and_not_in_the_day_list() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Review Anna's PR", day("2026-09-07"));

    let quiet = world.day("2026-09-12");
    assert_eq!(quiet.counts, DayCounts::default());
    assert!(quiet.focus.is_empty() && quiet.plan.is_empty());
    assert!(quiet.done.is_empty() && quiet.moved.is_empty());

    let days: Vec<String> = day_list(&world.model)
        .days
        .iter()
        .map(|row| row.day.to_string())
        .collect();
    assert_eq!(days, ["2026-09-07"]);
}

#[test]
fn what_a_day_planned_is_what_it_has_placements_for() {
    let mut world = World::at("2026-09-07T09:00:00");
    world.add("Kept", day("2026-09-07"));
    let moved = world.add("Moved", day("2026-09-07"));
    world.must(Command::Move {
        task: moved,
        place: Place::Backlog,
    });

    let placed = world
        .model
        .placements
        .values()
        .filter(|placement| placement.day == on("2026-09-07"))
        .filter(|placement| world.model.live_task(placement.task_id).is_some())
        .count();
    assert_eq!(world.day("2026-09-07").counts.planned, placed);
}

#[test]
fn every_n_weeks_counts_from_a_start_that_may_be_ahead() {
    let ahead = Rule::EveryNWeeks {
        n: 1,
        from: on("2027-01-08"),
    };
    assert_eq!(dates(&ahead, "2026-09-07", 2), ["2027-01-08", "2027-01-15"]);

    let rare = Rule::EveryNWeeks {
        n: 52,
        from: on("2026-09-04"),
    };
    assert_eq!(dates(&rare, "2026-09-07", 2), ["2027-09-03", "2028-09-01"]);
}
