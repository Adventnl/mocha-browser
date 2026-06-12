//! Browser application state: a tab strip + chrome + address bar over the
//! active tab's page.

use crate::address_bar::AddressBarState;
use crate::chrome::{ChromeElement, ChromeLayout};
use crate::profile::{
    BrowserProfile, Suggestion, SETTING_HOME_URL, SETTING_RESTORE_SESSION,
    SETTING_SHOW_BOOKMARKS_BAR,
};
use crate::tab::{InternalView, ListRow, SettingKind, SettingRow, TabId, TabManager};
use crate::views::{self, ViewHit};
use crate::{profile, SessionSnapshot};
use mocha_error::MochaResult;
use mocha_storage::StoredSession;
use mocha_url::Url;

use super::{DesktopAction, DesktopPageState};

/// Maximum address-bar suggestions shown at once.
const MAX_SUGGESTIONS: usize = 8;

/// The overflow-menu items, in order. The index matches `ChromeElement::MenuItem`.
pub const MENU_ITEMS: [(&str, MenuCommand); 7] = [
    ("New Tab", MenuCommand::NewTab),
    ("History", MenuCommand::History),
    ("Bookmarks", MenuCommand::Bookmarks),
    ("Downloads", MenuCommand::Downloads),
    ("Bookmark This Page", MenuCommand::BookmarkPage),
    ("Settings", MenuCommand::Settings),
    ("Toggle Bookmarks Bar", MenuCommand::ToggleBookmarksBar),
];

/// A command issued from the overflow menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    NewTab,
    History,
    Bookmarks,
    Downloads,
    BookmarkPage,
    Settings,
    ToggleBookmarksBar,
}

/// The current focus context in the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserFocus {
    /// Focus is in the address bar.
    AddressBar,
    /// Focus is on the page (or no focus).
    Page,
}

/// A high-level browser command. Navigation/history commands affect the **active
/// tab only**; tab commands affect the tab list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserAction {
    /// Navigate the active tab to a URL string.
    Navigate(String),
    /// Active tab: go back.
    Back,
    /// Active tab: go forward.
    Forward,
    /// Active tab: reload.
    Reload,
    /// Active tab: return to the new-tab (home) page.
    Home,
    /// Open and activate a new blank tab.
    NewTab,
    /// Close a tab.
    CloseTab(TabId),
    /// Activate an existing tab.
    SwitchTab(TabId),
    /// Toggle a bookmark for the active page.
    ToggleBookmark,
    /// Open the native history view in the active tab.
    ShowHistory,
    /// Open the native bookmarks view in the active tab.
    ShowBookmarks,
    /// Open the native downloads view in the active tab.
    ShowDownloads,
    /// Open the native settings view in the active tab.
    ShowSettings,
    /// Toggle the bookmarks bar visibility (and persist it).
    ToggleBookmarksBar,
    /// Move the tab at `from` to index `to` (drag-to-reorder).
    ReorderTab { from: usize, to: usize },
}

/// Multi-tab browser state: a tab manager, chrome, and an address bar. The
/// address-bar *draft* belongs to the app chrome (not each tab); switching or
/// navigating tabs resets it to the active tab's URL.
pub struct BrowserAppState {
    /// The open tabs and the active-tab invariant.
    pub tabs: TabManager,
    /// Browser chrome layout.
    pub chrome: ChromeLayout,
    /// Address bar state (app-level draft).
    pub address_bar: AddressBarState,
    /// Current focus context.
    pub focus: BrowserFocus,
    /// Persistent profile (history/bookmarks/downloads/settings/session).
    pub profile: BrowserProfile,
    /// Current address-bar suggestions (empty when the dropdown is hidden).
    pub suggestions: Vec<Suggestion>,
    /// Cached `(label, url)` of the bookmarks bar buttons.
    pub bookmark_bar: Vec<(String, String)>,
    /// Whether the active page's URL is bookmarked (drives the star fill).
    pub current_bookmarked: bool,
}

impl BrowserAppState {
    /// Load a page into the first tab and initialize the browser state.
    pub fn load(input: &str, viewport_width: u32, viewport_height: u32) -> MochaResult<Self> {
        let tabs = TabManager::with_loaded(input, viewport_width, viewport_height)?;
        Ok(Self::from_tabs(tabs, viewport_width, viewport_height))
    }

    /// Start with the internal new-tab (home) page — used when the app is
    /// launched without a document argument. Needs no network.
    pub fn start(viewport_width: u32, viewport_height: u32) -> MochaResult<Self> {
        let tabs = TabManager::new(viewport_width, viewport_height)?;
        Ok(Self::from_tabs(tabs, viewport_width, viewport_height))
    }

