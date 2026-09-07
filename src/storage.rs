//! The SQLite implementation of `Store`: migrations, loading the model,
//! committing a change, reporting the version.
//!
//! Storage is a dumb executor of changes. It decides nothing: every row
//! it writes was decided by the domain, and every value it reads is
//! turned back into the type the domain named.

use std::path::Path;

use rusqlite::types::Type;
use rusqlite::{Connection, ErrorCode, Row, Transaction, params};

use crate::domain::{
    Change, Command, FromPlace, Model, Note, Placement, Rule, Schedule, Settings, Store,
    StoreError, Task, UndoEntry, Write,
};

#[cfg(test)]
mod tests;

/// The migrations, in order, compiled into the binary. Adding one is a
/// line here and a file beside the others; the file sets `user_version`
/// as its last statement.
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (2, include_str!("../migrations/0002_settings.sql")),
    (
        3,
        include_str!("../migrations/0003_personal_dictionary.sql"),
    ),
];

/// `placements.from_place` is `new`, `backlog`, or the date the task came
/// from, which is the one column that is not a plain value.
const FROM_NEW: &str = "new";
const FROM_BACKLOG: &str = "backlog";

pub struct Sqlite {
    conn: Connection,
}

impl Sqlite {
    /// Opens or creates the database, sets the pragmas, applies pending
    /// migrations.
    pub fn open(path: &Path) -> Result<Sqlite, StoreError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&conn)?;
        Ok(Sqlite { conn })
    }
}

/// Applies every migration the database has not seen.
///
/// A database from the future is refused rather than opened: an older
/// binary writing rows against a schema it does not understand is the one
/// way this design can lose data, and downgrades are not supported
/// (DOMAIN.md section 17).
fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let latest = MIGRATIONS.last().map_or(0, |(number, _)| *number);

    if current > latest {
        return Err(StoreError::Other(format!(
            "the database is at schema {current} and this build of jobsdone only knows {latest}; \
             it was written by a newer version"
        )));
    }

    for (number, sql) in MIGRATIONS {
        if *number > current {
            conn.execute_batch(&format!("BEGIN;\n{sql}\nCOMMIT;"))?;
        }
    }
    Ok(())
}

impl Store for Sqlite {
    fn load(&self) -> Result<Model, StoreError> {
        let mut model = Model::empty();

        let mut tasks = self.conn.prepare(
            "SELECT id, title, day, position, focus, waiting, closed_at, due_on, remind_on,
                    schedule_id, scheduled_on, created_at, deleted_at
             FROM tasks",
        )?;
        for row in tasks.query_map([], read_task)? {
            let task = row?;
            model.tasks.insert(task.id, task);
        }

        let mut placements = self
            .conn
            .prepare("SELECT task_id, day, placed_at, from_place FROM placements")?;
        for row in placements.query_map([], read_placement)? {
            let placement = row?;
            model
                .placements
                .insert((placement.task_id, placement.day), placement);
        }

        let mut schedules = self.conn.prepare(
            "SELECT id, title, rule, generated_through, stopped_on, created_at FROM schedules",
        )?;
        for row in schedules.query_map([], read_schedule)? {
            let schedule = row?;
            model.schedules.insert(schedule.id, schedule);
        }

        let mut notes = self
            .conn
            .prepare("SELECT id, body, created_at, updated_at, deleted_at FROM notes")?;
        for row in notes.query_map([], read_note)? {
            let note = row?;
            model.notes.insert(note.id, note);
        }

        let mut undo = self
            .conn
            .prepare("SELECT id, at, label, inverse FROM undo_log ORDER BY id")?;
        for row in undo.query_map([], read_undo)? {
            model.undo.push(row?);
        }

        let mut meta = self.conn.prepare("SELECT key, value FROM meta")?;
        for row in meta.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))? {
            let (key, value) = row?;
            model.meta.insert(key, value);
        }

        let mut settings = self.conn.prepare("SELECT key, value FROM settings")?;
        let rows = settings
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<(String, String)>>>()?;
        model.settings = Settings::from_pairs(rows);

        let mut dictionary = self
            .conn
            .prepare("SELECT key, word FROM personal_dictionary")?;
        for row in dictionary.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))? {
            let (key, word) = row?;
            model.personal_dictionary.insert(key, word);
        }

        Ok(model)
    }

    /// Every write of a change in one transaction, so a change is either
    /// wholly there or not there at all after the process dies.
    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        for write in &change.writes {
            write_row(&tx, write)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn version(&self) -> Result<u64, StoreError> {
        // data_version is an i64 to SQLite; the domain only ever compares
        // it with the last one it saw.
        let version: i64 = self
            .conn
            .query_row("PRAGMA data_version", [], |row| row.get(0))?;
        Ok(version as u64)
    }
}

