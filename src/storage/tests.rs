use super::*;

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
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

    assert_eq!(user_version, 5);
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
            "personal_dictionary",
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
    world.run(Command::CreateNote);
    let archived = *world.model.notes.keys().next_back().expect("a note");
    world.run(Command::ArchiveNote { note: archived });

    world.clock("2026-09-08T09:00:00+02:00[Europe/Copenhagen]");
    world.commit(&domain::generate_copies(&world.model.clone(), &world.now));
    if let Some(change) = domain::start_review(&world.model.clone(), "2026-09-08".parse().unwrap())
    {
        world.commit(&change);
    }

    assert!(world.model.tasks.len() > 8);
    assert_eq!(world.model.schedules.len(), 2);
    assert_eq!(world.model.notes.len(), 3);
    assert!(world.model.notes.values().any(|note| note.is_archived()));
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
    settings.set_spell_check_notes(true);

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

    let settings = store.load().expect("load").settings;
    assert_eq!(settings, Settings::default());
    assert!(
        !settings.spell_check_notes(),
        "and a table with no row for a setting is that setting's default"
    );
}

// ---- one command, one transaction ------------------------------------

// ---- the personal dictionary -----------------------------------------

#[test]
fn the_personal_dictionary_reads_back_as_what_was_committed() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");

    for word in ["Ratatui", "Kubernetes", "Café"] {
        let change = domain::add_dictionary_word(&world.model, word).expect("the word");
        world.commit(&change);
    }
    let change = domain::remove_dictionary_word(&world.model, "kubernetes").expect("the word");
    world.commit(&change);

    assert_eq!(
        world.reopened().personal_dictionary,
        world.model.personal_dictionary
    );
    assert_eq!(
        world.reopened().personal_dictionary,
        [
            ("café".to_owned(), "Café".to_owned()),
            ("ratatui".to_owned(), "Ratatui".to_owned()),
        ]
        .into_iter()
        .collect()
    );
}

/// The schema the dictionary arrived in is applied to a database written
/// by a build that did not have it, and nothing that database held is
/// lost on the way.
#[test]
fn a_database_without_the_dictionary_gains_it_and_keeps_what_it_had() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("jobsdone.db");

    let conn = Connection::open(&path).expect("a database");
    for (number, sql) in MIGRATIONS.iter().filter(|(number, _)| *number < 3) {
        conn.execute_batch(&format!("BEGIN;\n{sql}\nCOMMIT;"))
            .unwrap_or_else(|error| panic!("migration {number}: {error}"));
    }
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('review_on', '2026-09-06')",
        [],
    )
    .expect("a row written before the dictionary existed");
    drop(conn);

    let mut store = Sqlite::open(&path).expect("the same database again");
    let user_version: u32 = store
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version");
    assert_eq!(user_version, 5);

    let model = store.load().expect("load");
    assert_eq!(
        model.meta.get("review_on").map(String::as_str),
        Some("2026-09-06")
    );
    assert!(model.personal_dictionary.is_empty());

    let change = domain::add_dictionary_word(&model, "Ratatui").expect("the word");
    store.commit(&change).expect("the new table takes a word");
    assert_eq!(
        store
            .load()
            .expect("load")
            .personal_dictionary
            .get("ratatui")
            .map(String::as_str),
        Some("Ratatui")
    );
}

#[test]
fn a_note_written_before_the_archive_existed_is_in_the_list() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("jobsdone.db");
    let conn = Connection::open(&path).expect("a new database");
    for (number, sql) in MIGRATIONS.iter().filter(|(number, _)| *number < 5) {
        conn.execute_batch(&format!("BEGIN;\n{sql}\nCOMMIT;"))
            .unwrap_or_else(|error| panic!("migration {number}: {error}"));
    }
    conn.execute(
        "INSERT INTO notes (id, body, created_at, updated_at, deleted_at)
         VALUES (1, 'Milk', ?1, ?1, NULL)",
        params!["2026-09-07T09:00:00+02:00[Europe/Copenhagen]"],
    )
    .expect("a note written before the archive existed");
    drop(conn);

    let store = Sqlite::open(&path).expect("the same database again");
    let note = store.load().expect("load").notes[&1].clone();
    assert_eq!(note.body, "Milk");
    assert!(!note.is_archived());
}