    /// Load `input` into the first tab; if the load fails, open the browser
    /// anyway showing an internal error page with the failure message, and
    /// keep the attempted input in the address bar so it can be corrected.
    /// Only errors if even the internal page cannot render.
    pub fn load_or_error_page(
        input: &str,
        viewport_width: u32,
        viewport_height: u32,
    ) -> MochaResult<Self> {
        match Self::load(input, viewport_width, viewport_height) {
            Ok(app) => Ok(app),
            Err(error) => {
                let tabs = TabManager::with_load_error(
                    input,
                    &error.to_string(),
                    viewport_width,
                    viewport_height,
                )?;
                let mut app = Self::from_tabs(tabs, viewport_width, viewport_height);
                app.address_bar.draft_text = input.to_string();
                Ok(app)
            }
        }
    }

    fn from_tabs(tabs: TabManager, viewport_width: u32, viewport_height: u32) -> Self {
        let url = tabs.active().url().cloned();
        let mut app = Self {
            tabs,
            chrome: ChromeLayout::new(viewport_width as f32, viewport_height as f32),
            address_bar: AddressBarState::new(url),
            focus: BrowserFocus::Page,
            profile: BrowserProfile::none(),
            suggestions: Vec::new(),
            bookmark_bar: Vec::new(),
            current_bookmarked: false,
        };
        app.refresh_chrome_state();
        app
    }

    /// Attach a persistent profile, apply its preferences, restore the previous
    /// session if enabled, and refresh chrome state. Returns `self` for chaining.
    pub fn with_profile(mut self, profile: BrowserProfile) -> Self {
        self.profile = profile;
        self.chrome
            .set_bookmarks_bar_visible(self.profile.show_bookmarks_bar());
        self.refresh_chrome_state();
        self
    }

    /// Build a browser, restoring the previous session from `profile` when the
    /// preference is set and a session exists; otherwise start on the home page
    /// (or load `target` if given).
    pub fn launch(
        profile: BrowserProfile,
        target: Option<&str>,
        viewport_width: u32,
        viewport_height: u32,
    ) -> MochaResult<Self> {
        if let Some(input) = target {
            return Ok(
                Self::load_or_error_page(input, viewport_width, viewport_height)?
                    .with_profile(profile),
            );
        }
        if profile.restore_session() {
            if let Some(stored) = profile.load_session() {
                let snapshot: SessionSnapshot = stored.into();
                if !snapshot.tabs.is_empty() {
                    let tabs = TabManager::restore(&snapshot, viewport_width, viewport_height)?;
                    return Ok(Self::from_tabs(tabs, viewport_width, viewport_height)
                        .with_profile(profile));
                }
            }
        }
        Ok(Self::start(viewport_width, viewport_height)?.with_profile(profile))
    }

    /// The active tab's page (for the window driver: display list, scroll, etc.).
    pub fn active_page(&self) -> &DesktopPageState {
        self.tabs.active().page()
    }

    /// Resize the window and viewport (all tabs + chrome).
    pub fn resize(&mut self, width: u32, height: u32) -> MochaResult<()> {
        self.tabs.resize(width, height)?;
        self.chrome.resize(width as f32, height as f32);
        Ok(())
    }

    /// Run a high-level browser action.
    pub fn dispatch(&mut self, action: BrowserAction) -> MochaResult<()> {
        match action {
            BrowserAction::Navigate(input) => {
                let url = Url::parse(&input)?;
                self.tabs.navigate_active(url)?;
                self.after_navigation();
            }
            BrowserAction::Back => {
                if self.tabs.back_active()? {
                    self.after_navigation();
                }
            }
            BrowserAction::Forward => {
                if self.tabs.forward_active()? {
                    self.after_navigation();
                }
            }
            BrowserAction::Reload => {
                self.tabs.reload_active()?;
            }
            BrowserAction::Home => {
                let home = self.profile.home_url();
                if home.trim().is_empty() {
                    self.tabs.home_active()?;
                    self.focus = BrowserFocus::Page;
                    self.sync_address_bar();
                } else {
                    self.dispatch(BrowserAction::Navigate(home))?;
                }
            }
            BrowserAction::NewTab => {
                self.tabs.new_tab()?;
                self.focus = BrowserFocus::Page;
                self.sync_address_bar();
                self.persist_session();
            }
            BrowserAction::CloseTab(id) => {
                self.tabs.close_tab(id)?;
                self.sync_address_bar();
                self.persist_session();
            }
            BrowserAction::SwitchTab(id) => {
                self.tabs.switch_tab(id)?;
                self.focus = BrowserFocus::Page;
                self.sync_address_bar();
                self.persist_session();
            }
            BrowserAction::ToggleBookmark => self.toggle_bookmark(),
            BrowserAction::ShowHistory => self.open_view(self.history_view())?,
            BrowserAction::ShowBookmarks => self.open_view(self.bookmarks_view())?,
            BrowserAction::ShowDownloads => self.open_view(self.downloads_view())?,
            BrowserAction::ShowSettings => self.open_view(self.settings_view())?,
            BrowserAction::ToggleBookmarksBar => {
                let now = !self.chrome.show_bookmarks_bar;
                self.chrome.set_bookmarks_bar_visible(now);
                self.profile.set_bool(SETTING_SHOW_BOOKMARKS_BAR, now);
                let (w, h) = self.viewport();
                self.tabs.resize(w, h)?;
            }
            BrowserAction::ReorderTab { from, to } => {
                if self.tabs.move_tab(from, to) {
                    self.persist_session();
                }
            }
        }
        Ok(())
    }

