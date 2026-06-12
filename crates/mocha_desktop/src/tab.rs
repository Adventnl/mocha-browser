//! Tabs and the tab manager (Milestone 13).
//!
//! Each [`BrowserTab`] owns an independent page ([`DesktopPageState`]), its own
//! navigation history, scroll, and focus (the latter two live inside the page).
//! The [`TabManager`] owns the tab list and the active-tab invariant: there is
//! always at least one tab and the active id always names an existing tab.
//!
//! Loading is synchronous (blocking), so `is_loading` is only ever transiently
//! true; it is kept for the chrome/UI. Restored tabs (see [`crate::session`])
//! start *unloaded*: their page is a placeholder and the real document is
//! (re)loaded lazily the first time the tab becomes active.

use mocha_error::{MochaError, MochaResult};
use mocha_url::Url;

use crate::new_tab::InternalPage;
use crate::DesktopPageState;

/// A stable, unique identifier for a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

/// One row in a native list view (history, bookmarks, downloads). A row with a
/// `url` is clickable and navigates there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    pub title: String,
    pub detail: String,
    pub url: Option<String>,
    /// Visually accented (bookmarked entry, completed download, …).
    pub accent: bool,
    /// A per-row action id the shell understands (e.g. `remove:<id>`), or empty.
    pub action: String,
}

impl ListRow {
    pub fn new(title: impl Into<String>, detail: impl Into<String>) -> ListRow {
        ListRow {
            title: title.into(),
            detail: detail.into(),
            url: None,
            accent: false,
            action: String::new(),
        }
    }
    pub fn with_url(mut self, url: impl Into<String>) -> ListRow {
        self.url = Some(url.into());
        self
    }
    pub fn with_accent(mut self, accent: bool) -> ListRow {
        self.accent = accent;
        self
    }
    pub fn with_action(mut self, action: impl Into<String>) -> ListRow {
        self.action = action.into();
        self
    }
}

/// The control kind for a settings row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingKind {
    /// A boolean toggle (current state).
    Toggle(bool),
    /// A free-text value.
    Text(String),
    /// A clickable action button (e.g. "Clear history").
    Action,
}

/// One row on the native settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    /// Stable key the shell maps to a setting/action.
    pub key: String,
    pub label: String,
    pub kind: SettingKind,
}

/// A native (non-web) view shown in a tab's viewport instead of rendered page
/// content. These are drawn directly by the desktop shell (`crate::views`) —
/// no HTML, no network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InternalView {
    /// The new-tab / home start page.
    NewTab,
    /// A document failed to load or render; shows the attempted input and the
    /// real error message.
    LoadError {
        /// What the user tried to open (path or URL).
        input: String,
        /// The failure, exactly as reported by the engine.
        message: String,
    },
    /// Browsing history (newest first).
    History(Vec<ListRow>),
    /// Saved bookmarks.
    Bookmarks(Vec<ListRow>),
    /// Downloads.
    Downloads(Vec<ListRow>),
    /// Settings/preferences.
    Settings(Vec<SettingRow>),
}

impl InternalView {
    /// The tab title shown for this view.
    pub fn title(&self) -> &'static str {
        match self {
            InternalView::NewTab => "New Tab",
            InternalView::LoadError { .. } => crate::new_tab::LOAD_ERROR_TITLE,
            InternalView::History(_) => "History",
            InternalView::Bookmarks(_) => "Bookmarks",
            InternalView::Downloads(_) => "Downloads",
            InternalView::Settings(_) => "Settings",
        }
    }
}

/// A single browser tab: an independent page plus its own history/title/url.
pub struct BrowserTab {
    /// Stable unique id.
    pub id: TabId,
    page: DesktopPageState,
    title: String,
    url: Option<Url>,
    is_loading: bool,
    /// This tab's navigation history (back/forward stack of visited URLs).
    history: Vec<Url>,
    /// Index of the current entry in `history`, or `None` for an internal page.
    history_index: Option<usize>,
    /// Restored tabs start unloaded: the real document is fetched lazily on the
    /// first activation. `false` once the page has been (re)loaded.
    needs_reload: bool,
    /// Scroll offset to reapply after a lazy (re)load (session restore).
    pending_scroll: Option<f32>,
    /// When set, the viewport shows this native view instead of the page.
    internal_view: Option<InternalView>,
    /// Vertical scroll offset for scrollable native views (history, etc.).
    view_scroll: f32,
}

