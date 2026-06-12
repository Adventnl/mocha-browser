//! Visit-history storage.

use mocha_error::MochaResult;
use mocha_url::Url;
use rusqlite::Connection;

use crate::storage_err;

/// One history row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub visit_count: i64,
    pub last_visited_ms: i64,
}

/// Records and queries page visits. Borrows the profile's connection.
pub struct HistoryStore<'a> {
    conn: &'a Connection,
}

impl<'a> HistoryStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        HistoryStore { conn }
    }

    /// Record a visit to `url`. The first visit inserts a row with
    /// `visit_count = 1`; later visits to the same URL increment the count and
    /// update `last_visited_ms` (and the title if a new one is given).
    pub fn record_visit(&self, url: &Url, title: Option<&str>, now_ms: i64) -> MochaResult<()> {
        let key = url.normalized();
        let existing: Option<i64> = self
            .conn
            .query_row("SELECT id FROM history WHERE url = ?1", [&key], |r| {
                r.get(0)
            })
            .ok();
        match existing {
            Some(id) => {
                self.conn
                    .execute(
                        "UPDATE history
                         SET visit_count = visit_count + 1,
                             last_visited_ms = ?2,
                             title = COALESCE(?3, title)
                         WHERE id = ?1",
                        rusqlite::params![id, now_ms, title],
                    )
                    .map_err(storage_err)?;
            }
            None => {
                self.conn
                    .execute(
                        "INSERT INTO history (url, title, visit_count, last_visited_ms)
                         VALUES (?1, ?2, 1, ?3)",
                        rusqlite::params![key, title, now_ms],
                    )
                    .map_err(storage_err)?;
            }
        }
        Ok(())
    }

    /// The most recently visited entries, newest first.
    pub fn recent_visits(&self, limit: usize) -> MochaResult<Vec<HistoryEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, url, title, visit_count, last_visited_ms
                 FROM history ORDER BY last_visited_ms DESC, id DESC LIMIT ?1",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([limit as i64], row_to_entry)
            .map_err(storage_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage_err)
    }

    /// Look up the single entry for `url`, if any.
    pub fn find_by_url(&self, url: &Url) -> MochaResult<Option<HistoryEntry>> {
        let key = url.normalized();
        self.conn
            .query_row(
                "SELECT id, url, title, visit_count, last_visited_ms
                 FROM history WHERE url = ?1",
                [&key],
                row_to_entry,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(storage_err(other)),
            })
    }

    /// Delete all history.
    pub fn clear_history(&self) -> MochaResult<()> {
        self.conn
            .execute("DELETE FROM history", [])
            .map_err(storage_err)?;
        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        visit_count: row.get(3)?,
        last_visited_ms: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profile;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn records_a_new_visit() {
        let p = Profile::private().unwrap();
        let h = p.history();
        h.record_visit(&url("http://example.com/a"), Some("A"), 1000)
            .unwrap();
        let entry = h
            .find_by_url(&url("http://example.com/a"))
            .unwrap()
            .unwrap();
        assert_eq!(entry.visit_count, 1);
        assert_eq!(entry.title.as_deref(), Some("A"));
        assert_eq!(entry.last_visited_ms, 1000);
    }

    #[test]
    fn repeat_visit_increments_count_and_updates_time() {
        let p = Profile::private().unwrap();
        let h = p.history();
        let u = url("http://example.com/a");
        h.record_visit(&u, Some("A"), 1000).unwrap();
        h.record_visit(&u, None, 2000).unwrap();
        let entry = h.find_by_url(&u).unwrap().unwrap();
        assert_eq!(entry.visit_count, 2);
        assert_eq!(entry.last_visited_ms, 2000);
        // The title is preserved when a later visit gives none.
        assert_eq!(entry.title.as_deref(), Some("A"));
    }

    #[test]
    fn recent_visits_are_sorted_newest_first() {
        let p = Profile::private().unwrap();
        let h = p.history();
        h.record_visit(&url("http://a.com/"), None, 100).unwrap();
        h.record_visit(&url("http://b.com/"), None, 300).unwrap();
        h.record_visit(&url("http://c.com/"), None, 200).unwrap();
        let recent = h.recent_visits(10).unwrap();
        let urls: Vec<_> = recent.iter().map(|e| e.url.as_str()).collect();
        assert_eq!(
            urls,
            vec!["http://b.com/", "http://c.com/", "http://a.com/"]
        );
        assert_eq!(h.recent_visits(2).unwrap().len(), 2);
    }

    #[test]
    fn clear_history_empties_it() {
        let p = Profile::private().unwrap();
        let h = p.history();
        h.record_visit(&url("http://a.com/"), None, 100).unwrap();
        h.clear_history().unwrap();
        assert!(h.recent_visits(10).unwrap().is_empty());
    }

    #[test]
    fn missing_url_returns_none() {
        let p = Profile::private().unwrap();
        assert!(p
            .history()
            .find_by_url(&url("http://nope.com/"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn persistent_history_survives_reopen_but_private_does_not() {
        use crate::testutil::TempDir;
        let dir = TempDir::new();
        let u = url("http://example.com/a");
        {
            let p = Profile::persistent(dir.path()).unwrap();
            p.history().record_visit(&u, Some("A"), 1000).unwrap();
        }
        // Reopen the persistent profile: the visit is still there.
        let p = Profile::persistent(dir.path()).unwrap();
        assert!(p.history().find_by_url(&u).unwrap().is_some());

        // A private profile never persists: a fresh one is empty.
        let priv1 = Profile::private().unwrap();
        priv1.history().record_visit(&u, None, 1).unwrap();
        let priv2 = Profile::private().unwrap();
        assert!(priv2.history().find_by_url(&u).unwrap().is_none());
    }
}
