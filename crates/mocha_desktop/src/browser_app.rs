//! Browser application state: a tab strip + chrome + address bar over the
//! active tab's page.

use crate::address_bar::AddressBarState;
use crate::chrome::{ChromeElement, ChromeLayout};
use crate::tab::{TabId, TabManager};
use mocha_error::MochaResult;
use mocha_url::Url;

use super::{DesktopAction, DesktopPageState};

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
        Self {
            tabs,
            chrome: ChromeLayout::new(viewport_width as f32, viewport_height as f32),
            address_bar: AddressBarState::new(url),
            focus: BrowserFocus::Page,
        }
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
                self.sync_address_bar();
            }
            BrowserAction::Back => {
                if self.tabs.back_active()? {
                    self.sync_address_bar();
                }
            }
            BrowserAction::Forward => {
                if self.tabs.forward_active()? {
                    self.sync_address_bar();
                }
            }
            BrowserAction::Reload => {
                self.tabs.reload_active()?;
            }
            BrowserAction::Home => {
                self.tabs.home_active()?;
                self.focus = BrowserFocus::Page;
                self.sync_address_bar();
            }
            BrowserAction::NewTab => {
                self.tabs.new_tab()?;
                self.focus = BrowserFocus::Page;
                self.sync_address_bar();
            }
            BrowserAction::CloseTab(id) => {
                self.tabs.close_tab(id)?;
                self.sync_address_bar();
            }
            BrowserAction::SwitchTab(id) => {
                self.tabs.switch_tab(id)?;
                self.focus = BrowserFocus::Page;
                self.sync_address_bar();
            }
        }
        Ok(())
    }

    /// Point the address bar at the active tab's URL and cancel any draft edit.
    fn sync_address_bar(&mut self) {
        let url = self.tabs.active().url().cloned();
        self.address_bar.set_current_url(url);
        if self.focus == BrowserFocus::AddressBar {
            self.focus = BrowserFocus::Page;
        }
    }

    /// Handle a window click, dispatching to chrome or the active page.
    pub fn click(&mut self, window_x: f32, window_y: f32) -> MochaResult<bool> {
        let tab_ids = self.tabs.tab_ids();
        match self.chrome.hit_test(window_x, window_y, &tab_ids) {
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
            Some(ChromeElement::AddressBar) => {
                self.address_bar.focus();
                self.focus = BrowserFocus::AddressBar;
                Ok(true)
            }
            Some(ChromeElement::PageViewport) => {
                self.focus = BrowserFocus::Page;
                let viewport = self.chrome.page_viewport();
                let page_x = window_x;
                let page_y = window_y - viewport.y;
                match self.tabs.active_mut().page_mut().click(page_x, page_y)? {
                    DesktopAction::Navigate(url) => {
                        self.tabs.navigate_active(url)?;
                        self.sync_address_bar();
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            None => Ok(false),
        }
    }

    /// Handle scroll wheel input (active tab only, when the page is focused).
    pub fn scroll(&mut self, delta_y: f32) {
        if self.focus == BrowserFocus::Page {
            self.tabs.active_mut().page_mut().scroll_by(delta_y);
        }
    }

    /// Handle keyboard input: text in the address bar, or forward to the page.
    pub fn input_char(&mut self, c: char) -> MochaResult<()> {
        match self.focus {
            BrowserFocus::AddressBar => self.address_bar.input_char(c),
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
            BrowserFocus::AddressBar => self.address_bar.backspace(),
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

    /// Handle Escape (cancel an address-bar edit).
    pub fn escape(&mut self) {
        if self.focus == BrowserFocus::AddressBar {
            self.address_bar.cancel();
            self.focus = BrowserFocus::Page;
        }
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
}
