use super::*;

use std::process::Stdio;
use std::time::Duration;

use jiff::Zoned;

use crate::domain::{self, Change, Command, Context, DateOrder, Id, Place, Rule, Settings, Write};

/// The database the writer of the kill test writes to. Its presence is
/// what tells that process it is the child rather than a test run.
const CRASH_DB: &str = "JOBSDONE_CRASH_DB";

/// The length the application holds the undo stack to.
const UNDO_CAP: usize = 50;

fn at(text: &str) -> Zoned {
    text.parse().expect("a zoned timestamp")
}

/// Opens a database in a temporary directory, so a test never touches the
/// real one.
fn scratch() -> (tempfile::TempDir, Sqlite) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let store = Sqlite::open(&dir.path().join("jobsdone.db")).expect("a fresh database");
    (dir, store)
}

fn set_meta(key: &str, value: &str) -> Change {
    Change {
        writes: vec![Write::SetMeta {
            key: key.to_owned(),
            value: value.to_owned(),
        }],
    }
}

/// A database in a temporary directory and the model that was committed
/// to it, so a test can compare what comes back with what went in.
struct Scratch {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    store: Sqlite,
    model: Model,
    now: Zoned,
}

impl Scratch {
    fn new(now: &str) -> Scratch {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("jobsdone.db");
        let store = Sqlite::open(&path).expect("a fresh database");
        Scratch {
            _dir: dir,
            path,
            store,
            model: Model::empty(),
            now: at(now),
        }
    }

    fn clock(&mut self, now: &str) {
        self.now = at(now);
    }

    fn run(&mut self, command: Command) {
        let ctx = Context {
            now: self.now.clone(),
            undo_cap: UNDO_CAP,
            dates: DateOrder::DayFirst,
        };
        let change = domain::apply(&self.model, command, &ctx).expect("a command");
        self.commit(&change);
    }

    fn commit(&mut self, change: &Change) {
        self.store.commit(change).unwrap();
        self.model.apply(change);
    }

    fn add(&mut self, title: &str, place: Place) -> Id {
        self.run(Command::AddTask {
            title: title.to_owned(),
            place,
        });
        self.model
            .tasks
            .values()
            .filter(|task| task.is_live() && task.title == title)
            .map(|task| task.id)
            .next_back()
            .expect("the task just added")
    }

    /// The model as another process would find it.
    fn reopened(&self) -> Model {
        Sqlite::open(&self.path)
            .expect("the same database again")
            .load()
            .expect("load")
    }
}

fn day(date: &str) -> Place {
    Place::Day(date.parse().expect("a civil date"))
}

// ---- the schema ------------------------------------------------------

#[test]
fn a_fresh_database_is_migrated_to_the_latest_schema() {
    let (_dir, store) = scratch();
    let user_version: u32 = store
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version");

    assert_eq!(user_version, 2);
    assert_eq!(store.load().expect("load"), Model::empty());
}

#[test]
fn the_schema_of_domain_section_17_is_what_was_created() {
    let (_dir, store) = scratch();
    let mut statement = store
        .conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("a query over the schema");
    let tables: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .expect("the table names")
        .map(|row| row.expect("a table name"))
        .collect();

    assert_eq!(
        tables,
        [
            "meta",
            "notes",
            "placements",
            "schedules",
            "settings",
            "tasks",
            "undo_log"
        ]
    );
}

#[test]
fn a_database_from_the_future_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("jobsdone.db");
    Sqlite::open(&path).expect("a fresh database");

    let conn = Connection::open(&path).expect("the database again");
    conn.pragma_update(None, "user_version", 99)
        .expect("a schema from the future");
    drop(conn);

    let refused = Sqlite::open(&path);
    let Err(StoreError::Other(message)) = refused else {
        panic!("a database from the future must not open");
    };
    assert!(message.contains("newer version"), "{message}");
}

#[test]
fn foreign_keys_are_on() {
    let (_dir, store) = scratch();
    let on: bool = store
        .conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("the pragma");
    assert!(on);
}

// ---- loading ---------------------------------------------------------

#[test]
fn a_committed_change_survives_the_process() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("jobsdone.db");

    {
        let mut store = Sqlite::open(&path).expect("a fresh database");
        store.commit(&set_meta("review_on", "2026-09-05")).unwrap();
        store.commit(&set_meta("review_on", "2026-09-06")).unwrap();
    }

    let reopened = Sqlite::open(&path).expect("the same database again");
    let model = reopened.load().expect("load");
    assert_eq!(
        model.meta.get("review_on").map(String::as_str),
        Some("2026-09-06")
    );
}