/// A client started before the archive existed keeps running until it
/// is restarted. Its note upsert names its columns, so saving a body into
/// a note that has since been archived leaves the archive alone.
#[test]
fn an_older_client_saving_a_note_leaves_its_archive_alone() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    world.run(Command::CreateNote);
    let note = *world.model.notes.keys().next().expect("the note");
    world.run(Command::ArchiveNote { note });

    let conn = Connection::open(&world.path).expect("the database again");
    conn.execute(
        "INSERT INTO notes (id, body, created_at, updated_at, deleted_at)
         VALUES (?1, 'Typed by the old client', ?2, ?2, NULL)
         ON CONFLICT (id) DO UPDATE SET
             body = excluded.body, created_at = excluded.created_at,
             updated_at = excluded.updated_at, deleted_at = excluded.deleted_at",
        params![note, "2026-09-07T10:00:00+02:00[Europe/Copenhagen]"],
    )
    .expect("the old upsert");
    drop(conn);

    let reopened = world.reopened();
    let saved = reopened.note(note).expect("the note");
    assert_eq!(saved.body, "Typed by the old client");
    assert_eq!(
        saved.archived_at,
        world.model.note(note).unwrap().archived_at
    );
}

#[test]
fn a_database_at_the_latest_schema_is_opened_without_migrating_it_again() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let change = domain::add_dictionary_word(&world.model, "Ratatui").expect("the word");
    world.commit(&change);

    let again = Sqlite::open(&world.path).expect("the same database again");
    let user_version: u32 = again
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version");

    assert_eq!(user_version, 5);
    assert_eq!(
        again.load().expect("load").personal_dictionary,
        world.model.personal_dictionary
    );
}

#[test]
fn a_word_from_another_instance_is_not_lost_to_this_ones_word() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");

    // Both windows worked their change out from the dictionary as it was
    // before either of them wrote.
    let mine = domain::add_dictionary_word(&world.model, "Kubernetes").expect("the word");
    let theirs = domain::add_dictionary_word(&world.model, "Wayland").expect("the word");
    world.commit(&mine);

    let mut elsewhere = Sqlite::open(&world.path).expect("a second instance");
    elsewhere.commit(&theirs).expect("the other window's word");

    let words: Vec<String> = world.reopened().personal_dictionary.into_values().collect();
    assert_eq!(words, ["Kubernetes", "Wayland"]);
}