    /// Common bookkeeping after a successful navigation: sync the address bar,
    /// record the visit, refresh the bookmark star, hide suggestions, persist.
    fn after_navigation(&mut self) {
        if let Some(url) = self.tabs.active().url().cloned() {
            let title = self.tabs.active().title().to_string();
            self.profile.record_visit(&url, Some(&title));
        }
        self.sync_address_bar();
        self.persist_session();
    }

    /// Point the address bar at the active tab's URL, drop suggestions, and
    /// refresh chrome (bookmark star + bookmarks bar).
    fn sync_address_bar(&mut self) {
        let url = self.tabs.active().url().cloned();
        self.address_bar.set_current_url(url);
        if self.focus == BrowserFocus::AddressBar {
            self.focus = BrowserFocus::Page;
        }
        self.clear_suggestions();
        self.refresh_chrome_state();
    }

    /// Recompute the bookmark star state and bookmarks-bar buttons from the
    /// profile and push counts into the chrome layout.
    pub fn refresh_chrome_state(&mut self) {
        self.current_bookmarked = self
            .tabs
            .active()
            .url()
            .is_some_and(|u| self.profile.is_bookmarked(u));
        self.bookmark_bar = self
            .profile
            .bookmarks()
            .into_iter()
            .map(|b| {
                let label = if b.title.is_empty() {
                    b.url.clone()
                } else {
                    b.title
                };
                (label, b.url)
            })
            .collect();
        self.chrome.set_bookmark_count(self.bookmark_bar.len());
    }

    // --- bookmarks / menu / views -------------------------------------------

    /// Toggle a bookmark for the active page (no-op on internal pages).
    fn toggle_bookmark(&mut self) {
        let Some(url) = self.tabs.active().url().cloned() else {
            return;
        };
        let title = self.tabs.active().title().to_string();
        self.current_bookmarked = self.profile.toggle_bookmark(&url, &title);
        self.refresh_chrome_state();
    }

    /// Open/close the overflow menu.
    pub fn toggle_menu(&mut self) {
        let open = !self.chrome.menu_open;
        self.chrome.set_menu(open, MENU_ITEMS.len());
    }

    /// Run an overflow-menu command.
    pub fn run_menu_command(&mut self, command: MenuCommand) -> MochaResult<()> {
        self.chrome.set_menu(false, MENU_ITEMS.len());
        match command {
            MenuCommand::NewTab => self.dispatch(BrowserAction::NewTab),
            MenuCommand::History => self.dispatch(BrowserAction::ShowHistory),
            MenuCommand::Bookmarks => self.dispatch(BrowserAction::ShowBookmarks),
            MenuCommand::Downloads => self.dispatch(BrowserAction::ShowDownloads),
            MenuCommand::Settings => self.dispatch(BrowserAction::ShowSettings),
            MenuCommand::BookmarkPage => self.dispatch(BrowserAction::ToggleBookmark),
            MenuCommand::ToggleBookmarksBar => self.dispatch(BrowserAction::ToggleBookmarksBar),
        }
    }

    fn open_view(&mut self, view: InternalView) -> MochaResult<()> {
        self.tabs.show_native_on_active(view)?;
        self.focus = BrowserFocus::Page;
        self.sync_address_bar();
        Ok(())
    }

    fn history_view(&self) -> InternalView {
        let rows = self
            .profile
            .recent_history(300)
            .into_iter()
            .map(|h| {
                let title = h.title.clone().unwrap_or_else(|| h.url.clone());
                ListRow::new(
                    title,
                    format!("{} · {}", h.url, profile::relative_time(h.last_visited_ms)),
                )
                .with_url(h.url)
            })
            .collect();
        InternalView::History(rows)
    }

    fn bookmarks_view(&self) -> InternalView {
        let rows = self
            .profile
            .bookmarks()
            .into_iter()
            .map(|b| {
                let title = if b.title.is_empty() {
                    b.url.clone()
                } else {
                    b.title
                };
                ListRow::new(title, b.url.clone())
                    .with_url(b.url)
                    .with_accent(true)
                    .with_action(format!("unbookmark:{}", b.id))
            })
            .collect();
        InternalView::Bookmarks(rows)
    }