impl BrowserTab {
    /// A fresh tab showing the native new-tab (home) view. No network.
    fn new_tab_page(id: TabId, width: u32, height: u32) -> MochaResult<Self> {
        let page = DesktopPageState::from_html(InternalPage::NewTab.html(), width, height)?;
        Ok(BrowserTab {
            id,
            page,
            title: InternalPage::NewTab.title().to_string(),
            url: None,
            is_loading: false,
            history: Vec::new(),
            history_index: None,
            needs_reload: false,
            pending_scroll: None,
            internal_view: Some(InternalView::NewTab),
            view_scroll: 0.0,
        })
    }

    /// A tab showing the native load-error view for a failed `input` load
    /// (no URL, no history — like a new-tab page with explanatory content).
    fn load_error_page(
        id: TabId,
        input: &str,
        message: &str,
        width: u32,
        height: u32,
    ) -> MochaResult<Self> {
        let page = DesktopPageState::from_html(InternalPage::NewTab.html(), width, height)?;
        Ok(BrowserTab {
            id,
            page,
            title: crate::new_tab::LOAD_ERROR_TITLE.to_string(),
            url: None,
            is_loading: false,
            history: Vec::new(),
            history_index: None,
            needs_reload: false,
            pending_scroll: None,
            internal_view: Some(InternalView::LoadError {
                input: input.to_string(),
                message: message.to_string(),
            }),
            view_scroll: 0.0,
        })
    }

    /// A tab that immediately loads `input` (file path / `file://` / `http://`).
    fn loaded(id: TabId, input: &str, width: u32, height: u32) -> MochaResult<Self> {
        let page = DesktopPageState::load(input, width, height)?;
        let url = page.base_url().cloned();
        let (history, history_index) = match url.clone() {
            Some(u) => (vec![u], Some(0)),
            None => (Vec::new(), None),
        };
        let title = url
            .as_ref()
            .map(tab_title)
            .unwrap_or_else(|| InternalPage::NewTab.title().to_string());
        Ok(BrowserTab {
            id,
            page,
            title,
            url,
            is_loading: false,
            history,
            history_index,
            needs_reload: false,
            pending_scroll: None,
            internal_view: None,
            view_scroll: 0.0,
        })
    }

    // --- read-only accessors -------------------------------------------------

    /// The native view shown instead of page content, if any.
    pub fn internal_view(&self) -> Option<&InternalView> {
        self.internal_view.as_ref()
    }

    /// The scroll offset of the active native list view.
    pub fn view_scroll(&self) -> f32 {
        self.view_scroll
    }

    /// Scroll the native view by `delta`, clamped to `[0, max]`.
    pub fn scroll_view_by(&mut self, delta: f32, max: f32) {
        self.view_scroll = (self.view_scroll + delta).clamp(0.0, max.max(0.0));
    }

    /// The tab's title (derived from its URL, or "New Tab").
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The tab's current URL, or `None` for an internal page.
    pub fn url(&self) -> Option<&Url> {
        self.url.as_ref()
    }

    /// Whether the tab is mid-load (always transient: loads are synchronous).
    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    /// The rendered page (display list, images, scroll, focus).
    pub fn page(&self) -> &DesktopPageState {
        &self.page
    }

    /// Mutable access to the page (for input routing).
    pub fn page_mut(&mut self) -> &mut DesktopPageState {
        &mut self.page
    }

    /// Whether there is an earlier entry to go back to.
    pub fn can_go_back(&self) -> bool {
        self.history_index.is_some_and(|i| i > 0)
    }

