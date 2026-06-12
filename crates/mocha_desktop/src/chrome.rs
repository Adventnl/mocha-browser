//! Browser chrome layout: tab strip, toolbar, address bar, buttons, page viewport.
//!
//! Vertical stack (top to bottom): tab strip, toolbar (back/forward/reload),
//! address bar, then the page viewport. All geometry is in window coordinates.

use crate::tab::TabId;

/// A rectangle in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// Chrome elements that can be hit-tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeElement {
    /// A tab's body (switch to it).
    Tab(TabId),
    /// A tab's close button.
    TabClose(TabId),
    /// The "new tab" (+) button.
    NewTabButton,
    BackButton,
    ForwardButton,
    ReloadButton,
    AddressBar,
    PageViewport,
}

/// Layout of browser chrome.
pub struct ChromeLayout {
    pub tab_strip_height: f32,
    pub tab_min_width: f32,
    pub tab_max_width: f32,
    pub new_tab_button_width: f32,
    pub tab_close_button_width: f32,
    pub toolbar_height: f32,
    pub button_size: f32,
    pub button_spacing: f32,
    pub address_bar_height: f32,
    pub total_chrome_height: f32,
    pub window_width: f32,
    pub window_height: f32,
}

impl ChromeLayout {
    pub fn new(window_width: f32, window_height: f32) -> Self {
        let tab_strip_height = 32.0;
        let toolbar_height = 40.0;
        let button_size = 28.0;
        let button_spacing = 6.0;
        let address_bar_height = 28.0;
        let total_chrome_height = tab_strip_height + toolbar_height + address_bar_height;

        Self {
            tab_strip_height,
            tab_min_width: 120.0,
            tab_max_width: 180.0,
            new_tab_button_width: 32.0,
            tab_close_button_width: 20.0,
            toolbar_height,
            button_size,
            button_spacing,
            address_bar_height,
            total_chrome_height,
            window_width: window_width.max(1.0),
            window_height: window_height.max(1.0),
        }
    }

    pub fn resize(&mut self, window_width: f32, window_height: f32) {
        self.window_width = window_width.max(1.0);
        self.window_height = window_height.max(1.0);
    }

    // --- tab strip -----------------------------------------------------------

    /// The whole tab strip band across the top.
    pub fn tab_strip(&self) -> Rect {
        Rect::new(0.0, 0.0, self.window_width, self.tab_strip_height)
    }

    /// The per-tab width given `count` tabs (clamped to [min, max]).
    fn tab_width(&self, count: usize) -> f32 {
        if count == 0 {
            return self.tab_max_width;
        }
        let available = (self.window_width - self.new_tab_button_width).max(0.0);
        (available / count as f32).clamp(self.tab_min_width, self.tab_max_width)
    }

    /// The rectangle of tab `index` (of `count` tabs).
    pub fn tab_rect(&self, index: usize, count: usize) -> Rect {
        let width = self.tab_width(count);
        Rect::new(index as f32 * width, 0.0, width, self.tab_strip_height)
    }

    /// The close-button rectangle inside tab `index`.
    pub fn tab_close_rect(&self, index: usize, count: usize) -> Rect {
        let tab = self.tab_rect(index, count);
        let pad = 4.0;
        let size = self.tab_close_button_width;
        Rect::new(
            tab.x + tab.width - size - pad,
            tab.y + (tab.height - size) / 2.0,
            size,
            size,
        )
    }

    /// The "new tab" button rectangle, just right of the last tab.
    pub fn new_tab_button(&self, count: usize) -> Rect {
        let width = self.tab_width(count);
        let x = (count as f32 * width).min(self.window_width - self.new_tab_button_width);
        Rect::new(
            x.max(0.0),
            0.0,
            self.new_tab_button_width,
            self.tab_strip_height,
        )
    }

    // --- toolbar (below the tab strip) --------------------------------------

    fn toolbar_y(&self) -> f32 {
        self.tab_strip_height + 6.0
    }

    /// Rectangle of the back button.
    pub fn back_button(&self) -> Rect {
        Rect::new(6.0, self.toolbar_y(), self.button_size, self.button_size)
    }

    /// Rectangle of the forward button.
    pub fn forward_button(&self) -> Rect {
        let x = self.back_button().x + self.button_size + self.button_spacing;
        Rect::new(x, self.toolbar_y(), self.button_size, self.button_size)
    }

    /// Rectangle of the reload button.
    pub fn reload_button(&self) -> Rect {
        let x = self.forward_button().x + self.button_size + self.button_spacing;
        Rect::new(x, self.toolbar_y(), self.button_size, self.button_size)
    }

    /// Rectangle of the address bar (below the toolbar buttons).
    pub fn address_bar(&self) -> Rect {
        let x = 6.0;
        let y = self.tab_strip_height + self.toolbar_height + 6.0;
        let width = self.window_width - 12.0;
        Rect::new(x, y, width.max(0.0), self.address_bar_height)
    }

    /// Rectangle of the page viewport (below all chrome).
    pub fn page_viewport(&self) -> Rect {
        let y = self.total_chrome_height;
        let height = (self.window_height - y).max(0.0);
        Rect::new(0.0, y, self.window_width, height)
    }