    fn downloads_view(&self) -> InternalView {
        let rows = self
            .profile
            .downloads()
            .into_iter()
            .map(|d| {
                let status = profile::download_status_label(d.status);
                ListRow::new(d.target_path, format!("{} · {}", d.url, status))
                    .with_url(d.url)
                    .with_accent(matches!(d.status, mocha_storage::DownloadStatus::Complete))
            })
            .collect();
        InternalView::Downloads(rows)
    }

    fn settings_view(&self) -> InternalView {
        let home = self.profile.home_url();
        let rows = vec![
            SettingRow {
                key: "show_bookmarks_bar".into(),
                label: "Show bookmarks bar".into(),
                kind: SettingKind::Toggle(self.profile.show_bookmarks_bar()),
            },
            SettingRow {
                key: SETTING_RESTORE_SESSION.into(),
                label: "Restore tabs on startup".into(),
                kind: SettingKind::Toggle(self.profile.restore_session()),
            },
            SettingRow {
                key: "home_cycle".into(),
                label: "Home page".into(),
                kind: SettingKind::Text(if home.is_empty() {
                    "New Tab page".into()
                } else {
                    home
                }),
            },
            SettingRow {
                key: "clear_history".into(),
                label: "Clear browsing history".into(),
                kind: SettingKind::Action,
            },
            SettingRow {
                key: "clear_downloads".into(),
                label: "Clear download history".into(),
                kind: SettingKind::Action,
            },
        ];
        InternalView::Settings(rows)
    }