    /// Whether there is a later entry to go forward to.
    pub fn can_go_forward(&self) -> bool {
        self.history_index
            .is_some_and(|i| i + 1 < self.history.len())
    }

    /// This tab's history as normalized URL strings (for session snapshots).
    pub(crate) fn history_strings(&self) -> Vec<String> {
        self.history.iter().map(|u| u.normalized()).collect()
    }

    /// The current history index (for session snapshots).
    pub(crate) fn history_index(&self) -> Option<usize> {
        self.history_index
    }

    // --- navigation ----------------------------------------------------------

    /// Navigate to `url`, truncating any forward history and pushing the new
    /// entry. Re-renders the page.
    fn navigate(&mut self, url: Url) -> MochaResult<()> {
        self.page.navigate(&url)?;
        let final_url = self.page.base_url().cloned().unwrap_or(url);
        match self.history_index {
            Some(i) => self.history.truncate(i + 1),
            None => self.history.clear(),
        }
        self.history.push(final_url.clone());
        self.history_index = Some(self.history.len() - 1);
        self.set_current(final_url);
        self.needs_reload = false;
        Ok(())
    }

    /// Move back one entry and re-render. Returns whether anything happened.
    fn go_back(&mut self) -> MochaResult<bool> {
        let Some(i) = self.history_index else {
            return Ok(false);
        };
        if i == 0 {
            return Ok(false);
        }
        let url = self.history[i - 1].clone();
        self.page.navigate(&url)?;
        let final_url = self.page.base_url().cloned().unwrap_or(url);
        self.history_index = Some(i - 1);
        if let Some(entry) = self.history.get_mut(i - 1) {
            *entry = final_url.clone();
        }
        self.set_current(final_url);
        Ok(true)
    }

    /// Move forward one entry and re-render. Returns whether anything happened.
    fn go_forward(&mut self) -> MochaResult<bool> {
        let Some(i) = self.history_index else {
            return Ok(false);
        };
        if i + 1 >= self.history.len() {
            return Ok(false);
        }
        let url = self.history[i + 1].clone();
        self.page.navigate(&url)?;
        let final_url = self.page.base_url().cloned().unwrap_or(url);
        self.history_index = Some(i + 1);
        if let Some(entry) = self.history.get_mut(i + 1) {
            *entry = final_url.clone();
        }
        self.set_current(final_url);
        Ok(true)
    }

    /// Reload the current entry without changing history.
    fn reload(&mut self) -> MochaResult<()> {
        if let Some(url) = self.current_url() {
            self.page.navigate(&url)?;
            if let Some(final_url) = self.page.base_url().cloned() {
                if let Some(i) = self.history_index {
                    if let Some(entry) = self.history.get_mut(i) {
                        *entry = final_url.clone();
                    }
                }
                self.set_current(final_url);
            }
        }
        self.needs_reload = false;
        if let Some(scroll) = self.pending_scroll.take() {
            self.page.set_scroll(scroll);
        }
        Ok(())
    }

    /// If this tab was restored unloaded, load it now (current entry), applying
    /// any pending scroll. A no-op for already-loaded or internal-page tabs.
    fn ensure_loaded(&mut self) -> MochaResult<()> {
        if self.needs_reload {
            if self.current_url().is_some() {
                self.reload()?;
            } else {
                self.needs_reload = false;
            }
        }
        Ok(())
    }

    fn current_url(&self) -> Option<Url> {
        self.history_index
            .and_then(|i| self.history.get(i).cloned())
    }

    fn set_current(&mut self, url: Url) {
        self.title = tab_title(&url);
        self.url = Some(url);
        // A successfully rendered document replaces any native view.
        self.internal_view = None;
        self.view_scroll = 0.0;
    }