    /// Hit-test a window coordinate against the chrome. `tabs` provides the tab
    /// ids in strip order (so tab/close hits resolve to a [`TabId`]).
    pub fn hit_test(&self, x: f32, y: f32, tabs: &[TabId]) -> Option<ChromeElement> {
        // Tab strip band first.
        if y < self.tab_strip_height {
            let count = tabs.len();
            for (index, &id) in tabs.iter().enumerate() {
                // The close button sits inside the tab, so test it first.
                if self.tab_close_rect(index, count).contains(x, y) {
                    return Some(ChromeElement::TabClose(id));
                }
                if self.tab_rect(index, count).contains(x, y) {
                    return Some(ChromeElement::Tab(id));
                }
            }
            if self.new_tab_button(count).contains(x, y) {
                return Some(ChromeElement::NewTabButton);
            }
            return None;
        }

        if self.back_button().contains(x, y) {
            Some(ChromeElement::BackButton)
        } else if self.forward_button().contains(x, y) {
            Some(ChromeElement::ForwardButton)
        } else if self.reload_button().contains(x, y) {
            Some(ChromeElement::ReloadButton)
        } else if self.address_bar().contains(x, y) {
            Some(ChromeElement::AddressBar)
        } else if self.page_viewport().contains(x, y) {
            Some(ChromeElement::PageViewport)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(n: u64) -> Vec<TabId> {
        (0..n).map(TabId).collect()
    }

    #[test]
    fn chrome_layout_computes_button_rects() {
        let layout = ChromeLayout::new(800.0, 600.0);
        assert!(layout.back_button().width > 0.0);
        assert!(layout.forward_button().width > 0.0);
        assert!(layout.reload_button().width > 0.0);
    }

    #[test]
    fn toolbar_is_below_the_tab_strip() {
        let layout = ChromeLayout::new(800.0, 600.0);
        assert!(layout.back_button().y >= layout.tab_strip_height);
        assert!(layout.address_bar().y > layout.tab_strip_height + layout.toolbar_height);
    }

    #[test]
    fn page_viewport_is_below_all_chrome() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let viewport = layout.page_viewport();
        assert_eq!(viewport.y, layout.total_chrome_height);
        assert_eq!(
            layout.total_chrome_height,
            layout.tab_strip_height + layout.toolbar_height + layout.address_bar_height
        );
        assert!(viewport.height > 0.0);
    }

    #[test]
    fn tab_rects_are_computed_and_ordered() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let a = layout.tab_rect(0, 3);
        let b = layout.tab_rect(1, 3);
        assert!(a.width >= layout.tab_min_width && a.width <= layout.tab_max_width);
        assert!(b.x > a.x, "later tabs are further right");
        assert_eq!(a.y, 0.0);
        assert_eq!(a.height, layout.tab_strip_height);
    }

    #[test]
    fn hit_test_active_and_inactive_tabs() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let ids = tabs(3);
        let t0 = layout.tab_rect(0, 3);
        let t2 = layout.tab_rect(2, 3);
        // Click the body of tab 0 (left of its close button).
        assert_eq!(
            layout.hit_test(t0.x + 4.0, t0.y + t0.height / 2.0, &ids),
            Some(ChromeElement::Tab(TabId(0)))
        );
        assert_eq!(
            layout.hit_test(t2.x + 4.0, t2.y + t2.height / 2.0, &ids),
            Some(ChromeElement::Tab(TabId(2)))
        );
    }

    #[test]
    fn hit_test_tab_close_button() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let ids = tabs(2);
        let close = layout.tab_close_rect(1, 2);
        assert_eq!(
            layout.hit_test(
                close.x + close.width / 2.0,
                close.y + close.height / 2.0,
                &ids
            ),
            Some(ChromeElement::TabClose(TabId(1)))
        );
    }

    #[test]
    fn hit_test_new_tab_button() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let ids = tabs(2);
        let plus = layout.new_tab_button(2);
        assert_eq!(
            layout.hit_test(plus.x + plus.width / 2.0, plus.y + plus.height / 2.0, &ids),
            Some(ChromeElement::NewTabButton)
        );
    }

    #[test]
    fn hit_test_buttons_below_tab_strip() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let back = layout.back_button();
        assert_eq!(
            layout.hit_test(back.x + back.width / 2.0, back.y + back.height / 2.0, &[]),
            Some(ChromeElement::BackButton)
        );
    }

    #[test]
    fn hit_test_address_bar() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let addr = layout.address_bar();
        assert_eq!(
            layout.hit_test(addr.x + addr.width / 2.0, addr.y + addr.height / 2.0, &[]),
            Some(ChromeElement::AddressBar)
        );
    }

    #[test]
    fn hit_test_page_viewport() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let viewport = layout.page_viewport();
        assert_eq!(
            layout.hit_test(
                viewport.x + viewport.width / 2.0,
                viewport.y + viewport.height / 2.0,
                &[]
            ),
            Some(ChromeElement::PageViewport)
        );
    }
}
