//! Persisted-session storage.
//!
//! These DTOs intentionally mirror the desktop shell's in-memory
//! `SessionSnapshot`/`SessionTab` (Milestone 13) **without** depending on
//! `mocha_desktop` — the desktop crate converts to/from these types. Per-tab
//! history is stored as a newline-joined string in `session_tabs.history_json`
//! (Mocha's normalized URLs never contain newlines), avoiding a JSON dependency.

use mocha_error::MochaResult;
use rusqlite::Connection;

use crate::storage_err;

/// A persisted session: tabs plus the active-tab index.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSession {
    pub tabs: Vec<StoredTab>,
    pub active_tab_index: usize,
}

/// Per-tab persisted metadata (no DOM/layout/form state).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredTab {
    pub url: Option<String>,
    pub title: String,
    pub scroll_y: f32,
    pub history: Vec<String>,
    pub current_history_index: Option<usize>,
}

const ACTIVE_TAB_KEY: &str = "active_tab_index";

/// Saves and loads the last session. Borrows the profile's connection.
pub struct SessionStore<'a> {
    conn: &'a Connection,
}

impl<'a> SessionStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        SessionStore { conn }
    }

    /// Replace the saved session with `snapshot`.
    pub fn save_session(&self, snapshot: &StoredSession) -> MochaResult<()> {
        let tx = self.conn.unchecked_transaction().map_err(storage_err)?;
        tx.execute("DELETE FROM session_tabs", [])
            .map_err(storage_err)?;
        for (position, tab) in snapshot.tabs.iter().enumerate() {
            tx.execute(
                "INSERT INTO session_tabs
                   (position, url, title, scroll_y, history_json, current_history_index)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    position as i64,
                    tab.url,
                    tab.title,
                    tab.scroll_y as f64,
                    encode_history(&tab.history),
                    tab.current_history_index.map(|i| i as i64),
                ],
            )
            .map_err(storage_err)?;
        }
        tx.execute(
            "INSERT INTO session_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![ACTIVE_TAB_KEY, snapshot.active_tab_index.to_string()],
        )
        .map_err(storage_err)?;
        tx.commit().map_err(storage_err)?;
        Ok(())
    }

    /// Load the saved session, or `None` if there are no saved tabs.
    pub fn load_session(&self) -> MochaResult<Option<StoredSession>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT url, title, scroll_y, history_json, current_history_index
                 FROM session_tabs ORDER BY position ASC, id ASC",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |row| {
                let url: Option<String> = row.get(0)?;
                let title: Option<String> = row.get(1)?;
                let scroll_y: f64 = row.get(2)?;
                let history_json: Option<String> = row.get(3)?;
                let current: Option<i64> = row.get(4)?;
                Ok(StoredTab {
                    url,
                    title: title.unwrap_or_default(),
                    scroll_y: scroll_y as f32,
                    history: decode_history(history_json.as_deref()),
                    current_history_index: current.map(|i| i as usize),
                })
            })
            .map_err(storage_err)?;
        let tabs = rows.collect::<Result<Vec<_>, _>>().map_err(storage_err)?;
        if tabs.is_empty() {
            return Ok(None);
        }

        let active_tab_index = self
            .conn
            .query_row(
                "SELECT value FROM session_meta WHERE key = ?1",
                [ACTIVE_TAB_KEY],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
            .min(tabs.len() - 1);

        Ok(Some(StoredSession {
            tabs,
            active_tab_index,
        }))
    }

    /// Clear the saved session.
    pub fn clear_session(&self) -> MochaResult<()> {
        self.conn
            .execute("DELETE FROM session_tabs", [])
            .map_err(storage_err)?;
        self.conn
            .execute("DELETE FROM session_meta WHERE key = ?1", [ACTIVE_TAB_KEY])
            .map_err(storage_err)?;
        Ok(())
    }
}

fn encode_history(history: &[String]) -> Option<String> {
    if history.is_empty() {
        None
    } else {
        Some(history.join("\n"))
    }
}

fn decode_history(encoded: Option<&str>) -> Vec<String> {
    match encoded {
        None | Some("") => Vec::new(),
        Some(s) => s.split('\n').map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use crate::Profile;

    fn sample() -> StoredSession {
        StoredSession {
            tabs: vec![
                StoredTab {
                    url: Some("http://a.com/".to_string()),
                    title: "a.com".to_string(),
                    scroll_y: 12.5,
                    history: vec!["http://a.com/".to_string(), "http://a.com/2".to_string()],
                    current_history_index: Some(1),
                },
                StoredTab {
                    url: None,
                    title: "New Tab".to_string(),
                    scroll_y: 0.0,
                    history: Vec::new(),
                    current_history_index: None,
                },
            ],
            active_tab_index: 1,
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let p = Profile::private().unwrap();
        let s = p.session();
        s.save_session(&sample()).unwrap();
        let loaded = s.load_session().unwrap().unwrap();
        assert_eq!(loaded, sample());
    }

    #[test]
    fn load_with_no_session_is_none() {
        let p = Profile::private().unwrap();
        assert_eq!(p.session().load_session().unwrap(), None);
    }

    #[test]
    fn save_replaces_previous_session() {
        let p = Profile::private().unwrap();
        let s = p.session();
        s.save_session(&sample()).unwrap();
        let smaller = StoredSession {
            tabs: vec![StoredTab {
                url: Some("http://only.com/".to_string()),
                title: "only".to_string(),
                scroll_y: 0.0,
                history: vec!["http://only.com/".to_string()],
                current_history_index: Some(0),
            }],
            active_tab_index: 0,
        };
        s.save_session(&smaller).unwrap();
        let loaded = s.load_session().unwrap().unwrap();
        assert_eq!(loaded.tabs.len(), 1);
        assert_eq!(loaded.active_tab_index, 0);
    }

    #[test]
    fn clear_session_removes_it() {
        let p = Profile::private().unwrap();
        let s = p.session();
        s.save_session(&sample()).unwrap();
        s.clear_session().unwrap();
        assert_eq!(s.load_session().unwrap(), None);
    }

    #[test]
    fn session_persists_across_reopen() {
        let dir = TempDir::new();
        {
            let p = Profile::persistent(dir.path()).unwrap();
            p.session().save_session(&sample()).unwrap();
        }
        let p = Profile::persistent(dir.path()).unwrap();
        let loaded = p.session().load_session().unwrap().unwrap();
        assert_eq!(loaded.tabs.len(), 2);
        assert_eq!(loaded.active_tab_index, 1);
        assert_eq!(loaded.tabs[0].scroll_y, 12.5);
        assert_eq!(loaded.tabs[0].history.len(), 2);
    }

    #[test]
    fn private_session_not_persisted() {
        let p1 = Profile::private().unwrap();
        p1.session().save_session(&sample()).unwrap();
        let p2 = Profile::private().unwrap();
        assert_eq!(p2.session().load_session().unwrap(), None);
    }
}
