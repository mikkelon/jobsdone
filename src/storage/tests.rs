use super::*;

use crate::domain::Change;

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

#[test]
fn a_fresh_database_is_migrated_to_the_latest_schema() {
    let (_dir, store) = scratch();
    let user_version: u32 = store
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version");

    assert_eq!(user_version, 1);
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
            "tasks",
            "undo_log"
        ]
    );
}

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
