//! Desktop integration with the persistent [`mocha_storage::Profile`]: history,
//! bookmarks, downloads, settings, and the last session, plus address-bar
//! suggestions drawn from history + bookmarks.
//!
//! Storage is best-effort by design: the browser must keep running if a write
//! fails (read-only disk, locked db), so every method swallows errors into a
//! sensible default and logs nothing here (the caller may). A profile may be
//! absent entirely (private window / no writable location): all reads return
//! empty, all writes are no-ops.

use std::time::{SystemTime, UNIX_EPOCH};

use mocha_storage::bookmarks::BookmarkEntry;
use mocha_storage::downloads::{DownloadEntry, DownloadStatus};
use mocha_storage::history::HistoryEntry;
use mocha_storage::{Profile, StoredSession};
use mocha_url::Url;

/// Settings keys persisted in the profile.
pub const SETTING_HOME_URL: &str = "home_url";
pub const SETTING_SHOW_BOOKMARKS_BAR: &str = "show_bookmarks_bar";
pub const SETTING_RESTORE_SESSION: &str = "restore_session";

/// The default home/start target (empty => native new-tab page).
pub const DEFAULT_HOME_URL: &str = "";

/// One address-bar suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The URL to navigate to if chosen.
    pub url: String,
    /// The page title (or the URL when unknown).
    pub title: String,
    /// Whether this suggestion comes from a bookmark (vs. history).
    pub bookmarked: bool,
}

/// Current epoch milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A profile-backed store for the desktop shell. Holds an optional persistent
/// [`Profile`]; when `None` everything degrades to empty/no-op.
pub struct BrowserProfile {
    profile: Option<Profile>,
}

impl Default for BrowserProfile {
    fn default() -> BrowserProfile {
        BrowserProfile::none()
    }
}

impl BrowserProfile {
    /// A profile-less store (private window / unavailable storage).
    pub fn none() -> BrowserProfile {
        BrowserProfile { profile: None }
    }

    /// Wrap an open persistent profile.
    pub fn new(profile: Profile) -> BrowserProfile {
        BrowserProfile {
            profile: Some(profile),
        }
    }

    /// Whether a persistent profile is attached.
    pub fn is_persistent(&self) -> bool {
        self.profile.as_ref().is_some_and(|p| !p.is_private())
    }

    // --- history ------------------------------------------------------------

    /// Record a visit (best effort).
    pub fn record_visit(&self, url: &Url, title: Option<&str>) {
        if let Some(p) = &self.profile {
            let _ = p.history().record_visit(url, title, now_ms());
        }
    }

    /// The most recent `limit` visits, newest first.
    pub fn recent_history(&self, limit: usize) -> Vec<HistoryEntry> {
        self.profile
            .as_ref()
            .and_then(|p| p.history().recent_visits(limit).ok())
            .unwrap_or_default()
    }

    /// Wipe all history (best effort).
    pub fn clear_history(&self) {
        if let Some(p) = &self.profile {
            let _ = p.history().clear_history();
        }
    }

    // --- bookmarks ----------------------------------------------------------

    /// All bookmarks, newest first.
    pub fn bookmarks(&self) -> Vec<BookmarkEntry> {
        self.profile
            .as_ref()
            .and_then(|p| p.bookmarks().list_bookmarks().ok())
            .unwrap_or_default()
    }

    /// Whether `url` is bookmarked.
    pub fn is_bookmarked(&self, url: &Url) -> bool {
        let normalized = url.normalized();
        self.bookmarks().iter().any(|b| b.url == normalized)
    }

    /// Toggle a bookmark for `url`; returns the new bookmarked state.
    pub fn toggle_bookmark(&self, url: &Url, title: &str) -> bool {
        let Some(p) = &self.profile else {
            return false;
        };
        let normalized = url.normalized();
        let store = p.bookmarks();
        if let Ok(list) = store.list_bookmarks() {
            if let Some(existing) = list.iter().find(|b| b.url == normalized) {
                let _ = store.remove_bookmark(existing.id);
                return false;
            }
        }
        let _ = store.add_bookmark(url, title, now_ms());
        true
    }