/// One row write.
///
/// Every put is an upsert rather than `INSERT OR REPLACE`, which would
/// delete the row first and take the placements that reference it with
/// it, or fail the foreign key trying.
fn write_row(tx: &Transaction<'_>, write: &Write) -> Result<(), StoreError> {
    match write {
        Write::PutTask(task) => {
            tx.execute(
                "INSERT INTO tasks (id, title, day, position, focus, waiting, closed_at, due_on,
                                    remind_on, schedule_id, scheduled_on, created_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT (id) DO UPDATE SET
                     title = excluded.title, day = excluded.day, position = excluded.position,
                     focus = excluded.focus, waiting = excluded.waiting,
                     closed_at = excluded.closed_at, due_on = excluded.due_on,
                     remind_on = excluded.remind_on, schedule_id = excluded.schedule_id,
                     scheduled_on = excluded.scheduled_on, created_at = excluded.created_at,
                     deleted_at = excluded.deleted_at",
                params![
                    task.id,
                    task.title,
                    task.day.map(date_text),
                    task.position as i64,
                    task.focus,
                    task.waiting,
                    task.closed_at.as_ref().map(instant_text),
                    task.due_on.map(date_text),
                    task.remind_on.map(date_text),
                    task.schedule_id,
                    task.scheduled_on.map(date_text),
                    instant_text(&task.created_at),
                    task.deleted_at.as_ref().map(instant_text),
                ],
            )?;
        }
        Write::PutPlacement(placement) => {
            tx.execute(
                "INSERT INTO placements (task_id, day, placed_at, from_place)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    placement.task_id,
                    date_text(placement.day),
                    instant_text(&placement.placed_at),
                    from_place_text(placement.from_place),
                ],
            )?;
        }
        Write::DeletePlacement { task, day } => {
            tx.execute(
                "DELETE FROM placements WHERE task_id = ?1 AND day = ?2",
                params![task, date_text(*day)],
            )?;
        }
        Write::PutSchedule(schedule) => {
            tx.execute(
                "INSERT INTO schedules (id, title, rule, generated_through, stopped_on, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (id) DO UPDATE SET
                     title = excluded.title, rule = excluded.rule,
                     generated_through = excluded.generated_through,
                     stopped_on = excluded.stopped_on, created_at = excluded.created_at",
                params![
                    schedule.id,
                    schedule.title,
                    rule_text(&schedule.rule)?,
                    date_text(schedule.generated_through),
                    schedule.stopped_on.map(date_text),
                    instant_text(&schedule.created_at),
                ],
            )?;
        }
        Write::DeleteSchedule(id) => {
            tx.execute("DELETE FROM schedules WHERE id = ?1", params![id])?;
        }
        Write::PutNote(note) => {
            tx.execute(
                "INSERT INTO notes (id, body, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (id) DO UPDATE SET
                     body = excluded.body, created_at = excluded.created_at,
                     updated_at = excluded.updated_at, deleted_at = excluded.deleted_at",
                params![
                    note.id,
                    note.body,
                    instant_text(&note.created_at),
                    instant_text(&note.updated_at),
                    note.deleted_at.as_ref().map(instant_text),
                ],
            )?;
        }
        Write::PushUndo(entry) => {
            tx.execute(
                "INSERT INTO undo_log (id, at, label, inverse) VALUES (?1, ?2, ?3, ?4)",
                params![
                    entry.id,
                    instant_text(&entry.at),
                    entry.label,
                    command_text(&entry.inverse)?,
                ],
            )?;
        }
        Write::PopUndo(id) => {
            tx.execute("DELETE FROM undo_log WHERE id = ?1", params![id])?;
        }
        Write::TruncateUndo(cap) => {
            tx.execute(
                "DELETE FROM undo_log WHERE id NOT IN
                     (SELECT id FROM undo_log ORDER BY id DESC LIMIT ?1)",
                params![*cap as i64],
            )?;
        }
        Write::SetMeta { key, value } => {
            tx.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        // The whole table at once, so that a key this build does not
        // write is a key it does not keep either. A key it does not know
        // is another matter: the delete takes those too, which is the
        // price of the value being whole.
        Write::PutSettings(settings) => {
            tx.execute("DELETE FROM settings", [])?;
            for (key, value) in settings.to_pairs() {
                tx.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                    params![key, value],
                )?;
            }
        }
        Write::PutDictionaryWord { key, word } => {
            tx.execute(
                "INSERT INTO personal_dictionary (key, word) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET word = excluded.word",
                params![key, word],
            )?;
        }
        Write::DeleteDictionaryWord { key } => {
            tx.execute(
                "DELETE FROM personal_dictionary WHERE key = ?1",
                params![key],
            )?;
        }
    }
    Ok(())
}