/// One task of every shape, one placement of every kind, a stopped and an
/// unstopped schedule, a live and a deleted note, an undo stack and the
/// review gate: load has to give every one of them back.
#[test]
fn load_gives_back_the_whole_model() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");

    let repeating = world.add("Write standup notes", day("2026-09-07"));
    world.run(Command::CreateSchedule {
        task: repeating,
        rule: Rule::Weekly {
            weekdays: vec![domain::Weekday::Mon, domain::Weekday::Thu],
        },
    });
    let stopping = world.add("Weekly planning", day("2026-09-07"));
    world.run(Command::CreateSchedule {
        task: stopping,
        rule: Rule::Monthly {
            day: domain::MonthDay::Last,
        },
    });
    let stopped = world
        .model
        .task(stopping)
        .and_then(|task| task.schedule_id)
        .expect("a schedule");
    world.run(Command::StopSchedule { schedule: stopped });

    let focused = world.add("Ship invoice export", day("2026-09-07"));
    world.run(Command::SetFocus {
        task: focused,
        focus: true,
    });

    let closed = world.add("Morning review", day("2026-09-07"));
    world.run(Command::Close { task: closed });

    let pulled = world.add("Fix the flaky migration test", Place::Backlog);
    world.run(Command::Move {
        task: pulled,
        place: day("2026-09-07"),
    });

    let moved = world.add("Chase the hosting invoice", day("2026-09-07"));
    world.run(Command::Move {
        task: moved,
        place: day("2026-09-08"),
    });

    let dated = world.add("Migrate CI to the new runners", Place::Backlog);
    world.run(Command::SetDue {
        task: dated,
        date: Some("2026-09-12".parse().expect("a date")),
    });
    world.run(Command::SetRemind {
        task: dated,
        date: Some("2026-09-10".parse().expect("a date")),
    });

    let waiting = world.add("Quote from the electrician", Place::Backlog);
    world.run(Command::SetWaiting {
        task: waiting,
        waiting: true,
    });

    let deleted = world.add("Book the venue", day("2026-09-07"));
    world.run(Command::DeleteTask { task: deleted });

    world.run(Command::CreateNote);
    world.run(Command::CreateNote);
    let notes: Vec<Id> = world.model.notes.keys().copied().collect();
    world.run(Command::EditNote {
        note: notes[0],
        body: "remember to mention X\nand Y".to_owned(),
    });
    world.run(Command::DeleteNote { note: notes[1] });

    world.clock("2026-09-08T09:00:00+02:00[Europe/Copenhagen]");
    world.commit(&domain::generate_copies(&world.model.clone(), &world.now));
    if let Some(change) = domain::start_review(&world.model.clone(), "2026-09-08".parse().unwrap())
    {
        world.commit(&change);
    }

    assert!(world.model.tasks.len() > 8);
    assert_eq!(world.model.schedules.len(), 2);
    assert_eq!(world.model.notes.len(), 2);
    assert!(!world.model.undo.is_empty());
    assert_eq!(world.reopened(), world.model);
}

#[test]
fn an_instant_reads_back_as_the_instant_that_was_written() {
    let mut world = Scratch::new("2026-09-05T01:30:45.123456789+02:00[Europe/Copenhagen]");
    let id = world.add("Ship the release", day("2026-09-04"));
    world.run(Command::Close { task: id });

    let reopened = world.reopened();
    let task = reopened.task(id).expect("the task");
    assert_eq!(task.closed_at, world.model.task(id).expect("it").closed_at);
    assert_eq!(
        task.closed_at
            .as_ref()
            .map(|at| Settings::default().working_day(at)),
        Some("2026-09-04".parse().expect("a date"))
    );
}

#[test]
fn the_version_moves_when_another_instance_writes() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("jobsdone.db");
    let mut store = Sqlite::open(&path).expect("a fresh database");
    let before = store.version().expect("version");

    // PRAGMA data_version does not move for a write on the connection that
    // made it. That is the point of it: it answers "did somebody else
    // change this", which is the question STACK.md section 3 asks on every
    // tick.
    store.commit(&set_meta("review_on", "2026-09-05")).unwrap();
    assert_eq!(store.version().expect("version"), before);

    let mut elsewhere = Sqlite::open(&path).expect("a second instance");
    elsewhere
        .commit(&set_meta("review_on", "2026-09-06"))
        .unwrap();

    assert_ne!(store.version().expect("version"), before);
    assert_eq!(
        store
            .load()
            .expect("load")
            .meta
            .get("review_on")
            .map(String::as_str),
        Some("2026-09-06")
    );
}

// ---- settings --------------------------------------------------------

#[test]
fn the_settings_read_back_as_what_was_committed() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let mut settings = Settings::default();
    settings.set_day_starts_at(8);
    settings.set_window_size(domain::WindowSize::new(1200, 800));
    settings.set_confirm_delete(true);

    world.commit(&Change {
        writes: vec![Write::PutSettings(settings.clone())],
    });

    assert_eq!(world.reopened().settings, settings);
}

#[test]
fn a_settings_key_this_build_does_not_know_is_ignored() {
    let (_dir, store) = scratch();
    store
        .conn
        .execute(
            "INSERT INTO settings (key, value) VALUES ('what_a_later_version_added', 'whatever')",
            [],
        )
        .expect("a row from another version");

    assert_eq!(store.load().expect("load").settings, Settings::default());
}

// ---- one command, one transaction ------------------------------------