    /// Remove a bookmark by id (best effort).
    pub fn remove_bookmark(&self, id: i64) {
        if let Some(p) = &self.profile {
            let _ = p.bookmarks().remove_bookmark(id);
        }
    }

    // --- downloads ----------------------------------------------------------

    /// All downloads, newest first.
    pub fn downloads(&self) -> Vec<DownloadEntry> {
        self.profile
            .as_ref()
            .and_then(|p| p.downloads().list_downloads().ok())
            .unwrap_or_default()
    }

    /// Record a started download, returning its id (or `None` with no profile).
    pub fn start_download(&self, url: &Url, target_path: &str) -> Option<i64> {
        self.profile.as_ref().and_then(|p| {
            p.downloads()
                .insert_download(url, target_path, now_ms())
                .ok()
        })
    }

    /// Mark a download finished (best effort).
    pub fn complete_download(&self, id: i64, ok: bool) {
        if let Some(p) = &self.profile {
            let store = p.downloads();
            let _ = if ok {
                store.mark_complete(id, now_ms())
            } else {
                store.mark_failed(id, now_ms())
            };
        }
    }

    /// Clear download history (best effort).
    pub fn clear_downloads(&self) {
        if let Some(p) = &self.profile {
            let _ = p.downloads().clear_downloads();
        }
    }

    // --- settings -----------------------------------------------------------

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.profile
            .as_ref()
            .and_then(|p| p.settings().get_string(key).ok().flatten())
    }

    pub fn set_string(&self, key: &str, value: &str) {
        if let Some(p) = &self.profile {
            let _ = p.settings().set_string(key, value);
        }
    }

    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.profile
            .as_ref()
            .and_then(|p| p.settings().get_bool(key).ok().flatten())
            .unwrap_or(default)
    }

    pub fn set_bool(&self, key: &str, value: bool) {
        if let Some(p) = &self.profile {
            let _ = p.settings().set_bool(key, value);
        }
    }

    /// The configured home target (empty string => native new-tab page).
    pub fn home_url(&self) -> String {
        self.get_string(SETTING_HOME_URL)
            .unwrap_or_else(|| DEFAULT_HOME_URL.to_string())
    }

    /// Whether the bookmarks bar is shown (default true).
    pub fn show_bookmarks_bar(&self) -> bool {
        self.get_bool(SETTING_SHOW_BOOKMARKS_BAR, true)
    }

    /// Whether to restore the previous session on launch (default true).
    pub fn restore_session(&self) -> bool {
        self.get_bool(SETTING_RESTORE_SESSION, true)
    }

    // --- session ------------------------------------------------------------

    /// Persist the current session (best effort).
    pub fn save_session(&self, session: &StoredSession) {
        if let Some(p) = &self.profile {
            let _ = p.session().save_session(session);
        }
    }

    /// Load the last session, if any.
    pub fn load_session(&self) -> Option<StoredSession> {
        self.profile
            .as_ref()
            .and_then(|p| p.session().load_session().ok().flatten())
    }

    // --- suggestions --------------------------------------------------------

    /// Up to `limit` address-bar suggestions for `query`, bookmarks first, then
    /// history, de-duplicated by URL. Matching is case-insensitive over URL and
    /// title substrings. An empty query yields the most recent history.
    pub fn suggestions(&self, query: &str, limit: usize) -> Vec<Suggestion> {
        let q = query.trim().to_ascii_lowercase();
        let mut out: Vec<Suggestion> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let matches = |hay: &str| q.is_empty() || hay.to_ascii_lowercase().contains(&q);

        for b in self.bookmarks() {
            if out.len() >= limit {
                break;
            }
            if (matches(&b.url) || matches(&b.title)) && !seen.contains(&b.url) {
                seen.push(b.url.clone());
                out.push(Suggestion {
                    url: b.url,
                    title: if b.title.is_empty() {
                        "Bookmark".into()
                    } else {
                        b.title
                    },
                    bookmarked: true,
                });
            }
        }
        for h in self.recent_history(limit * 4) {
            if out.len() >= limit {
                break;
            }
            let title = h.title.clone().unwrap_or_else(|| h.url.clone());
            if (matches(&h.url) || matches(&title)) && !seen.contains(&h.url) {
                seen.push(h.url.clone());
                out.push(Suggestion {
                    url: h.url,
                    title,
                    bookmarked: false,
                });
            }
        }
        out
    }
}