    /// Apply a settings-row activation by key, then re-render the settings view.
    fn activate_setting(&mut self, key: &str) -> MochaResult<()> {
        match key {
            "show_bookmarks_bar" => {
                self.dispatch(BrowserAction::ToggleBookmarksBar)?;
            }
            SETTING_RESTORE_SESSION => {
                let now = !self.profile.restore_session();
                self.profile.set_bool(SETTING_RESTORE_SESSION, now);
            }
            "home_cycle" => {
                // Cycle home: New Tab page -> current page -> New Tab page.
                let current = self.profile.home_url();
                let next = if current.is_empty() {
                    self.tabs
                        .active()
                        .url()
                        .map(|u| u.normalized())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                self.profile.set_string(SETTING_HOME_URL, &next);
            }
            "clear_history" => self.profile.clear_history(),
            "clear_downloads" => self.profile.clear_downloads(),
            _ => {}
        }
        // Rebuild the settings view in place so toggles reflect the new state.
        let view = self.settings_view();
        self.tabs.show_native_on_active(view)?;
        self.refresh_chrome_state();
        Ok(())
    }

    // --- suggestions --------------------------------------------------------

    /// Recompute address-bar suggestions for the current draft and update the
    /// chrome dropdown row count.
    pub fn refresh_suggestions(&mut self) {
        if self.focus == BrowserFocus::AddressBar {
            self.suggestions = self
                .profile
                .suggestions(&self.address_bar.draft_text, MAX_SUGGESTIONS);
        } else {
            self.suggestions.clear();
        }
        self.chrome.set_suggestion_count(self.suggestions.len());
    }

    fn clear_suggestions(&mut self) {
        self.suggestions.clear();
        self.chrome.set_suggestion_count(0);
    }

    /// Navigate to the suggestion at `index` (used by click/Enter on the dropdown).
    pub fn accept_suggestion(&mut self, index: usize) -> MochaResult<()> {
        if let Some(s) = self.suggestions.get(index).cloned() {
            self.address_bar.draft_text = s.url.clone();
            self.dispatch(BrowserAction::Navigate(s.url))?;
        }
        Ok(())
    }

    // --- session persistence ------------------------------------------------

    /// Persist the current tab session (best effort).
    pub fn persist_session(&self) {
        if self.profile.is_persistent() {
            let snapshot: SessionSnapshot = self.tabs.snapshot();
            let stored: StoredSession = (&snapshot).into();
            self.profile.save_session(&stored);
        }
    }

    /// Handle a window click, dispatching to chrome, a native view, or the page.
    pub fn click(&mut self, window_x: f32, window_y: f32) -> MochaResult<bool> {
        let tab_ids = self.tabs.tab_ids();
        let hit = self.chrome.hit_test(window_x, window_y, &tab_ids);
        // A click anywhere outside the open menu closes it (except on the button).
        if self.chrome.menu_open
            && !matches!(
                hit,
                Some(ChromeElement::MenuItem(_)) | Some(ChromeElement::MenuButton)
            )
        {
            self.chrome.set_menu(false, MENU_ITEMS.len());
        }
        match hit {
            Some(ChromeElement::Tab(id)) => {
                self.dispatch(BrowserAction::SwitchTab(id))?;
                Ok(true)
            }
            Some(ChromeElement::TabClose(id)) => {
                self.dispatch(BrowserAction::CloseTab(id))?;
                Ok(true)
            }
            Some(ChromeElement::NewTabButton) => {
                self.dispatch(BrowserAction::NewTab)?;
                Ok(true)
            }
            Some(ChromeElement::BackButton) => {
                self.dispatch(BrowserAction::Back)?;
                Ok(true)
            }
            Some(ChromeElement::ForwardButton) => {
                self.dispatch(BrowserAction::Forward)?;
                Ok(true)
            }
            Some(ChromeElement::ReloadButton) => {
                self.dispatch(BrowserAction::Reload)?;
                Ok(true)
            }
            Some(ChromeElement::HomeButton) => {
                self.dispatch(BrowserAction::Home)?;
                Ok(true)
            }
            Some(ChromeElement::BookmarkButton) => {
                self.dispatch(BrowserAction::ToggleBookmark)?;
                Ok(true)
            }
            Some(ChromeElement::MenuButton) => {
                self.toggle_menu();
                Ok(true)
            }
            Some(ChromeElement::MenuItem(i)) => {
                if let Some((_, command)) = MENU_ITEMS.get(i) {
                    self.run_menu_command(*command)?;
                }
                Ok(true)
            }
            Some(ChromeElement::BookmarksBarItem(i)) => {
                if let Some((_, url)) = self.bookmark_bar.get(i).cloned() {
                    self.focus_page();
                    self.dispatch(BrowserAction::Navigate(url))?;
                }
                Ok(true)
            }
            Some(ChromeElement::SuggestionRow(i)) => {
                self.accept_suggestion(i)?;
                Ok(true)
            }
            Some(ChromeElement::AddressBar) => {
                self.address_bar.focus();
                self.focus = BrowserFocus::AddressBar;
                self.refresh_suggestions();
                Ok(true)
            }
            Some(ChromeElement::PageViewport) => {
                self.focus = BrowserFocus::Page;
                self.clear_suggestions();
                self.click_page(window_x, window_y)
            }
            None => {
                self.clear_suggestions();
                Ok(false)
            }
        }
    }

    /// Route a click inside the page viewport: native list/settings views first,
    /// otherwise the rendered document (links, form controls).
    fn click_page(&mut self, window_x: f32, window_y: f32) -> MochaResult<bool> {
        let viewport = self.chrome.page_viewport();
        if self.tabs.active().internal_view().is_some() {
            let scroll = self.tabs.active().view_scroll();
            let view = self.tabs.active().internal_view().unwrap().clone();
            if let Some(hit) = views::hit_view(&view, viewport, scroll, window_x, window_y) {
                return self.handle_view_hit(hit);
            }
            return Ok(false);
        }
        let page_x = window_x;
        let page_y = window_y - viewport.y;
        match self.tabs.active_mut().page_mut().click(page_x, page_y)? {
            DesktopAction::Navigate(url) => {
                self.tabs.navigate_active(url)?;
                self.after_navigation();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_view_hit(&mut self, hit: ViewHit) -> MochaResult<bool> {
        match hit {
            ViewHit::Navigate(url) => {
                self.focus_page();
                self.dispatch(BrowserAction::Navigate(url))?;
                Ok(true)
            }
            ViewHit::Action(action) => {
                if let Some(rest) = action.strip_prefix("unbookmark:") {
                    if let Ok(id) = rest.parse::<i64>() {
                        self.profile.remove_bookmark(id);
                        let view = self.bookmarks_view();
                        self.tabs.show_native_on_active(view)?;
                        self.refresh_chrome_state();
                    }
                }
                Ok(true)
            }
            ViewHit::Setting(key) => {
                self.activate_setting(&key)?;
                Ok(true)
            }
        }
    }

    fn focus_page(&mut self) {
        self.focus = BrowserFocus::Page;
        self.clear_suggestions();
    }

    /// Handle scroll wheel input: the active page, or a scrollable native view.
    pub fn scroll(&mut self, delta_y: f32) {
        if self.focus != BrowserFocus::Page {
            return;
        }
        let viewport = self.chrome.page_viewport();
        if let Some(view) = self.tabs.active().internal_view() {
            let count = match view {
                InternalView::History(r)
                | InternalView::Bookmarks(r)
                | InternalView::Downloads(r) => r.len(),
                InternalView::Settings(r) => r.len(),
                _ => 0,
            };
            let max = views::list_max_scroll(count, viewport);
            self.tabs.active_mut().scroll_view_by(delta_y, max);
        } else {
            self.tabs.active_mut().page_mut().scroll_by(delta_y);
        }
    }

    /// Handle keyboard input: text in the address bar, or forward to the page.
    pub fn input_char(&mut self, c: char) -> MochaResult<()> {
        match self.focus {
            BrowserFocus::AddressBar => {
                self.address_bar.input_char(c);
                self.refresh_suggestions();
            }
            BrowserFocus::Page => {
                self.tabs
                    .active_mut()
                    .page_mut()
                    .input_text(&c.to_string())?;
            }
        }
        Ok(())
    }

    /// Handle backspace.
    pub fn backspace(&mut self) -> MochaResult<()> {
        match self.focus {
            BrowserFocus::AddressBar => {
                self.address_bar.backspace();
                self.refresh_suggestions();
            }
            BrowserFocus::Page => {
                self.tabs.active_mut().page_mut().backspace()?;
            }
        }
        Ok(())
    }

    /// Handle address bar Enter: resolve the draft (URL or web search) and
    /// navigate the active tab. A failed load shows the native error view in the
    /// tab (keeping the typed text editable) instead of silently doing nothing.
    pub fn address_bar_submit(&mut self) -> MochaResult<()> {
        self.clear_suggestions();
        if let Some(url) = self.address_bar.submit() {
            let target = url.normalized();
            if let Err(error) = self.dispatch(BrowserAction::Navigate(target.clone())) {
                self.tabs
                    .show_error_on_active(&target, &error.to_string())?;
                self.sync_address_bar();
                self.address_bar.draft_text = target;
            }
        }
        Ok(())
    }

    /// Focus the address bar and select-all (Ctrl+L). Shows suggestions.
    pub fn focus_address_bar(&mut self) {
        self.address_bar.focus();
        self.focus = BrowserFocus::AddressBar;
        self.refresh_suggestions();
    }

    /// Handle Escape: close the menu, dismiss suggestions, or cancel an edit.
    pub fn escape(&mut self) {
        if self.chrome.menu_open {
            self.chrome.set_menu(false, MENU_ITEMS.len());
            return;
        }
        if !self.suggestions.is_empty() {
            self.clear_suggestions();
        }
        if self.focus == BrowserFocus::AddressBar {
            self.address_bar.cancel();
            self.focus = BrowserFocus::Page;
        }
    }

    /// The bookmark star is filled when the active page is bookmarked.
    pub fn is_current_bookmarked(&self) -> bool {
        self.current_bookmarked
    }

    // --- accessors for the window driver ------------------------------------

    pub fn display_list(&self) -> &[mocha_paint::DisplayCommand] {
        self.active_page().display_list()
    }

    pub fn images(&self) -> &[mocha_image::RasterImage] {
        self.active_page().images()
    }

    pub fn scroll_y(&self) -> f32 {
        self.active_page().scroll_y()
    }

    pub fn viewport(&self) -> (u32, u32) {
        self.active_page().viewport()
    }

    pub fn can_go_back(&self) -> bool {
        self.tabs.active().can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.tabs.active().can_go_forward()
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

    fn app() -> BrowserAppState {
        BrowserAppState::load(&example_path("basic/index.html"), 800, 600).unwrap()
    }

    #[test]
    fn browser_loads_and_initializes_one_tab() {
        let app = app();
        assert_eq!(app.focus, BrowserFocus::Page);
        assert_eq!(app.tabs.len(), 1);
        assert!(app.address_bar.current_url.is_some());
    }

    #[test]
    fn start_opens_the_home_page_without_a_target() {
        let app = BrowserAppState::start(800, 600).unwrap();
        assert_eq!(app.focus, BrowserFocus::Page);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs.active().title(), "New Tab");
        assert!(app.tabs.active().url().is_none());
        assert!(app.address_bar.current_url.is_none());
        // The active tab shows the native new-tab (home) view, not an HTML doc.
        assert_eq!(
            app.tabs.active().internal_view(),
            Some(&crate::tab::InternalView::NewTab)
        );
    }

    #[test]
    fn failed_load_opens_an_error_page_instead_of_exiting() {
        let app = BrowserAppState::load_or_error_page("definitely/missing.html", 800, 600).unwrap();
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs.active().title(), "Problem loading page");
        assert!(app.tabs.active().url().is_none());
        // The attempted input stays editable in the address bar.
        assert_eq!(app.address_bar.draft_text, "definitely/missing.html");
        // The tab shows the native error view carrying the attempted input.
        match app.tabs.active().internal_view() {
            Some(crate::tab::InternalView::LoadError { input, .. }) => {
                assert_eq!(input, "definitely/missing.html");
            }
            other => panic!("expected a load-error view, got {other:?}"),
        }
    }

    #[test]
    fn successful_load_through_load_or_error_page_behaves_like_load() {
        let app = BrowserAppState::load_or_error_page(&example_path("basic/index.html"), 800, 600)
            .unwrap();
        assert!(app.tabs.active().url().is_some());
        assert_ne!(app.tabs.active().title(), "Problem loading page");
    }

    #[test]
    fn address_bar_focus_click() {
        let mut app = app();
        let addr = app.chrome.address_bar();
        app.click(addr.x + addr.width / 2.0, addr.y + addr.height / 2.0)
            .unwrap();
        assert_eq!(app.focus, BrowserFocus::AddressBar);
        assert!(app.address_bar.focused);
    }

    #[test]
    fn new_tab_button_opens_and_activates_a_tab() {
        let mut app = app();
        let plus = app.chrome.new_tab_button(app.tabs.len());
        app.click(plus.x + plus.width / 2.0, plus.y + plus.height / 2.0)
            .unwrap();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.tabs.active().title(), "New Tab");
    }

    #[test]
    fn switching_tabs_updates_address_bar() {
        let mut app = app();
        let loaded_url = app.tabs.active().url().cloned();
        // Open a blank tab: address bar should now be empty (new-tab page).
        app.dispatch(BrowserAction::NewTab).unwrap();
        assert!(app.address_bar.current_url.is_none());
        // Switch back to the first (loaded) tab: address bar follows it.
        let first = app.tabs.tab_ids()[0];
        app.dispatch(BrowserAction::SwitchTab(first)).unwrap();
        assert_eq!(app.address_bar.current_url, loaded_url);
    }

    #[test]
    fn navigate_affects_active_tab_only() {
        let mut app = app();
        let first = app.tabs.tab_ids()[0];
        app.dispatch(BrowserAction::NewTab).unwrap();
        let second = app.tabs.active_id();
        let target = example_path("styled/index.html");
        app.dispatch(BrowserAction::Navigate(target.clone()))
            .unwrap();
        // Second tab navigated; first tab unchanged.
        assert_eq!(app.tabs.active_id(), second);
        assert!(app.tabs.active().url().is_some());
        let first_tab = app.tabs.tab(first).unwrap();
        assert_ne!(first_tab.url(), app.tabs.active().url());
    }

    #[test]
    fn back_and_forward_affect_active_tab_only() {
        let mut app = app();
        let first_url = app.tabs.active().url().cloned();
        let target = example_path("styled/index.html");
        app.dispatch(BrowserAction::Navigate(target)).unwrap();
        assert!(app.can_go_back());
        app.dispatch(BrowserAction::Back).unwrap();
        assert_eq!(app.tabs.active().url(), first_url.as_ref());
        assert!(app.can_go_forward());
        app.dispatch(BrowserAction::Forward).unwrap();
        assert!(!app.can_go_forward());
    }

    #[test]
    fn home_action_returns_active_tab_to_the_new_tab_page() {
        let mut app = app();
        assert!(app.tabs.active().url().is_some());
        app.dispatch(BrowserAction::Home).unwrap();
        assert_eq!(app.focus, BrowserFocus::Page);
        assert_eq!(app.tabs.active().title(), "New Tab");
        assert!(app.tabs.active().url().is_none());
        assert!(app.tabs.active().internal_view().is_some());
        assert!(app.address_bar.current_url.is_none());
    }

    #[test]
    fn close_tab_action_works() {
        let mut app = app();
        app.dispatch(BrowserAction::NewTab).unwrap();
        let second = app.tabs.active_id();
        app.dispatch(BrowserAction::CloseTab(second)).unwrap();
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn address_edit_then_switch_cancels_draft() {
        let mut app = app();
        app.dispatch(BrowserAction::NewTab).unwrap();
        let first = app.tabs.tab_ids()[0];
        // Start editing the address bar.
        app.address_bar.focus();
        app.focus = BrowserFocus::AddressBar;
        app.address_bar.input_char('z');
        // Switching tabs cancels the draft and shows the active tab's URL.
        app.dispatch(BrowserAction::SwitchTab(first)).unwrap();
        assert_eq!(app.focus, BrowserFocus::Page);
        assert_eq!(
            app.address_bar.draft_text,
            app.tabs.active().url().unwrap().normalized()
        );
    }

    #[test]
    fn resize_updates_chrome_and_active_page() {
        let mut app = app();
        app.resize(400, 300).unwrap();
        assert_eq!(app.viewport(), (400, 300));
        assert_eq!(app.chrome.window_width, 400.0);
    }

    // --- profile-backed features --------------------------------------------

    use mocha_storage::Profile;

    fn app_with_profile() -> BrowserAppState {
        BrowserAppState::load(&example_path("basic/index.html"), 1000, 700)
            .unwrap()
            .with_profile(BrowserProfile::new(Profile::private().unwrap()))
    }

    fn unique_temp_dir() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mocha_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn toggle_bookmark_updates_star_and_bar() {
        let mut app = app_with_profile();
        assert!(!app.is_current_bookmarked());
        app.dispatch(BrowserAction::ToggleBookmark).unwrap();
        assert!(app.is_current_bookmarked());
        assert_eq!(app.bookmark_bar.len(), 1);
        assert_eq!(app.chrome.bookmark_count, 1);
        app.dispatch(BrowserAction::ToggleBookmark).unwrap();
        assert!(!app.is_current_bookmarked());
        assert!(app.bookmark_bar.is_empty());
    }

    #[test]
    fn navigation_records_history_and_view_lists_it() {
        let mut app = app_with_profile();
        app.dispatch(BrowserAction::Navigate(example_path("styled/index.html")))
            .unwrap();
        app.dispatch(BrowserAction::ShowHistory).unwrap();
        match app.tabs.active().internal_view() {
            Some(InternalView::History(rows)) => {
                assert!(rows.iter().any(|r| r.detail.contains("styled")));
            }
            other => panic!("expected history view, got {other:?}"),
        }
    }

    #[test]
    fn typing_in_address_bar_produces_suggestions() {
        let mut app = app_with_profile();
        app.dispatch(BrowserAction::Navigate(example_path("styled/index.html")))
            .unwrap();
        app.focus_address_bar();
        for c in "styled".chars() {
            app.input_char(c).unwrap();
        }
        assert!(!app.suggestions.is_empty());
        assert_eq!(app.chrome.suggestion_count, app.suggestions.len());
        // Accepting a suggestion navigates to it.
        app.accept_suggestion(0).unwrap();
        assert!(app.suggestions.is_empty());
        assert!(app.tabs.active().url().is_some());
    }

    #[test]
    fn overflow_menu_opens_views_and_closes() {
        let mut app = app_with_profile();
        app.toggle_menu();
        assert!(app.chrome.menu_open);
        app.run_menu_command(MenuCommand::Bookmarks).unwrap();
        assert!(!app.chrome.menu_open);
        assert!(matches!(
            app.tabs.active().internal_view(),
            Some(InternalView::Bookmarks(_))
        ));
    }

    #[test]
    fn settings_toggle_persists_and_rebuilds_view() {
        let mut app = app_with_profile();
        app.dispatch(BrowserAction::ShowSettings).unwrap();
        assert!(app.chrome.show_bookmarks_bar);
        // Toggle the bookmarks bar via the settings row.
        app.handle_view_hit(ViewHit::Setting("show_bookmarks_bar".into()))
            .unwrap();
        assert!(!app.chrome.show_bookmarks_bar);
        // The settings view is rebuilt in place reflecting the new state.
        match app.tabs.active().internal_view() {
            Some(InternalView::Settings(rows)) => {
                let row = rows.iter().find(|r| r.key == "show_bookmarks_bar").unwrap();
                assert_eq!(row.kind, SettingKind::Toggle(false));
            }
            other => panic!("expected settings view, got {other:?}"),
        }
    }

    #[test]
    fn reorder_tab_action_moves_tabs() {
        let mut app = app_with_profile();
        app.dispatch(BrowserAction::NewTab).unwrap();
        let ids = app.tabs.tab_ids();
        app.dispatch(BrowserAction::ReorderTab { from: 0, to: 1 })
            .unwrap();
        assert_eq!(app.tabs.tab_ids(), vec![ids[1], ids[0]]);
    }

    #[test]
    fn toggle_bookmarks_bar_changes_chrome_height() {
        let mut app = app_with_profile();
        let tall = app.chrome.total_chrome_height;
        app.dispatch(BrowserAction::ToggleBookmarksBar).unwrap();
        assert!(!app.chrome.show_bookmarks_bar);
        assert!(app.chrome.total_chrome_height < tall);
    }

    #[test]
    fn session_persists_and_restores_across_launches() {
        let dir = unique_temp_dir();
        {
            let profile = BrowserProfile::new(Profile::persistent(&dir).unwrap());
            let mut app = BrowserAppState::load(&example_path("basic/index.html"), 800, 600)
                .unwrap()
                .with_profile(profile);
            app.dispatch(BrowserAction::NewTab).unwrap();
            app.dispatch(BrowserAction::Navigate(example_path("styled/index.html")))
                .unwrap();
            app.persist_session();
        }
        let profile = BrowserProfile::new(Profile::persistent(&dir).unwrap());
        let app = BrowserAppState::launch(profile, None, 800, 600).unwrap();
        assert_eq!(app.tabs.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