#[test]
fn a_change_that_cannot_be_written_writes_none_of_itself() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let id = world.add("Ship invoice export", day("2026-09-07"));

    // A good write followed by a placement for a task that is not there.
    let broken = Change {
        writes: vec![
            Write::SetMeta {
                key: "review_on".to_owned(),
                value: "2026-09-07".to_owned(),
            },
            Write::PutPlacement(Placement {
                task_id: 404,
                day: "2026-09-07".parse().expect("a date"),
                placed_at: world.now.clone(),
                from_place: FromPlace::New,
            }),
        ],
    };

    assert_eq!(world.store.commit(&broken), Err(StoreError::Conflict));

    let reopened = world.reopened();
    assert!(reopened.meta.is_empty());
    assert_eq!(reopened.placements.len(), 1);
    assert!(reopened.task(id).is_some());
}

#[test]
fn the_second_writer_of_the_same_copy_is_a_conflict() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.run(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });

    world.clock("2026-09-08T09:00:00+02:00[Europe/Copenhagen]");
    let generated = domain::generate_copies(&world.model, &world.now);
    world.commit(&generated);

    // The other instance generated the same day from the model it had
    // before, and the unique index on (schedule_id, scheduled_on) stops it.
    let mut elsewhere = Sqlite::open(&world.path).expect("a second instance");
    assert_eq!(elsewhere.commit(&generated), Err(StoreError::Conflict));
    assert_eq!(world.reopened(), world.model);
}

#[test]
fn a_task_can_be_written_again_without_losing_its_placements() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let id = world.add("Ship invoice export", day("2026-09-07"));

    world.run(Command::EditTitle {
        task: id,
        title: "Ship the invoice export".to_owned(),
    });

    let reopened = world.reopened();
    assert_eq!(
        reopened.task(id).map(|task| task.title.as_str()),
        Some("Ship the invoice export")
    );
    assert!(
        reopened
            .placement(id, "2026-09-07".parse().expect("a date"))
            .is_some()
    );
}

#[test]
fn undoing_a_repeat_takes_the_schedule_row_with_it() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let id = world.add("Write standup notes", day("2026-09-07"));
    world.run(Command::CreateSchedule {
        task: id,
        rule: Rule::Daily,
    });
    assert_eq!(world.reopened().schedules.len(), 1);

    let ctx = Context {
        now: world.now.clone(),
        undo_cap: UNDO_CAP,
        dates: DateOrder::DayFirst,
    };
    let undone = domain::undo(&world.model, &ctx).expect("something to undo");
    world.commit(&undone.change);

    let reopened = world.reopened();
    assert!(reopened.schedules.is_empty());
    assert_eq!(reopened.task(id).and_then(|task| task.schedule_id), None);
    assert_eq!(reopened, world.model);
}

// ---- killed at any moment --------------------------------------------

/// A change is either wholly there or not there at all after the process
/// is killed, whenever the kill lands.
///
/// The writer is this same test binary, started again for the one test
/// below and killed after a while. Every change it commits is a task and
/// the undo entry that goes with it, so half a change would show up as a
/// task without its entry, or a place whose positions have a hole in it.
#[test]
fn a_kill_at_any_moment_leaves_no_half_change() {
    let mut written = Vec::new();

    for delay in [15u64, 40, 80, 150] {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("jobsdone.db");
        Sqlite::open(&path).expect("a fresh database");

        let mut writer =
            std::process::Command::new(std::env::current_exe().expect("the test binary"))
                .args([
                    "--exact",
                    "--ignored",
                    "storage::tests::the_writer_a_kill_stops",
                ])
                .env(CRASH_DB, &path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("a writer");

        std::thread::sleep(Duration::from_millis(delay));
        writer.kill().expect("the writer to be killable");
        writer.wait().expect("the writer to be gone");

        let model = Sqlite::open(&path)
            .expect("the database after the kill")
            .load()
            .expect("load");

        assert_eq!(
            model.tasks.len(),
            model.undo.len(),
            "a task was written without its undo entry, or the other way round"
        );
        for (position, task) in model.tasks.values().enumerate() {
            assert_eq!(task.position, position, "the backlog has a hole in it");
            assert_eq!(
                model.undo[position].label,
                format!("Added \"{}\"", task.title)
            );
        }
        written.push(model.tasks.len());
    }

    assert!(
        written.iter().any(|count| *count > 0),
        "the writer never got as far as a change: {written:?}"
    );
}

/// The writer the test above kills. It is ignored so that an ordinary run
/// skips it, and it does nothing at all unless it was started with a
/// database to write to.
#[test]
#[ignore = "started as a child process by a_kill_at_any_moment_leaves_no_half_change"]
fn the_writer_a_kill_stops() {
    let Ok(path) = std::env::var(CRASH_DB) else {
        return;
    };
    let mut store = Sqlite::open(Path::new(&path)).expect("the database");
    let mut model = store.load().expect("load");
    let now = at("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");

    loop {
        let command = Command::AddTask {
            title: format!("Task {}", model.tasks.len() + 1),
            place: Place::Backlog,
        };
        let ctx = Context {
            now: now.clone(),
            undo_cap: usize::MAX,
            dates: DateOrder::DayFirst,
        };
        let change = domain::apply(&model, command, &ctx).expect("an add");
        if store.commit(&change).is_err() {
            return;
        }
        model.apply(&change);
    }
}
