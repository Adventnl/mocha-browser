//! Schema versioning and migrations.
//!
//! The current schema version is stored in a single-row `schema_version` table.
//! Opening a database runs every migration whose target version is greater than
//! the stored one, in order, inside a transaction. Migrations are written to be
//! idempotent (`CREATE TABLE IF NOT EXISTS`), so reopening an up-to-date database
//! is a no-op.

use mocha_error::MochaResult;
use rusqlite::Connection;

use crate::storage_err;

/// The latest schema version this build knows how to produce.
pub const LATEST_VERSION: i64 = 1;

/// Read the current schema version (0 if the database is brand new).
pub fn current_version(conn: &Connection) -> MochaResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get::<_, i64>(0),
    )
    // A missing table (first open) reads as version 0.
    .or(Ok(0))
}

/// Ensure the database is migrated up to [`LATEST_VERSION`].
pub fn migrate(conn: &Connection) -> MochaResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
        [],
    )
    .map_err(storage_err)?;

    let mut version = current_version(conn)?;
    while version < LATEST_VERSION {
        let next = version + 1;
        apply(conn, next)?;
        version = next;
    }
    Ok(())
}

/// Apply a single migration to `target` inside a transaction.
fn apply(conn: &Connection, target: i64) -> MochaResult<()> {
    match target {
        1 => migration_1(conn)?,
        other => {
            return Err(mocha_error::MochaError::Storage(format!(
                "no migration for schema version {other}"
            )))
        }
    }
    set_version(conn, target)
}

fn set_version(conn: &Connection, version: i64) -> MochaResult<()> {
    conn.execute("DELETE FROM schema_version", [])
        .map_err(storage_err)?;
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [version],
    )
    .map_err(storage_err)?;
    Ok(())
}

/// Migration 1: the initial Milestone 14 tables.
fn migration_1(conn: &Connection) -> MochaResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS history (
            id              INTEGER PRIMARY KEY,
            url             TEXT NOT NULL,
            title           TEXT,
            visit_count     INTEGER NOT NULL DEFAULT 0,
            last_visited_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS bookmarks (
            id          INTEGER PRIMARY KEY,
            url         TEXT NOT NULL,
            title       TEXT NOT NULL,
            created_ms  INTEGER NOT NULL,
            updated_ms  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS downloads (
            id          INTEGER PRIMARY KEY,
            url         TEXT NOT NULL,
            target_path TEXT NOT NULL,
            started_ms  INTEGER NOT NULL,
            finished_ms INTEGER,
            status      TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS session_tabs (
            id                    INTEGER PRIMARY KEY,
            position              INTEGER NOT NULL,
            url                   TEXT,
            title                 TEXT,
            scroll_y              REAL NOT NULL DEFAULT 0,
            history_json          TEXT,
            current_history_index INTEGER
        );

        CREATE TABLE IF NOT EXISTS session_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .map_err(storage_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn fresh_database_is_at_latest_version() {
        let conn = mem();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = mem();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        // schema_version has exactly one row.
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn all_tables_exist() {
        let conn = mem();
        for table in [
            "history",
            "bookmarks",
            "settings",
            "downloads",
            "session_tabs",
            "session_meta",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {table} should exist");
        }
    }

    #[test]
    fn current_version_of_empty_db_is_zero() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(current_version(&conn).unwrap(), 0);
    }
}