/// A human-readable "x ago" for an epoch-ms timestamp relative to now.
pub fn relative_time(then_ms: i64) -> String {
    let delta = (now_ms() - then_ms).max(0) / 1000;
    match delta {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", delta / 60),
        3600..=86399 => format!("{} hr ago", delta / 3600),
        _ => format!("{} days ago", delta / 86400),
    }
}

/// A short label for a download status.
pub fn download_status_label(status: DownloadStatus) -> &'static str {
    match status {
        DownloadStatus::InProgress => "In progress",
        DownloadStatus::Complete => "Complete",
        DownloadStatus::Failed => "Failed",
        DownloadStatus::Canceled => "Canceled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_storage::Profile;

    fn profile() -> BrowserProfile {
        BrowserProfile::new(Profile::private().unwrap())
    }

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn none_profile_is_all_empty_and_noop() {
        let p = BrowserProfile::none();
        assert!(!p.is_persistent());
        assert!(p.bookmarks().is_empty());
        assert!(p.recent_history(10).is_empty());
        assert!(p.suggestions("x", 5).is_empty());
        assert!(!p.toggle_bookmark(&url("https://a.com/"), "A"));
        p.record_visit(&url("https://a.com/"), Some("A")); // no panic
        assert!(p.load_session().is_none());
    }

    #[test]
    fn bookmark_toggle_round_trips() {
        let p = profile();
        let u = url("https://example.com/");
        assert!(!p.is_bookmarked(&u));
        assert!(p.toggle_bookmark(&u, "Example"));
        assert!(p.is_bookmarked(&u));
        assert_eq!(p.bookmarks().len(), 1);
        assert!(!p.toggle_bookmark(&u, "Example"));
        assert!(!p.is_bookmarked(&u));
        assert!(p.bookmarks().is_empty());
    }

    #[test]
    fn history_and_suggestions() {
        let p = profile();
        p.record_visit(&url("https://rust-lang.org/"), Some("Rust"));
        p.record_visit(&url("https://docs.rs/"), Some("Docs"));
        p.toggle_bookmark(&url("https://news.example/"), "News Example");
        assert!(p.recent_history(10).len() >= 2);
        // Query matches title or url; bookmarks come first.
        let s = p.suggestions("example", 5);
        assert!(s
            .iter()
            .any(|x| x.bookmarked && x.url.contains("news.example")));
        let rust = p.suggestions("rust", 5);
        assert!(rust.iter().any(|x| x.url.contains("rust-lang")));
        // Empty query returns recent history.
        assert!(!p.suggestions("", 5).is_empty());
    }

    #[test]
    fn settings_have_defaults_and_persist() {
        let p = profile();
        assert!(p.show_bookmarks_bar());
        assert!(p.restore_session());
        assert_eq!(p.home_url(), DEFAULT_HOME_URL);
        p.set_string(SETTING_HOME_URL, "https://example.com/");
        p.set_bool(SETTING_SHOW_BOOKMARKS_BAR, false);
        assert_eq!(p.home_url(), "https://example.com/");
        assert!(!p.show_bookmarks_bar());
    }

    #[test]
    fn relative_time_buckets() {
        let now = now_ms();
        assert_eq!(relative_time(now), "just now");
        assert!(relative_time(now - 120_000).contains("min"));
        assert!(relative_time(now - 7_200_000).contains("hr"));
        assert!(relative_time(now - 3 * 86_400_000).contains("days"));
    }
}