#[test]
fn a_dictionary_change_that_cannot_be_written_writes_none_of_itself() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let change = domain::add_dictionary_word(&world.model, "Ratatui").expect("the word");
    world.commit(&change);

    // A word, and then a placement for a task that is not there.
    let broken = Change {
        writes: vec![
            Write::PutDictionaryWord {
                key: "kubernetes".to_owned(),
                word: "Kubernetes".to_owned(),
            },
            Write::DeleteDictionaryWord {
                key: "ratatui".to_owned(),
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
    assert_eq!(
        world.reopened().personal_dictionary,
        world.model.personal_dictionary
    );
}

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
    assert!(!reopened.meta.contains_key("review_on"));
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

// ---- two connections -------------------------------------------------

/// A second connection that has loaded, which is what makes it a window
/// with a model of its own rather than a caller that has claimed nothing.
fn another(path: &std::path::Path) -> Sqlite {
    let store = Sqlite::open(path).expect("a second instance");
    store.load().expect("load");
    store
}

fn ctx(now: &Zoned) -> Context {
    Context {
        now: now.clone(),
        undo_cap: UNDO_CAP,
        dates: DateOrder::DayFirst,
    }
}

/// Both windows read the same task and each set a different field on it.
/// A task is written as a whole row, so the second write carries the
/// first's field back to what it was; it is refused instead.
#[test]
fn two_windows_setting_two_fields_of_one_task_do_not_lose_one() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let id = world.add("Book the venue", day("2026-09-07"));
    let mut mine = another(&world.path);
    let mut theirs = another(&world.path);
    let model = mine.load().expect("load");

    let due = domain::apply(
        &model,
        Command::SetDue {
            task: id,
            date: Some("2026-09-11".parse().expect("a date")),
        },
        &ctx(&world.now),
    )
    .expect("the due date");
    let focus = domain::apply(
        &model,
        Command::SetFocus {
            task: id,
            focus: true,
        },
        &ctx(&world.now),
    )
    .expect("the focus");

    mine.commit(&due).expect("the first window's change");
    assert_eq!(theirs.commit(&focus), Err(StoreError::Conflict));

    let task = world.reopened().task(id).expect("the task").clone();
    assert_eq!(task.due_on, Some("2026-09-11".parse().expect("a date")));
    assert!(!task.focus, "the second window wrote nothing at all");
}

/// New ids are the largest in the model plus one, so two windows adding
/// a task at the same moment choose the same one and the upsert would
/// quietly make their two tasks one row.
#[test]
fn two_windows_adding_a_task_do_not_land_on_one_row() {
    let world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let mut mine = another(&world.path);
    let mut theirs = another(&world.path);
    let model = mine.load().expect("load");

    let add = |title: &str| {
        domain::apply(
            &model,
            Command::AddTask {
                title: title.to_owned(),
                place: day("2026-09-07"),
            },
            &ctx(&world.now),
        )
        .expect("the task")
    };
    let ours = add("Book the venue");
    let others = add("Chase the invoice");

    mine.commit(&ours).expect("the first window's task");
    assert_eq!(theirs.commit(&others), Err(StoreError::Conflict));

    // Asked again from the model as it now is, the second task is its own
    // row beside the first.
    let model = theirs.load().expect("load");
    let again = domain::apply(
        &model,
        Command::AddTask {
            title: "Chase the invoice".to_owned(),
            place: day("2026-09-07"),
        },
        &ctx(&world.now),
    )
    .expect("the task");
    theirs.commit(&again).expect("the second window's task");

    let titles: Vec<String> = world
        .reopened()
        .tasks
        .values()
        .map(|task| task.title.clone())
        .collect();
    assert_eq!(titles, ["Book the venue", "Chase the invoice"]);
}

#[test]
fn a_note_saved_elsewhere_is_not_written_over_by_an_older_body() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    world.run(Command::CreateNote);
    let note = *world.model.notes.keys().next().expect("the note");
    let mut mine = another(&world.path);
    let mut theirs = another(&world.path);
    let model = mine.load().expect("load");

    let body = |text: &str| {
        domain::apply(
            &model,
            Command::EditNote {
                note,
                body: text.to_owned(),
            },
            &ctx(&world.now),
        )
        .expect("the body")
    };

    mine.commit(&body("Milk\nBread")).expect("the first body");
    assert_eq!(theirs.commit(&body("Eggs")), Err(StoreError::Conflict));

    assert_eq!(
        world.reopened().note(note).map(|note| note.body.as_str()),
        Some("Milk\nBread")
    );
}

/// The settings are one value and they decide what today is, so a
/// command worked out under the old ones is worked out under a rule that
/// has changed, whatever rows it happens to name.
#[test]
fn a_settings_change_elsewhere_refuses_a_command_worked_out_before_it() {
    let world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let mut mine = another(&world.path);
    let mut theirs = another(&world.path);
    let model = mine.load().expect("load");

    let mut settings = model.settings.clone();
    settings.set_day_starts_at(8);
    let moved = domain::change_settings(&model, settings).expect("the settings");
    let added = domain::apply(
        &model,
        Command::AddTask {
            title: "Book the venue".to_owned(),
            place: day("2026-09-07"),
        },
        &ctx(&world.now),
    )
    .expect("the task");

    mine.commit(&moved).expect("the settings");
    assert_eq!(theirs.commit(&added), Err(StoreError::Conflict));

    let reopened = world.reopened();
    assert_eq!(reopened.settings.day_starts_at(), 8);
    assert!(reopened.tasks.is_empty());
}