    /// Replace this tab's content with the native load-error view (the page,
    /// URL, and title reset; navigation history is left untouched so Back
    /// still works after a failed navigation).
    pub(crate) fn show_load_error(
        &mut self,
        input: &str,
        message: &str,
        width: u32,
        height: u32,
    ) -> MochaResult<()> {
        self.page = DesktopPageState::from_html(InternalPage::NewTab.html(), width, height)?;
        self.title = crate::new_tab::LOAD_ERROR_TITLE.to_string();
        self.url = None;
        self.internal_view = Some(InternalView::LoadError {
            input: input.to_string(),
            message: message.to_string(),
        });
        self.view_scroll = 0.0;
        Ok(())
    }

    /// Replace this tab's content with a native list/settings view (history,
    /// bookmarks, downloads, settings). Clears URL/history like other internal
    /// pages; the title comes from the view.
    pub(crate) fn show_native(
        &mut self,
        view: InternalView,
        width: u32,
        height: u32,
    ) -> MochaResult<()> {
        self.page = DesktopPageState::from_html(InternalPage::NewTab.html(), width, height)?;
        self.title = view.title().to_string();
        self.url = None;
        self.history.clear();
        self.history_index = None;
        self.needs_reload = false;
        self.internal_view = Some(view);
        self.view_scroll = 0.0;
        Ok(())
    }

    /// Reset this tab to the native new-tab (home) view, clearing its history
    /// (Mocha's internal pages have no history entries).
    pub(crate) fn show_new_tab_page(&mut self, width: u32, height: u32) -> MochaResult<()> {
        self.page = DesktopPageState::from_html(InternalPage::NewTab.html(), width, height)?;
        self.title = InternalPage::NewTab.title().to_string();
        self.url = None;
        self.history.clear();
        self.history_index = None;
        self.needs_reload = false;
        self.internal_view = Some(InternalView::NewTab);
        self.view_scroll = 0.0;
        Ok(())
    }
}

/// Derive a short tab title from a URL: the last path segment, else the host,
/// else the normalized URL.
fn tab_title(url: &Url) -> String {
    if let Some(name) = url.path.rsplit(['/', '\\']).find(|s| !s.is_empty()) {
        return name.to_string();
    }
    url.host.clone().unwrap_or_else(|| url.normalized())
}

/// Owns the tab list and the active-tab invariant.
pub struct TabManager {
    tabs: Vec<BrowserTab>,
    active_tab: TabId,
    next_tab_id: u64,
    viewport_width: u32,
    viewport_height: u32,
}

impl TabManager {
    /// A new manager with a single blank new-tab page.
    pub fn new(viewport_width: u32, viewport_height: u32) -> MochaResult<Self> {
        let mut manager = Self::empty(viewport_width, viewport_height);
        let tab = BrowserTab::new_tab_page(
            manager.alloc_id(),
            manager.viewport_width,
            manager.viewport_height,
        )?;
        manager.active_tab = tab.id;
        manager.tabs.push(tab);
        Ok(manager)
    }

    /// A new manager whose single tab immediately loads `input`.
    pub fn with_loaded(
        input: &str,
        viewport_width: u32,
        viewport_height: u32,
    ) -> MochaResult<Self> {
        let mut manager = Self::empty(viewport_width, viewport_height);
        let tab = BrowserTab::loaded(
            manager.alloc_id(),
            input,
            manager.viewport_width,
            manager.viewport_height,
        )?;
        manager.active_tab = tab.id;
        manager.tabs.push(tab);
        Ok(manager)
    }

    /// A new manager whose single tab shows the internal load-error page for a
    /// failed `input` load (the browser opens instead of exiting).
    pub fn with_load_error(
        input: &str,
        message: &str,
        viewport_width: u32,
        viewport_height: u32,
    ) -> MochaResult<Self> {
        let mut manager = Self::empty(viewport_width, viewport_height);
        let tab = BrowserTab::load_error_page(
            manager.alloc_id(),
            input,
            message,
            manager.viewport_width,
            manager.viewport_height,
        )?;
        manager.active_tab = tab.id;
        manager.tabs.push(tab);
        Ok(manager)
    }

    pub(crate) fn empty(viewport_width: u32, viewport_height: u32) -> Self {
        TabManager {
            tabs: Vec::new(),
            active_tab: TabId(0),
            next_tab_id: 0,
            viewport_width: viewport_width.max(1),
            viewport_height: viewport_height.max(1),
        }
    }

