//! Origin-keyed persistent `localStorage` (Milestone 15).
//!
//! Keys and values are strings, scoped by [`Origin`]. Different origins are
//! isolated. A private profile keeps entries only in memory. There are **no
//! quotas**, no `StorageEvent`, and `file://` has no tuple origin (so storing for
//! a `file://` document is rejected upstream via [`Origin::from_url`]). The
//! `updated_ms` column is reserved metadata and currently written as `0` (this
//! layer takes no timestamp, matching the web `setItem(key, value)` signature).

use mocha_error::MochaResult;
use mocha_origin::Origin;
use rusqlite::Connection;

use crate::storage_err;

/// Persistent, origin-keyed key/value web storage. Borrows the connection.
pub struct LocalStorageStore<'a> {
    conn: &'a Connection,
}

impl<'a> LocalStorageStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        LocalStorageStore { conn }
    }

    /// Get an item for `origin`, or `None`.
    pub fn get_item(&self, origin: &Origin, key: &str) -> MochaResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM local_storage WHERE origin = ?1 AND key = ?2",
                rusqlite::params![origin.storage_key(), key],
                |r| r.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(storage_err(other)),
            })
    }

    /// Set an item for `origin` (upsert).
    pub fn set_item(&self, origin: &Origin, key: &str, value: &str) -> MochaResult<()> {
        self.conn
            .execute(
                "INSERT INTO local_storage (origin, key, value, updated_ms)
                 VALUES (?1, ?2, ?3, 0)
                 ON CONFLICT(origin, key) DO UPDATE SET value = excluded.value",
                rusqlite::params![origin.storage_key(), key, value],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    /// Remove a single item for `origin`.
    pub fn remove_item(&self, origin: &Origin, key: &str) -> MochaResult<()> {
        self.conn
            .execute(
                "DELETE FROM local_storage WHERE origin = ?1 AND key = ?2",
                rusqlite::params![origin.storage_key(), key],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    /// Remove all items for `origin`.
    pub fn clear_origin(&self, origin: &Origin) -> MochaResult<()> {
        self.conn
            .execute(
                "DELETE FROM local_storage WHERE origin = ?1",
                [origin.storage_key()],
            )
            .map_err(storage_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use crate::Profile;
    use mocha_url::Url;

    fn origin(s: &str) -> Origin {
        Origin::from_url(&Url::parse(s).unwrap()).unwrap()
    }

    #[test]
    fn set_get_and_remove() {
        let p = Profile::private().unwrap();
        let ls = p.local_storage();
        let o = origin("http://a.com/");
        ls.set_item(&o, "k", "v").unwrap();
        assert_eq!(ls.get_item(&o, "k").unwrap().as_deref(), Some("v"));
        // Upsert overwrites.
        ls.set_item(&o, "k", "v2").unwrap();
        assert_eq!(ls.get_item(&o, "k").unwrap().as_deref(), Some("v2"));
        ls.remove_item(&o, "k").unwrap();
        assert_eq!(ls.get_item(&o, "k").unwrap(), None);
    }

    #[test]
    fn origins_are_isolated() {
        let p = Profile::private().unwrap();
        let ls = p.local_storage();
        let a = origin("http://a.com/");
        let b = origin("http://b.com/");
        ls.set_item(&a, "k", "from_a").unwrap();
        assert_eq!(ls.get_item(&b, "k").unwrap(), None);
        ls.set_item(&b, "k", "from_b").unwrap();
        assert_eq!(ls.get_item(&a, "k").unwrap().as_deref(), Some("from_a"));
        assert_eq!(ls.get_item(&b, "k").unwrap().as_deref(), Some("from_b"));
    }

    #[test]
    fn clear_origin_only_clears_that_origin() {
        let p = Profile::private().unwrap();
        let ls = p.local_storage();
        let a = origin("http://a.com/");
        let b = origin("http://b.com/");
        ls.set_item(&a, "k", "1").unwrap();
        ls.set_item(&b, "k", "2").unwrap();
        ls.clear_origin(&a).unwrap();
        assert_eq!(ls.get_item(&a, "k").unwrap(), None);
        assert_eq!(ls.get_item(&b, "k").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn persists_across_reopen_but_private_does_not() {
        let dir = TempDir::new();
        let o = origin("http://a.com/");
        {
            let p = Profile::persistent(dir.path()).unwrap();
            p.local_storage().set_item(&o, "k", "v").unwrap();
        }
        let p = Profile::persistent(dir.path()).unwrap();
        assert_eq!(
            p.local_storage().get_item(&o, "k").unwrap().as_deref(),
            Some("v")
        );

        let pr1 = Profile::private().unwrap();
        pr1.local_storage().set_item(&o, "k", "v").unwrap();
        let pr2 = Profile::private().unwrap();
        assert_eq!(pr2.local_storage().get_item(&o, "k").unwrap(), None);
    }
}