/// The undo stack is shared, and `u` takes back the entry that was on
/// top of the stack the window loaded. Another window having pushed a
/// newer one means the entry on top is no longer the one this undo was
/// worked out from, so the change is refused rather than reaching past
/// somebody else's work.
#[test]
fn an_undo_worked_out_before_a_newer_entry_arrived_is_refused() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    world.add("Book the venue", day("2026-09-07"));
    let mut mine = another(&world.path);
    let mut theirs = another(&world.path);

    let undone =
        domain::undo(&theirs.load().expect("load"), &ctx(&world.now)).expect("something to undo");

    // The other window added a task, which is the entry on top now.
    let model = mine.load().expect("load");
    let added = domain::apply(
        &model,
        Command::AddTask {
            title: "Chase the invoice".to_owned(),
            place: day("2026-09-07"),
        },
        &ctx(&world.now),
    )
    .expect("the task");
    mine.commit(&added).expect("the other window's task");

    assert_eq!(theirs.commit(&undone.change), Err(StoreError::Conflict));

    let reopened = world.reopened();
    assert_eq!(reopened.undo.len(), 2, "neither entry was taken off");
    assert_eq!(reopened.tasks.len(), 2);
}

/// A refused change writes nothing, not even the writes in front of the
/// one that made the change stale.
#[test]
fn a_change_refused_as_stale_leaves_the_database_as_it_was() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let id = world.add("Book the venue", day("2026-09-07"));
    let mut mine = another(&world.path);
    let mut theirs = another(&world.path);
    let before = theirs.load().expect("load");

    mine.commit(&set_meta("review_on", "2026-09-07"))
        .expect("the other window's write");

    let change = Change {
        writes: vec![
            Write::SetMeta {
                key: "review_before".to_owned(),
                value: "2026-09-06".to_owned(),
            },
            Write::PutDictionaryWord {
                key: "kubernetes".to_owned(),
                word: "Kubernetes".to_owned(),
            },
        ],
    };
    assert_eq!(theirs.commit(&change), Err(StoreError::Conflict));

    let reopened = world.reopened();
    assert!(!reopened.meta.contains_key("review_before"));
    assert!(reopened.personal_dictionary.is_empty());
    assert_eq!(
        reopened.task(id).map(|task| task.id),
        before.task(id).map(|task| task.id)
    );
}

/// A change committed by this connection is the model this connection
/// now holds, so the next change made from it goes in. Without that, one
/// window on its own would refuse its own second change.
#[test]
fn a_window_can_commit_twice_running_without_loading_in_between() {
    let world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let mut store = another(&world.path);
    let mut model = store.load().expect("load");

    for title in ["Book the venue", "Chase the invoice", "Draft the notes"] {
        let change = domain::apply(
            &model,
            Command::AddTask {
                title: title.to_owned(),
                place: day("2026-09-07"),
            },
            &ctx(&world.now),
        )
        .expect("the task");
        store
            .commit(&change)
            .expect("a change of this window's own");
        model.apply(&change);
    }

    assert_eq!(world.reopened().tasks.len(), 3);
}

/// Every table is read at one moment. Read a statement at a time, a load
/// could take the tasks from after another process's commit and the meta
/// table from before it, and the count the writer keeps beside the tasks
/// would not be the number of tasks.
#[test]
fn a_load_reads_one_moment_of_the_database() {
    let world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let path = &world.path;
    let now = &world.now;
    let stop = AtomicBool::new(false);
    let reader = Sqlite::open(path).expect("the reader");

    std::thread::scope(|scope| {
        // The writer goes on until the reader has what it came for, so
        // the two overlap however the threads are scheduled.
        let writer = scope.spawn(|| {
            let mut store = Sqlite::open(path).expect("the writer");
            let mut model = store.load().expect("load");
            let mut number = 0;
            while !stop.load(Ordering::Relaxed) {
                let mut change = domain::apply(
                    &model,
                    Command::AddTask {
                        title: format!("Task {number}"),
                        place: Place::Backlog,
                    },
                    &ctx(now),
                )
                .expect("the task");
                number += 1;
                // A row in another table, written in the same transaction,
                // saying how many tasks there are once this one is in.
                change.writes.push(Write::SetMeta {
                    key: TASK_COUNT.to_owned(),
                    value: number.to_string(),
                });
                store.commit(&change).expect("the writer's change");
                model.apply(&change);
            }
        });
        // The scope joins the writer on the way out, so the flag has to
        // go up even when the reader panics.
        let _stop = StopOnDrop(&stop);

        let mut sightings = 0;
        let mut last = 0;
        // A writer that finishes unasked has panicked, which the scope
        // reports once the reader lets go.
        while sightings < SIGHTINGS && !writer.is_finished() {
            let model = reader.load().expect("load");
            assert_eq!(
                model.tasks.len(),
                task_count(&model),
                "a model from two moments of the database"
            );
            if model.tasks.len() > last {
                last = model.tasks.len();
                sightings += 1;
            }
        }
    });

    let model = reader.load().expect("load");
    assert_eq!(model.tasks.len(), task_count(&model));
    assert!(model.tasks.len() >= SIGHTINGS);
}

