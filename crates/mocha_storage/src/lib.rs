//! Profile storage for Mocha Browser (Milestone 14).
//!
//! A [`Profile`] is an embedded-SQLite database holding the persistent pieces of
//! a browser profile: visit [`history`], [`bookmarks`], [`settings`], download
//! [`downloads`] metadata, and a persisted [`session`] snapshot. A profile is
//! either **persistent** (a file under a profile directory) or **private** (an
//! in-memory database that is never written to disk).
//!
//! This is **not** a full browser profile: there is no encryption, no
//! concurrency model beyond a single connection, no cookies or origin-keyed web
//! storage (those arrive in Milestone 15), and no sync. The schema is versioned
//! and upgraded by small idempotent [`migrations`].
//!
//! All timestamps are caller-supplied epoch milliseconds (`now_ms: i64`) so the
//! stores stay deterministic and testable — the crate never reads a clock.

use std::path::{Path, PathBuf};

use mocha_error::{MochaError, MochaResult};
use rusqlite::Connection;

pub mod bookmarks;
pub mod cookies;
pub mod downloads;
pub mod history;
pub mod local_storage;
pub mod migrations;
pub mod session;
pub mod session_storage;
pub mod settings;

pub use bookmarks::{BookmarkEntry, BookmarkStore};
pub use cookies::CookieStore;
pub use downloads::{DownloadEntry, DownloadStatus, DownloadStore};
pub use history::{HistoryEntry, HistoryStore};
pub use local_storage::LocalStorageStore;
pub use session::{SessionStore, StoredSession, StoredTab};
pub use session_storage::SessionStorage;
pub use settings::SettingsStore;

/// The file name of the SQLite database inside a persistent profile directory.
pub const DATABASE_FILE_NAME: &str = "mocha.db";

/// Convert a `rusqlite` error into a [`MochaError::Storage`].
pub(crate) fn storage_err(error: rusqlite::Error) -> MochaError {
    MochaError::Storage(error.to_string())
}

/// A validated profile directory (created if missing).
#[derive(Debug, Clone)]
pub struct ProfilePath(PathBuf);

impl ProfilePath {
    /// Validate (and create if missing) a profile directory. Errors if the path
    /// exists but is not a directory.
    pub fn new(path: impl Into<PathBuf>) -> MochaResult<Self> {
        let path = path.into();
        if path.exists() && !path.is_dir() {
            return Err(MochaError::Storage(format!(
                "profile path {} exists but is not a directory",
                path.display()
            )));
        }
        std::fs::create_dir_all(&path).map_err(|e| {
            MochaError::Storage(format!(
                "could not create profile directory {}: {e}",
                path.display()
            ))
        })?;
        Ok(ProfilePath(path))
    }

    /// The profile directory.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// The SQLite database file inside the profile directory.
    pub fn database_file(&self) -> PathBuf {
        self.0.join(DATABASE_FILE_NAME)
    }
}

/// How a profile is backed.
#[derive(Debug, Clone)]
pub enum ProfileMode {
    /// A persistent profile stored under `path` (a directory).
    Persistent { path: PathBuf },
    /// A private profile: an in-memory database, never written to disk.
    Private,
}

/// An open browser profile: a single SQLite connection plus the schema.
///
/// All stores share this one connection (which is required for the private
/// in-memory mode, where every separate connection would be a different
/// database). `rusqlite` methods take `&self`, so the stores borrow it.
pub struct Profile {
    conn: Connection,
    mode: ProfileMode,
}

impl Profile {
    /// Open (or create) a profile in the given mode, running migrations.
    pub fn open(mode: ProfileMode) -> MochaResult<Self> {
        let conn = match &mode {
            ProfileMode::Persistent { path } => {
                let dir = ProfilePath::new(path)?;
                Connection::open(dir.database_file()).map_err(storage_err)?
            }
            ProfileMode::Private => Connection::open_in_memory().map_err(storage_err)?,
        };
        migrations::migrate(&conn)?;
        Ok(Profile { conn, mode })
    }

    /// Open a persistent profile rooted at `path`.
    pub fn persistent(path: impl Into<PathBuf>) -> MochaResult<Self> {
        Profile::open(ProfileMode::Persistent { path: path.into() })
    }

    /// Open a private (in-memory) profile.
    pub fn private() -> MochaResult<Self> {
        Profile::open(ProfileMode::Private)
    }

    /// Whether this profile is private (in-memory, non-persistent).
    pub fn is_private(&self) -> bool {
        matches!(self.mode, ProfileMode::Private)
    }

    /// The current schema version.
    pub fn schema_version(&self) -> MochaResult<i64> {
        migrations::current_version(&self.conn)
    }

    /// The visit-history store.
    pub fn history(&self) -> HistoryStore<'_> {
        HistoryStore::new(&self.conn)
    }

    /// The bookmarks store.
    pub fn bookmarks(&self) -> BookmarkStore<'_> {
        BookmarkStore::new(&self.conn)
    }

    /// The settings (key/value) store.
    pub fn settings(&self) -> SettingsStore<'_> {
        SettingsStore::new(&self.conn)
    }

    /// The downloads-metadata store.
    pub fn downloads(&self) -> DownloadStore<'_> {
        DownloadStore::new(&self.conn)
    }

    /// The persisted-session store.
    pub fn session(&self) -> SessionStore<'_> {
        SessionStore::new(&self.conn)
    }

    /// The persistent cookie store (Milestone 15).
    pub fn cookies(&self) -> CookieStore<'_> {
        CookieStore::new(&self.conn)
    }

    /// The persistent, origin-keyed `localStorage` store (Milestone 15).
    pub fn local_storage(&self) -> LocalStorageStore<'_> {
        LocalStorageStore::new(&self.conn)
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A unique, not-yet-created temporary directory path for a persistent
    /// profile. The caller may pass it to [`super::Profile::persistent`], which
    /// creates it. Removed by [`TempDir`]'s `Drop`.
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new() -> TempDir {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let pid = std::process::id();
            let path = std::env::temp_dir().join(format!("mocha_storage_test_{pid}_{n}"));
            TempDir(path)
        }

        pub fn path(&self) -> &PathBuf {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::TempDir;
    use super::*;

    #[test]
    fn persistent_profile_creates_directory_and_db() {
        let dir = TempDir::new();
        assert!(!dir.path().exists());
        let profile = Profile::persistent(dir.path()).unwrap();
        assert!(dir.path().is_dir(), "profile directory created");
        assert!(
            dir.path().join(DATABASE_FILE_NAME).exists(),
            "database file created"
        );
        assert!(!profile.is_private());
    }

    #[test]
    fn profile_path_errors_when_path_is_a_file() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.path()).unwrap();
        let file = dir.path().join("not_a_dir");
        std::fs::write(&file, b"x").unwrap();
        let err = ProfilePath::new(&file).unwrap_err();
        assert!(matches!(err, MochaError::Storage(_)));
    }

    #[test]
    fn private_profile_creates_no_files() {
        let profile = Profile::private().unwrap();
        assert!(profile.is_private());
        // It works (has a schema) but is purely in memory.
        assert!(profile.schema_version().unwrap() >= 1);
    }

    #[test]
    fn persistent_profile_reopens_with_same_schema() {
        let dir = TempDir::new();
        {
            let p = Profile::persistent(dir.path()).unwrap();
            assert!(p.schema_version().unwrap() >= 1);
        }
        // Reopen: migrations are idempotent; the version is unchanged.
        let p = Profile::persistent(dir.path()).unwrap();
        assert_eq!(p.schema_version().unwrap(), migrations::LATEST_VERSION);
    }
}