    pub(crate) fn alloc_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    pub(crate) fn push_tab(&mut self, tab: BrowserTab) {
        self.tabs.push(tab);
    }

    pub(crate) fn set_active(&mut self, id: TabId) {
        self.active_tab = id;
    }

    /// Build a restored, unloaded tab from session metadata.
    pub(crate) fn build_restored_tab(
        &mut self,
        url: Option<Url>,
        title: String,
        scroll_y: f32,
        mut history: Vec<Url>,
        mut history_index: Option<usize>,
    ) -> MochaResult<BrowserTab> {
        let id = self.alloc_id();
        let page = DesktopPageState::from_html(
            InternalPage::NewTab.html(),
            self.viewport_width,
            self.viewport_height,
        )?;
        // If there is a URL but no history, seed a one-entry history so the lazy
        // reload has a current entry to load.
        if history.is_empty() {
            if let Some(u) = url.clone() {
                history.push(u);
                history_index = Some(0);
            }
        }
        let history_index = history_index.filter(|i| *i < history.len());
        let needs_reload = url.is_some();
        Ok(BrowserTab {
            id,
            page,
            title,
            url,
            is_loading: false,
            history,
            history_index,
            needs_reload,
            pending_scroll: Some(scroll_y),
            // Until the lazy load happens the tab shows the home view; the
            // first successful (re)load clears it via `set_current`.
            internal_view: Some(InternalView::NewTab),
            view_scroll: 0.0,
        })
    }

    /// The active tab's id.
    pub fn active_id(&self) -> TabId {
        self.active_tab
    }

    /// The index of the active tab (always valid).
    pub fn active_index(&self) -> usize {
        self.index_of(self.active_tab)
            .expect("active tab id always names an existing tab")
    }

    /// The active tab.
    pub fn active(&self) -> &BrowserTab {
        &self.tabs[self.active_index()]
    }

    /// The active tab (mutable).
    pub fn active_mut(&mut self) -> &mut BrowserTab {
        let index = self.active_index();
        &mut self.tabs[index]
    }

    /// All tabs in left-to-right order.
    pub fn tabs(&self) -> &[BrowserTab] {
        &self.tabs
    }

    /// All tab ids in order (for chrome hit testing).
    pub fn tab_ids(&self) -> Vec<TabId> {
        self.tabs.iter().map(|t| t.id).collect()
    }

    /// The number of open tabs (always >= 1).
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Always `false`: a manager always has at least one tab.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Look up a tab by id.
    pub fn tab(&self, id: TabId) -> Option<&BrowserTab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Open a new blank tab and make it active. Returns the new id.
    pub fn new_tab(&mut self) -> MochaResult<TabId> {
        let id = self.alloc_id();
        let tab = BrowserTab::new_tab_page(id, self.viewport_width, self.viewport_height)?;
        self.tabs.push(tab);
        self.active_tab = id;
        Ok(id)
    }

    /// Open `input` in a new tab and make it active. Returns the new id.
    pub fn open_in_new_tab(&mut self, input: &str) -> MochaResult<TabId> {
        let id = self.alloc_id();
        let tab = BrowserTab::loaded(id, input, self.viewport_width, self.viewport_height)?;
        self.tabs.push(tab);
        self.active_tab = id;
        Ok(id)
    }

    /// Make `id` the active tab (loading it lazily if it was restored unloaded).
    pub fn switch_tab(&mut self, id: TabId) -> MochaResult<()> {
        let index = self
            .index_of(id)
            .ok_or_else(|| MochaError::Navigation(format!("no tab with id {}", id.0)))?;
        self.active_tab = id;
        self.tabs[index].ensure_loaded()?;
        Ok(())
    }