/// The `meta` key the coherence test keeps its count under. Storage does
/// not care what a key means, and no build reads this one.
const TASK_COUNT: &str = "tasks_written";

/// How many different moments of the writer's run the reader loads before
/// it lets the writer stop. Each is a load that landed between two commits.
const SIGHTINGS: usize = 20;

/// The number of tasks the writer says there are.
fn task_count(model: &Model) -> usize {
    model
        .meta
        .get(TASK_COUNT)
        .map_or(0, |count| count.parse().expect("a count"))
}

/// Raises the flag when it goes out of scope, by return or by panic.
struct StopOnDrop<'a>(&'a AtomicBool);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
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

#[test]
fn simultaneous_open_migrates_once() {
    for version in [0, 1, 2, 3] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("race.db");
        let conn = Connection::open(&path).unwrap();
        for (_, sql) in MIGRATIONS.iter().filter(|(n, _)| *n <= version) {
            conn.execute_batch(sql).unwrap();
        }
        drop(conn);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..12)
                .map(|_| {
                    let barrier = barrier.clone();
                    let path = path.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        let store = Sqlite::open(&path).expect("concurrent open");
                        assert_eq!(store.load().unwrap(), Model::empty());
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
        });
    }
}

#[test]
fn migration_failure_rolls_back_the_whole_sequence() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE settings (collision TEXT)")
        .unwrap();
    assert!(migrate(&mut conn).is_err());
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'tasks'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert!(conn.is_autocommit());
}

#[test]
fn undo_identity_survives_pop_reopen_and_truncation() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    world.add("A", Place::Backlog);
    let first = world.model.undo.last().unwrap().id;
    let undone = domain::undo(&world.model, &ctx(&world.now)).unwrap();
    world.commit(&undone.change);
    world.store = Sqlite::open(&world.path).unwrap();
    world.model = world.store.load().unwrap();
    world.add("B", Place::Backlog);
    assert!(world.model.undo.last().unwrap().id > first);
    let second = world.model.undo.last().unwrap().id;
    world.commit(&Change {
        writes: vec![Write::TruncateUndo(0)],
    });
    world.add("C", Place::Backlog);
    assert!(world.model.undo.last().unwrap().id > second);
}

