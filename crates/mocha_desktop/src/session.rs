//! In-memory session snapshot and restore (Milestone 13).
//!
//! A [`SessionSnapshot`] captures only lightweight metadata for each tab — URL,
//! title, scroll, and history — never the DOM, form state, layout tree, or
//! display list. This makes it cheap to copy and (in Milestone 14) to persist.
//!
//! **Restore policy.** Restored tabs are recreated as *unloaded metadata tabs*
//! backed by the internal new-tab placeholder page. The active tab is reloaded
//! eagerly; inactive tabs are reloaded lazily the first time they are activated
//! (see [`crate::tab::TabManager::switch_tab`]). Tabs whose URL is `None` stay on
//! the new-tab page.

use mocha_error::MochaResult;
use mocha_url::Url;

use crate::tab::TabManager;

/// A lightweight, serializable-shaped snapshot of all tabs and the active tab.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshot {
    /// One entry per tab, in tab-strip order.
    pub tabs: Vec<SessionTab>,
    /// Index into `tabs` of the active tab.
    pub active_tab_index: usize,
}

/// Per-tab session metadata. No DOM/layout/form state is captured.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionTab {
    /// The tab's current URL (normalized), or `None` for the new-tab page.
    pub url: Option<String>,
    /// The tab title.
    pub title: String,
    /// The vertical scroll offset in px.
    pub scroll_y: f32,
    /// The back/forward history as normalized URL strings.
    pub history: Vec<String>,
    /// Index of the current entry in `history`.
    pub current_history_index: Option<usize>,
}

impl TabManager {
    /// Capture a snapshot of the current tabs (metadata only).
    pub fn snapshot(&self) -> SessionSnapshot {
        let tabs = self
            .tabs()
            .iter()
            .map(|tab| SessionTab {
                url: tab.url().map(|u| u.normalized()),
                title: tab.title().to_string(),
                scroll_y: tab.page().scroll_y(),
                history: tab.history_strings(),
                current_history_index: tab.history_index(),
            })
            .collect();
        SessionSnapshot {
            tabs,
            active_tab_index: self.active_index(),
        }
    }

    /// Rebuild a manager from a snapshot. Recreates metadata tabs (no heavy page
    /// serialization); the active tab is loaded eagerly, inactive tabs lazily.
    pub fn restore(
        snapshot: &SessionSnapshot,
        viewport_width: u32,
        viewport_height: u32,
    ) -> MochaResult<Self> {
        // An empty snapshot degrades to a single fresh tab.
        if snapshot.tabs.is_empty() {
            return TabManager::new(viewport_width, viewport_height);
        }

        let mut manager = TabManager::empty(viewport_width, viewport_height);
        for session_tab in &snapshot.tabs {
            let url = session_tab.url.as_deref().and_then(|s| Url::parse(s).ok());
            let history = session_tab
                .history
                .iter()
                .filter_map(|s| Url::parse(s).ok())
                .collect::<Vec<_>>();
            let tab = manager.build_restored_tab(
                url,
                session_tab.title.clone(),
                session_tab.scroll_y,
                history,
                session_tab.current_history_index,
            )?;
            manager.push_tab(tab);
        }

        let active_index = snapshot.active_tab_index.min(manager.len() - 1);
        let active_id = manager.tabs()[active_index].id;
        manager.set_active(active_id);
        // Eagerly load the active tab; inactive tabs load on first activation.
        manager.switch_tab(active_id)?;
        Ok(manager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn example_path(name: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crate is under crates/mocha_desktop")
            .join("examples")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn loaded_manager() -> (TabManager, String) {
        let path = example_path("basic/index.html");
        (TabManager::with_loaded(&path, 800, 600).unwrap(), path)
    }

    #[test]
    fn snapshot_captures_tabs_active_and_metadata() {
        let (mut m, path) = loaded_manager();
        m.new_tab().unwrap(); // a second, blank tab (now active)
        let snap = m.snapshot();
        assert_eq!(snap.tabs.len(), 2);
        assert_eq!(snap.active_tab_index, 1);
        // First tab carries the loaded URL + history; second is the new-tab page.
        let expected_url = Url::parse(&path).unwrap().normalized();
        assert_eq!(snap.tabs[0].url.as_deref(), Some(expected_url.as_str()));
        assert_eq!(snap.tabs[0].history.len(), 1);
        assert_eq!(snap.tabs[0].current_history_index, Some(0));
        assert_eq!(snap.tabs[1].url, None);
        assert_eq!(snap.tabs[1].title, "New Tab");
    }

    #[test]
    fn snapshot_captures_scroll() {
        let html = r#"<html><body><div style="height: 2000px;">x</div></body></html>"#;
        let mut m = TabManager::new(400, 300).unwrap();
        // Replace the new-tab page with a tall in-memory page, then scroll it.
        *m.active_mut().page_mut() = crate::DesktopPageState::from_html(html, 400, 300).unwrap();
        m.active_mut().page_mut().scroll_by(150.0);
        let snap = m.snapshot();
        assert!(snap.tabs[0].scroll_y > 0.0);
    }

    #[test]
    fn restore_recreates_tabs_order_and_active() {
        let (mut m, _path) = loaded_manager();
        let b = m.new_tab().unwrap();
        m.switch_tab(b).unwrap();
        let snap = m.snapshot();

        let restored = TabManager::restore(&snap, 800, 600).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.active_index(), 1);
        // Order preserved: first tab kept its URL, second is the new-tab page.
        assert!(restored.tabs()[0].url().is_some());
        assert!(restored.tabs()[1].url().is_none());
    }

    #[test]
    fn restore_loads_active_tab_eagerly() {
        let (m, _path) = loaded_manager();
        let snap = m.snapshot();
        let restored = TabManager::restore(&snap, 800, 600).unwrap();
        // The active (only) tab was loaded: its page has a non-trivial display list.
        assert!(!restored.active().page().display_list().is_empty());
        assert!(restored.active().url().is_some());
    }

    #[test]
    fn restore_preserves_history_metadata() {
        // Two-entry history in a single tab.
        let first = example_path("basic/index.html");
        let second = example_path("styled/index.html");
        let mut m = TabManager::with_loaded(&first, 800, 600).unwrap();
        m.navigate_active(Url::parse(&second).unwrap()).unwrap();
        assert!(m.active().can_go_back());
        let snap = m.snapshot();
        assert_eq!(snap.tabs[0].history.len(), 2);
        assert_eq!(snap.tabs[0].current_history_index, Some(1));

        let restored = TabManager::restore(&snap, 800, 600).unwrap();
        assert!(restored.active().can_go_back());
        assert!(!restored.active().can_go_forward());
    }

    #[test]
    fn restore_empty_snapshot_degrades_to_one_tab() {
        let snap = SessionSnapshot {
            tabs: Vec::new(),
            active_tab_index: 0,
        };
        let restored = TabManager::restore(&snap, 800, 600).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.active().title(), "New Tab");
    }
}
