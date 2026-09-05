//! The SQLite implementation of `Store`: migrations, loading the model,
//! committing a change, reporting the version.

use std::path::Path;

use rusqlite::{Connection, ErrorCode, params};

use crate::domain::{Change, Model, Store, StoreError, Write};

#[cfg(test)]
mod tests;

/// The migrations, in order, compiled into the binary. Adding one is a
/// line here and a file beside the others; the file sets `user_version`
/// as its last statement.
const MIGRATIONS: &[(u32, &str)] = &[(1, include_str!("../migrations/0001_initial.sql"))];

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

        let mut statement = self.conn.prepare("SELECT key, value FROM meta")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        for row in rows {
            let (key, value) = row?;
            model.meta.insert(key, value);
        }

        Ok(model)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        for write in &change.writes {
            match write {
                Write::SetMeta { key, value } => {
                    tx.execute(
                        "INSERT INTO meta (key, value) VALUES (?1, ?2)
                         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                        params![key, value],
                    )?;
                }
            }
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