#[test]
fn undo_migration_preserves_existing_rows_and_initializes_from_stack() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let task = world.add("Scheduled sentinel", day("2026-09-07"));
    world.run(Command::CreateSchedule {
        task,
        rule: Rule::Weekly {
            weekdays: vec![domain::Weekday::Mon],
        },
    });
    world.run(Command::CreateNote);
    world.run(Command::EditNote {
        note: 1,
        body: "Sentinel note".to_owned(),
    });
    let change = domain::add_dictionary_word(&world.model, "Sentinel").unwrap();
    world.commit(&change);
    world.commit(&set_meta("review_on", "2026-09-07"));
    // Recreate the immediately preceding schema with representative real data.
    world
        .store
        .conn
        .execute_batch("DROP TRIGGER undo_identity_guard; DROP TRIGGER undo_identity_advance; DELETE FROM meta WHERE key = 'undo_high_water'; ALTER TABLE notes DROP COLUMN archived_at; PRAGMA user_version = 3;")
        .unwrap();
    let before = world.model.clone();
    let upgraded = Sqlite::open(&world.path).unwrap();
    assert_eq!(upgraded.load().unwrap(), before);
    assert_eq!(
        upgraded
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(
        !upgraded
            .conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
}

#[test]
fn failed_writes_do_not_consume_undo_identities() {
    let mut world = Scratch::new("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    world.add("Before", Place::Backlog);
    let before = world.model.clone();
    let mut change = domain::apply(
        &world.model,
        Command::AddTask {
            title: "Rejected".to_owned(),
            place: Place::Backlog,
        },
        &ctx(&world.now),
    )
    .unwrap();
    change.writes.push(Write::PutPlacement(Placement {
        task_id: 404,
        day: "2026-09-07".parse().unwrap(),
        placed_at: world.now.clone(),
        from_place: FromPlace::New,
    }));
    assert!(world.store.commit(&change).is_err());
    assert_eq!(world.store.load().unwrap(), before);
    world.add("After", Place::Backlog);
    assert_eq!(world.model.undo.last().unwrap().id, 2);
}

#[test]
fn opening_under_a_held_write_lock_returns_a_bounded_conflict() {
    let (dir, mut store) = scratch();
    let path = dir.path().join("jobsdone.db");
    let _held = store
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let started = std::time::Instant::now();
    assert!(matches!(Sqlite::open(&path), Err(StoreError::Conflict)));
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(6));
}

#[test]
fn an_already_open_old_client_cannot_reuse_an_undo_identity_after_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("upgrade.db");
    let old = Connection::open(&path).unwrap();
    for (_, sql) in MIGRATIONS.iter().filter(|(n, _)| *n < 4) {
        old.execute_batch(sql).unwrap();
    }
    let mut new = Sqlite::open(&path).unwrap();
    let now = at("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
    let change = domain::apply(
        &new.load().unwrap(),
        Command::AddTask {
            title: "A".to_owned(),
            place: Place::Backlog,
        },
        &ctx(&now),
    )
    .unwrap();
    new.commit(&change).unwrap();
    // Keep the old connection open across migration, then mimic its SQL.
    let entry = new.load().unwrap().undo[0].clone();
    let inverse = serde_json::to_string(&entry.inverse).unwrap();
    old.execute("DELETE FROM undo_log", []).unwrap();
    assert!(
        old.execute(
            "INSERT INTO undo_log (id, at, label, inverse) VALUES (1, ?1, ?2, ?3)",
            params![entry.at.to_string(), entry.label, inverse]
        )
        .is_err()
    );
    assert!(new.load().unwrap().undo.is_empty());
    old.execute(
        "INSERT INTO undo_log (id, at, label, inverse) VALUES (2, ?1, ?2, ?3)",
        params![entry.at.to_string(), entry.label, inverse],
    )
    .unwrap();
    let model = new.load().unwrap();
    assert_eq!(model.meta["undo_high_water"], "2");
    let change = domain::apply(
        &model,
        Command::AddTask {
            title: "B".to_owned(),
            place: Place::Backlog,
        },
        &ctx(&now),
    )
    .unwrap();
    new.commit(&change).unwrap();
    assert_eq!(new.load().unwrap().undo.last().unwrap().id, 3);
}

#[test]
fn a_missing_or_invalid_persisted_counter_never_becomes_a_fresh_allocator() {
    for value in [None, Some("bad"), Some("-1"), Some("9223372036854775808")] {
        let (_dir, store) = scratch();
        store
            .conn
            .execute("DELETE FROM meta WHERE key = 'undo_high_water'", [])
            .unwrap();
        if let Some(value) = value {
            store
                .conn
                .execute(
                    "INSERT INTO meta (key, value) VALUES ('undo_high_water', ?1)",
                    [value],
                )
                .unwrap();
        }
        let model = store.load().unwrap();
        let now = at("2026-09-07T09:00:00+02:00[Europe/Copenhagen]");
        assert!(
            domain::apply(
                &model,
                Command::AddTask {
                    title: "Rejected".to_owned(),
                    place: Place::Backlog
                },
                &ctx(&now)
            )
            .is_err()
        );
        assert!(
            store
                .conn
                .execute(
                    "INSERT INTO undo_log (id, at, label, inverse) VALUES (1, '', '', '')",
                    []
                )
                .is_err()
        );
        assert!(store.load().unwrap().undo.is_empty());
    }
}
