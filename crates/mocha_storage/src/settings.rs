//! Key/value settings storage.
//!
//! Values are stored as text. Booleans use the strings `"true"`/`"false"`.
//! Common keys an embedder might use: `homepage`, `last_window_width`,
//! `last_window_height`, `restore_session`.

use mocha_error::MochaResult;
use rusqlite::Connection;

use crate::storage_err;

/// A simple string/bool settings table. Borrows the profile's connection.
pub struct SettingsStore<'a> {
    conn: &'a Connection,
}

impl<'a> SettingsStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        SettingsStore { conn }
    }

    /// Set a string value (upsert).
    pub fn set_string(&self, key: &str, value: &str) -> MochaResult<()> {
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    /// Get a string value, or `None` if the key is unset.
    pub fn get_string(&self, key: &str) -> MochaResult<Option<String>> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(storage_err(other)),
            })
    }

    /// Set a boolean value (stored as `"true"`/`"false"`).
    pub fn set_bool(&self, key: &str, value: bool) -> MochaResult<()> {
        self.set_string(key, if value { "true" } else { "false" })
    }

    /// Get a boolean value, or `None` if unset. A stored value other than
    /// `"true"` reads as `false`.
    pub fn get_bool(&self, key: &str) -> MochaResult<Option<bool>> {
        Ok(self.get_string(key)?.map(|v| v == "true"))
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::TempDir;
    use crate::Profile;

    #[test]
    fn set_and_get_string() {
        let p = Profile::private().unwrap();
        let s = p.settings();
        s.set_string("homepage", "http://start/").unwrap();
        assert_eq!(
            s.get_string("homepage").unwrap().as_deref(),
            Some("http://start/")
        );
        // Upsert overwrites.
        s.set_string("homepage", "http://other/").unwrap();
        assert_eq!(
            s.get_string("homepage").unwrap().as_deref(),
            Some("http://other/")
        );
    }

    #[test]
    fn set_and_get_bool() {
        let p = Profile::private().unwrap();
        let s = p.settings();
        s.set_bool("restore_session", true).unwrap();
        assert_eq!(s.get_bool("restore_session").unwrap(), Some(true));
        s.set_bool("restore_session", false).unwrap();
        assert_eq!(s.get_bool("restore_session").unwrap(), Some(false));
    }

    #[test]
    fn missing_key_is_none() {
        let p = Profile::private().unwrap();
        assert_eq!(p.settings().get_string("nope").unwrap(), None);
        assert_eq!(p.settings().get_bool("nope").unwrap(), None);
    }

    #[test]
    fn settings_persist_across_reopen() {
        let dir = TempDir::new();
        {
            let p = Profile::persistent(dir.path()).unwrap();
            p.settings().set_string("homepage", "http://h/").unwrap();
        }
        let p = Profile::persistent(dir.path()).unwrap();
        assert_eq!(
            p.settings().get_string("homepage").unwrap().as_deref(),
            Some("http://h/")
        );
    }

    #[test]
    fn private_settings_not_persisted() {
        let p1 = Profile::private().unwrap();
        p1.settings().set_string("homepage", "http://h/").unwrap();
        let p2 = Profile::private().unwrap();
        assert_eq!(p2.settings().get_string("homepage").unwrap(), None);
    }
}
