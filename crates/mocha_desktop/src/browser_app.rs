//! Browser application state: single page + chrome + address bar.

use crate::address_bar::AddressBarState;
use crate::chrome::{ChromeElement, ChromeLayout};
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

/// Single-page browser state: a loaded page, chrome, and address bar.
pub struct BrowserAppState {
    /// The currently loaded page.
    pub page: DesktopPageState,
    /// Browser chrome layout.
    pub chrome: ChromeLayout,
    /// Address bar state.
    pub address_bar: AddressBarState,
    /// Navigation history (simple: back/forward stack).
    pub history: Vec<Url>,
    /// Current position in history.
    pub history_index: Option<usize>,
    /// Current focus context.
    pub focus: BrowserFocus,
}

impl BrowserAppState {
    /// Load a page and initialize the browser state.
    pub fn load(input: &str, viewport_width: u32, viewport_height: u32) -> MochaResult<Self> {
        let page = DesktopPageState::load(input, viewport_width, viewport_height)?;
        let url = Url::parse(input).ok();

        let history = url.clone().into_iter().collect::<Vec<_>>();
        let history_index = if history.is_empty() { None } else { Some(0) };

        Ok(Self {
            page,
            chrome: ChromeLayout::new(viewport_width as f32, viewport_height as f32),
            address_bar: AddressBarState::new(url),
            history,
            history_index,
            focus: BrowserFocus::Page,
        })
    }

    /// Resize the window and viewport.
    pub fn resize(&mut self, width: u32, height: u32) -> MochaResult<()> {
        self.page.resize(width, height)?;
        self.chrome.resize(width as f32, height as f32);
        Ok(())
    }

    /// Handle a window click, dispatching to chrome or page as needed.
    pub fn click(&mut self, window_x: f32, window_y: f32) -> MochaResult<bool> {
        match self.chrome.hit_test(window_x, window_y) {
            Some(ChromeElement::BackButton) => {
                self.handle_back();
                Ok(true)
            }
            Some(ChromeElement::ForwardButton) => {
                self.handle_forward();
                Ok(true)
            }
            Some(ChromeElement::ReloadButton) => {
                self.handle_reload()?;
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
                match self.page.click(page_x, page_y)? {
                    DesktopAction::Navigate(url) => {
                        self.navigate_to(url)?;
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            None => Ok(false),
        }
    }

    /// Handle scroll wheel input.
    pub fn scroll(&mut self, delta_y: f32) {
        if self.focus == BrowserFocus::Page {
            self.page.scroll_by(delta_y);
        }
    }

    /// Handle keyboard input: text in address bar, or forward to page.
    pub fn input_char(&mut self, c: char) -> MochaResult<()> {
        match self.focus {
            BrowserFocus::AddressBar => {
                self.address_bar.input_char(c);
            }
            BrowserFocus::Page => {
                self.page.input_text(&c.to_string())?;
            }
        }
        Ok(())
    }

    /// Handle backspace.
    pub fn backspace(&mut self) -> MochaResult<()> {
        match self.focus {
            BrowserFocus::AddressBar => {
                self.address_bar.backspace();
            }
            BrowserFocus::Page => {
                self.page.backspace()?;
            }
        }
        Ok(())
    }

    /// Handle address bar Enter (navigate).
    pub fn address_bar_submit(&mut self) -> MochaResult<()> {
        if let Some(url) = self.address_bar.submit() {
            self.navigate_to(url)?;
        }
        Ok(())
    }

    /// Handle Escape (blur address bar or unfocus).
    pub fn escape(&mut self) {
        if self.focus == BrowserFocus::AddressBar {
            self.address_bar.cancel();
            self.focus = BrowserFocus::Page;
        }
    }

    /// Navigate back in history.
    fn handle_back(&mut self) {
        if let Some(idx) = self.history_index {
            if idx > 0 {
                self.history_index = Some(idx - 1);
                if let Some(url) = self.history.get(idx - 1).cloned() {
                    if let Err(e) = self.page.navigate(&url) {
                        eprintln!("mocha: back navigation failed: {e}");
                        self.history_index = Some(idx);
                    } else {
                        self.address_bar.set_current_url(Some(url));
                    }
                }
            }
        }
    }

    /// Navigate forward in history.
    fn handle_forward(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                self.history_index = Some(idx + 1);
                if let Some(url) = self.history.get(idx + 1).cloned() {
                    if let Err(e) = self.page.navigate(&url) {
                        eprintln!("mocha: forward navigation failed: {e}");
                        self.history_index = Some(idx);
                    } else {
                        self.address_bar.set_current_url(Some(url));
                    }
                }
            }
        }
    }

    /// Reload the current page.
    fn handle_reload(&mut self) -> MochaResult<()> {
        if let Some(idx) = self.history_index {
            if let Some(url) = self.history.get(idx).cloned() {
                self.page.navigate(&url)?;
            }
        }
        Ok(())
    }

    /// Navigate to a new URL, adding to history.
    fn navigate_to(&mut self, url: Url) -> MochaResult<()> {
        self.page.navigate(&url)?;

        if let Some(idx) = self.history_index {
            self.history.truncate(idx + 1);
        } else {
            self.history.clear();
        }

        self.history.push(url.clone());
        self.history_index = Some(self.history.len() - 1);
        self.address_bar.set_current_url(Some(url));
        Ok(())
    }

    pub fn display_list(&self) -> &[mocha_paint::DisplayCommand] {
        self.page.display_list()
    }

    pub fn images(&self) -> &[mocha_image::RasterImage] {
        self.page.images()
    }

    pub fn scroll_y(&self) -> f32 {
        self.page.scroll_y()
    }

    pub fn viewport(&self) -> (u32, u32) {
        self.page.viewport()
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index.is_some_and(|idx| idx > 0)
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_index
            .is_some_and(|idx| idx + 1 < self.history.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn example_path(name: &str) -> String {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect("crate is under crates/mocha_desktop")
            .join("examples")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn browser_loads_and_initializes() {
        let app = BrowserAppState::load(&example_path("basic/index.html"), 800, 600).unwrap();
        assert_eq!(app.focus, BrowserFocus::Page);
    }

    #[test]
    fn address_bar_focus_click() {
        let mut app = BrowserAppState::load(&example_path("basic/index.html"), 800, 600).unwrap();
        let addr_rect = app.chrome.address_bar();
        app.click(
            addr_rect.x + addr_rect.width / 2.0,
            addr_rect.y + addr_rect.height / 2.0,
        )
        .unwrap();
        assert_eq!(app.focus, BrowserFocus::AddressBar);
        assert!(app.address_bar.focused);
    }

    #[test]
    fn reload_button_works() {
        let mut app = BrowserAppState::load(&example_path("basic/index.html"), 800, 600).unwrap();
        let reload_rect = app.chrome.reload_button();
        app.click(
            reload_rect.x + reload_rect.width / 2.0,
            reload_rect.y + reload_rect.height / 2.0,
        )
        .unwrap();
    }

    #[test]
    fn resize_updates_chrome_and_page() {
        let mut app = BrowserAppState::load(&example_path("basic/index.html"), 800, 600).unwrap();
        app.resize(400, 300).unwrap();
        let (w, h) = app.viewport();
        assert_eq!(w, 400);
        assert_eq!(h, 300);
    }
}