    /// Close `id`. Closing the active tab activates its right neighbour, else its
    /// left neighbour; closing the last tab opens a fresh blank tab.
    pub fn close_tab(&mut self, id: TabId) -> MochaResult<()> {
        let index = self
            .index_of(id)
            .ok_or_else(|| MochaError::Navigation(format!("no tab with id {}", id.0)))?;
        let was_active = self.active_tab == id;
        self.tabs.remove(index);

        if self.tabs.is_empty() {
            let tab = BrowserTab::new_tab_page(
                self.alloc_id(),
                self.viewport_width,
                self.viewport_height,
            )?;
            self.active_tab = tab.id;
            self.tabs.push(tab);
            return Ok(());
        }

        if was_active {
            // The right neighbour shifted into `index`; otherwise take the left.
            let new_index = if index < self.tabs.len() {
                index
            } else {
                self.tabs.len() - 1
            };
            self.active_tab = self.tabs[new_index].id;
            self.tabs[new_index].ensure_loaded()?;
        }
        Ok(())
    }

    /// Resize every tab's page to a new viewport.
    pub fn resize(&mut self, width: u32, height: u32) -> MochaResult<()> {
        self.viewport_width = width.max(1);
        self.viewport_height = height.max(1);
        for tab in &mut self.tabs {
            tab.page.resize(self.viewport_width, self.viewport_height)?;
        }
        Ok(())
    }

    // --- active-tab navigation ----------------------------------------------

    /// Navigate the active tab to `url`.
    pub fn navigate_active(&mut self, url: Url) -> MochaResult<()> {
        self.active_mut().navigate(url)
    }

    /// Back in the active tab. Returns whether anything happened.
    pub fn back_active(&mut self) -> MochaResult<bool> {
        self.active_mut().go_back()
    }

    /// Forward in the active tab. Returns whether anything happened.
    pub fn forward_active(&mut self) -> MochaResult<bool> {
        self.active_mut().go_forward()
    }

    /// Reload the active tab.
    pub fn reload_active(&mut self) -> MochaResult<()> {
        self.active_mut().reload()
    }

    /// Show the native load-error view in the active tab (after a failed
    /// navigation); existing history is kept so Back still works.
    pub fn show_error_on_active(&mut self, input: &str, message: &str) -> MochaResult<()> {
        let (width, height) = (self.viewport_width, self.viewport_height);
        self.active_mut()
            .show_load_error(input, message, width, height)
    }

    /// Reset the active tab to the new-tab (home) view.
    pub fn home_active(&mut self) -> MochaResult<()> {
        let (width, height) = (self.viewport_width, self.viewport_height);
        self.active_mut().show_new_tab_page(width, height)
    }

    /// Show a native list/settings view in the active tab.
    pub fn show_native_on_active(&mut self, view: InternalView) -> MochaResult<()> {
        let (width, height) = (self.viewport_width, self.viewport_height);
        self.active_mut().show_native(view, width, height)
    }

    /// Open a native view in a new active tab.
    pub fn open_native_tab(&mut self, view: InternalView) -> MochaResult<TabId> {
        let id = self.new_tab()?;
        self.show_native_on_active(view)?;
        Ok(id)
    }

    /// Move the tab at `from` to index `to` (clamped), preserving the active
    /// tab. Used by drag-to-reorder. Returns whether the order changed.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        let len = self.tabs.len();
        if from >= len {
            return false;
        }
        let to = to.min(len - 1);
        if from == to {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        true
    }

    /// The strip index of `id`, if present (for drag-to-reorder hit testing).
    pub fn index_of_id(&self, id: TabId) -> Option<usize> {
        self.index_of(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> TabManager {
        TabManager::new(800, 600).unwrap()
    }

    #[test]
    fn starts_with_one_active_tab() {
        let m = manager();
        assert_eq!(m.len(), 1);
        assert_eq!(m.active().title(), "New Tab");
        assert!(m.active().url().is_none());
    }

    #[test]
    fn with_load_error_shows_the_error_page_tab() {
        let m =
            TabManager::with_load_error("definitely/missing.html", "io error: not found", 800, 600)
                .unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m.active().title(), "Problem loading page");
        assert!(m.active().url().is_none());
        assert!(!m.active().can_go_back());
        assert!(!m.active().can_go_forward());
        // The tab shows the native error view with the attempted input + message.
        match m.active().internal_view() {
            Some(InternalView::LoadError { input, message }) => {
                assert_eq!(input, "definitely/missing.html");
                assert_eq!(message, "io error: not found");
            }
            other => panic!("expected a load-error view, got {other:?}"),
        }
    }

    #[test]
    fn new_tab_adds_and_activates() {
        let mut m = manager();
        let first = m.active_id();
        let id = m.new_tab().unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m.active_id(), id);
        assert_ne!(id, first);
    }