// ---- rows in ---------------------------------------------------------

fn read_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    let position: i64 = row.get(3)?;
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        day: date_of(row, 2)?,
        position: position.max(0) as usize,
        focus: row.get(4)?,
        waiting: row.get(5)?,
        closed_at: instant_of(row, 6)?,
        due_on: date_of(row, 7)?,
        remind_on: date_of(row, 8)?,
        schedule_id: row.get(9)?,
        scheduled_on: date_of(row, 10)?,
        created_at: instant_at(row, 11)?,
        deleted_at: instant_of(row, 12)?,
    })
}

fn read_placement(row: &Row<'_>) -> rusqlite::Result<Placement> {
    let day: String = row.get(1)?;
    let from: String = row.get(3)?;
    Ok(Placement {
        task_id: row.get(0)?,
        day: parse(&day, 1)?,
        placed_at: instant_at(row, 2)?,
        from_place: match from.as_str() {
            FROM_NEW => FromPlace::New,
            FROM_BACKLOG => FromPlace::Backlog,
            date => FromPlace::Day(parse(date, 3)?),
        },
    })
}

fn read_schedule(row: &Row<'_>) -> rusqlite::Result<Schedule> {
    let rule: String = row.get(2)?;
    let generated_through: String = row.get(3)?;
    Ok(Schedule {
        id: row.get(0)?,
        title: row.get(1)?,
        rule: serde_json::from_str::<Rule>(&rule)
            .map_err(|error| bad(2, Type::Text, error.to_string()))?,
        generated_through: parse(&generated_through, 3)?,
        stopped_on: date_of(row, 4)?,
        created_at: instant_at(row, 5)?,
    })
}

fn read_note(row: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        body: row.get(1)?,
        created_at: instant_at(row, 2)?,
        updated_at: instant_at(row, 3)?,
        deleted_at: instant_of(row, 4)?,
    })
}

fn read_undo(row: &Row<'_>) -> rusqlite::Result<UndoEntry> {
    let inverse: String = row.get(3)?;
    Ok(UndoEntry {
        id: row.get(0)?,
        at: instant_at(row, 1)?,
        label: row.get(2)?,
        inverse: serde_json::from_str(&inverse)
            .map_err(|error| bad(3, Type::Text, error.to_string()))?,
    })
}

// ---- values in and out -----------------------------------------------

/// Dates are `YYYY-MM-DD`, instants RFC 9557: RFC 3339 with the time zone
/// the instant was recorded in, so that reading one back gives the same
/// value (DOMAIN.md section 2).
fn date_text(date: jiff::civil::Date) -> String {
    date.to_string()
}

fn instant_text(instant: &jiff::Zoned) -> String {
    instant.to_string()
}

fn from_place_text(from: FromPlace) -> String {
    match from {
        FromPlace::New => FROM_NEW.to_owned(),
        FromPlace::Backlog => FROM_BACKLOG.to_owned(),
        FromPlace::Day(day) => date_text(day),
    }
}

fn rule_text(rule: &Rule) -> Result<String, StoreError> {
    serde_json::to_string(rule).map_err(|error| StoreError::Other(error.to_string()))
}

fn command_text(command: &Command) -> Result<String, StoreError> {
    serde_json::to_string(command).map_err(|error| StoreError::Other(error.to_string()))
}

fn parse<T: std::str::FromStr>(text: &str, column: usize) -> rusqlite::Result<T> {
    text.parse().map_err(|_| {
        bad(
            column,
            Type::Text,
            format!("{text:?} is not the right shape"),
        )
    })
}

fn date_of(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<jiff::civil::Date>> {
    match row.get::<_, Option<String>>(column)? {
        Some(text) => Ok(Some(parse(&text, column)?)),
        None => Ok(None),
    }
}

fn instant_at(row: &Row<'_>, column: usize) -> rusqlite::Result<jiff::Zoned> {
    let text: String = row.get(column)?;
    parse(&text, column)
}

fn instant_of(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<jiff::Zoned>> {
    match row.get::<_, Option<String>>(column)? {
        Some(text) => Ok(Some(parse(&text, column)?)),
        None => Ok(None),
    }
}

fn bad(column: usize, kind: Type, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, kind, message.into())
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        let constraint = matches!(
            &error,
            rusqlite::Error::SqliteFailure(failure, _)
                if failure.code == ErrorCode::ConstraintViolation
        );
        if constraint {
            StoreError::Conflict
        } else {
            StoreError::Other(error.to_string())
        }
    }
}