    #[test]
    fn switch_tab_changes_active() {
        let mut m = manager();
        let first = m.active_id();
        let second = m.new_tab().unwrap();
        m.switch_tab(first).unwrap();
        assert_eq!(m.active_id(), first);
        m.switch_tab(second).unwrap();
        assert_eq!(m.active_id(), second);
    }

    #[test]
    fn switch_tab_invalid_id_errors() {
        let mut m = manager();
        let err = m.switch_tab(TabId(999)).unwrap_err();
        assert!(matches!(err, MochaError::Navigation(_)));
    }

    #[test]
    fn close_inactive_keeps_active() {
        let mut m = manager();
        let first = m.active_id();
        let second = m.new_tab().unwrap();
        assert_eq!(m.active_id(), second);
        m.close_tab(first).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m.active_id(), second);
    }

    #[test]
    fn close_active_selects_right_neighbour() {
        let mut m = manager();
        let a = m.active_id();
        let b = m.new_tab().unwrap();
        let c = m.new_tab().unwrap();
        // Order: a, b, c. Activate b, close it -> right neighbour c.
        m.switch_tab(b).unwrap();
        m.close_tab(b).unwrap();
        assert_eq!(m.active_id(), c);
        // Now order: a, c. Activate c (rightmost), close -> left neighbour a.
        m.switch_tab(c).unwrap();
        m.close_tab(c).unwrap();
        assert_eq!(m.active_id(), a);
    }

    #[test]
    fn close_last_creates_blank_tab() {
        let mut m = manager();
        let only = m.active_id();
        m.close_tab(only).unwrap();
        assert_eq!(m.len(), 1);
        assert_ne!(m.active_id(), only, "a fresh tab with a new id replaces it");
        assert_eq!(m.active().title(), "New Tab");
    }

    #[test]
    fn tab_ids_are_unique_and_stable() {
        let mut m = manager();
        let a = m.active_id();
        let b = m.new_tab().unwrap();
        let c = m.new_tab().unwrap();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        // Closing a tab does not renumber the survivors.
        m.close_tab(b).unwrap();
        let ids = m.tab_ids();
        assert_eq!(ids, vec![a, c]);
    }

    #[test]
    fn order_is_preserved() {
        let mut m = manager();
        let a = m.active_id();
        let b = m.new_tab().unwrap();
        let c = m.new_tab().unwrap();
        assert_eq!(m.tab_ids(), vec![a, b, c]);
    }

    #[test]
    fn active_tab_always_valid_after_many_ops() {
        let mut m = manager();
        for _ in 0..5 {
            m.new_tab().unwrap();
        }
        // Close several; the active id must always resolve.
        let ids = m.tab_ids();
        m.close_tab(ids[2]).unwrap();
        assert!(m.tab(m.active_id()).is_some());
        m.close_tab(m.active_id()).unwrap();
        assert!(m.tab(m.active_id()).is_some());
    }

    #[test]
    fn loaded_local_tab_uses_rendered_final_url() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("crate is under crates/mocha_desktop")
            .join("examples/basic/index.html")
            .to_string_lossy()
            .into_owned();

        let manager = TabManager::with_loaded(&path, 800, 600).unwrap();
        let tab = manager.active();
        assert_eq!(tab.url(), tab.page().base_url());
        assert_eq!(
            tab.history_strings(),
            vec![tab.page().base_url().unwrap().normalized()]
        );
    }
}
