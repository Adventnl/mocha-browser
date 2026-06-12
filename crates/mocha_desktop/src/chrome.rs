//! Browser chrome layout: tab strip, toolbar, address bar, buttons, page viewport.
//!
//! Vertical stack (top to bottom): tab strip (tabs + new-tab button), toolbar
//! (back/forward/reload/home buttons and the address bar on one row), a
//! hairline border, then the page viewport. All geometry is in window
//! coordinates; sizes come from [`ChromeMetrics`] and colours from
//! [`crate::theme::BrowserTheme`].

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

/// Chrome elements that can be hit-tested (and hovered).
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
    HomeButton,
    AddressBar,
    PageViewport,
}

/// The fixed sizes of the browser chrome (device pixels).
#[derive(Debug, Clone, Copy)]
pub struct ChromeMetrics {
    pub tab_strip_height: f32,
    pub toolbar_height: f32,
    pub tab_height: f32,
    pub tab_min_width: f32,
    pub tab_max_width: f32,
    pub new_tab_button_size: f32,
    pub toolbar_button_size: f32,
    pub toolbar_padding: f32,
    pub address_bar_height: f32,
    pub address_bar_radius: f32,
    /// Extra band below the toolbar (the chrome's bottom hairline).
    pub page_padding_top: f32,
    /// Horizontal padding at the left edge of the tab strip.
    pub tab_strip_padding: f32,
    /// Gap between adjacent tabs and after the last tab.
    pub tab_gap: f32,
    /// Square close button inside a tab.
    pub tab_close_size: f32,
    /// Gap between toolbar buttons.
    pub button_gap: f32,
    /// Corner radius of tabs (top corners).
    pub tab_radius: f32,
}

impl Default for ChromeMetrics {
    fn default() -> ChromeMetrics {
        ChromeMetrics {
            tab_strip_height: 36.0,
            toolbar_height: 48.0,
            tab_height: 32.0,
            tab_min_width: 140.0,
            tab_max_width: 220.0,
            new_tab_button_size: 28.0,
            toolbar_button_size: 32.0,
            toolbar_padding: 8.0,
            address_bar_height: 34.0,
            address_bar_radius: 16.0,
            page_padding_top: 1.0,
            tab_strip_padding: 8.0,
            tab_gap: 4.0,
            tab_close_size: 16.0,
            button_gap: 4.0,
            tab_radius: 8.0,
        }
    }
}

/// Layout of browser chrome for a given window size.
pub struct ChromeLayout {
    pub metrics: ChromeMetrics,
    pub total_chrome_height: f32,
    pub window_width: f32,
    pub window_height: f32,
}

impl ChromeLayout {
    pub fn new(window_width: f32, window_height: f32) -> Self {
        let metrics = ChromeMetrics::default();
        Self {
            metrics,
            total_chrome_height: metrics.tab_strip_height
                + metrics.toolbar_height
                + metrics.page_padding_top,
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
        Rect::new(0.0, 0.0, self.window_width, self.metrics.tab_strip_height)
    }

    /// The per-tab width given `count` tabs (clamped to [min, max]).
    fn tab_width(&self, count: usize) -> f32 {
        let m = &self.metrics;
        if count == 0 {
            return m.tab_max_width;
        }
        let reserved = m.tab_strip_padding * 2.0 + m.new_tab_button_size + m.tab_gap * count as f32;
        let available = (self.window_width - reserved).max(0.0);
        (available / count as f32).clamp(m.tab_min_width, m.tab_max_width)
    }

    /// The rectangle of tab `index` (of `count` tabs). Tabs sit at the bottom
    /// of the strip so the active tab visually joins the toolbar.
    pub fn tab_rect(&self, index: usize, count: usize) -> Rect {
        let m = &self.metrics;
        let width = self.tab_width(count);
        let x = m.tab_strip_padding + index as f32 * (width + m.tab_gap);
        let y = m.tab_strip_height - m.tab_height;
        Rect::new(x, y, width, m.tab_height)
    }

    /// The close-button rectangle inside tab `index`.
    pub fn tab_close_rect(&self, index: usize, count: usize) -> Rect {
        let m = &self.metrics;
        let tab = self.tab_rect(index, count);
        let size = m.tab_close_size;
        Rect::new(
            tab.x + tab.width - size - 8.0,
            tab.y + (tab.height - size) / 2.0,
            size,
            size,
        )
    }

    /// The "new tab" (+) button, just right of the last tab.
    pub fn new_tab_button(&self, count: usize) -> Rect {
        let m = &self.metrics;
        let width = self.tab_width(count);
        let size = m.new_tab_button_size;
        let x = (m.tab_strip_padding + count as f32 * (width + m.tab_gap))
            .min(self.window_width - size - m.tab_strip_padding)
            .max(0.0);
        let tab_y = m.tab_strip_height - m.tab_height;
        Rect::new(x, tab_y + (m.tab_height - size) / 2.0, size, size)
    }

    // --- toolbar (one row: buttons + address bar) ----------------------------

    /// The whole toolbar band.
    pub fn toolbar(&self) -> Rect {
        Rect::new(
            0.0,
            self.metrics.tab_strip_height,
            self.window_width,
            self.metrics.toolbar_height,
        )
    }

    fn toolbar_button(&self, slot: usize) -> Rect {
        let m = &self.metrics;
        let size = m.toolbar_button_size;
        let x = m.toolbar_padding + slot as f32 * (size + m.button_gap);
        let y = m.tab_strip_height + (m.toolbar_height - size) / 2.0;
        Rect::new(x, y, size, size)
    }

    /// Rectangle of the back button.
    pub fn back_button(&self) -> Rect {
        self.toolbar_button(0)
    }

    /// Rectangle of the forward button.
    pub fn forward_button(&self) -> Rect {
        self.toolbar_button(1)
    }

    /// Rectangle of the reload button.
    pub fn reload_button(&self) -> Rect {
        self.toolbar_button(2)
    }

    /// Rectangle of the home button.
    pub fn home_button(&self) -> Rect {
        self.toolbar_button(3)
    }

    /// Rectangle of the address bar (fills the toolbar right of the buttons).
    pub fn address_bar(&self) -> Rect {
        let m = &self.metrics;
        let home = self.home_button();
        let x = home.x + home.width + m.toolbar_padding;
        let y = m.tab_strip_height + (m.toolbar_height - m.address_bar_height) / 2.0;
        let width = self.window_width - x - m.toolbar_padding;
        Rect::new(x, y, width.max(0.0), m.address_bar_height)
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
        if y < self.metrics.tab_strip_height {
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
        } else if self.home_button().contains(x, y) {
            Some(ChromeElement::HomeButton)
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
    fn metrics_are_modern_browser_sizes() {
        let m = ChromeMetrics::default();
        assert_eq!(m.tab_strip_height, 36.0);
        assert_eq!(m.toolbar_height, 48.0);
        assert_eq!(m.tab_height, 32.0);
        assert_eq!(m.address_bar_height, 34.0);
    }

    #[test]
    fn toolbar_starts_below_the_tab_strip() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        assert_eq!(layout.toolbar().y, layout.metrics.tab_strip_height);
        assert!(layout.back_button().y > layout.metrics.tab_strip_height);
    }

    #[test]
    fn page_viewport_is_below_all_chrome() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        let viewport = layout.page_viewport();
        assert_eq!(viewport.y, layout.total_chrome_height);
        assert_eq!(
            layout.total_chrome_height,
            layout.metrics.tab_strip_height
                + layout.metrics.toolbar_height
                + layout.metrics.page_padding_top
        );
        assert!(viewport.height > 0.0);
        // The address bar (the lowest toolbar element) never overlaps the page.
        let addr = layout.address_bar();
        assert!(addr.y + addr.height <= viewport.y);
    }

    #[test]
    fn address_bar_has_expected_height_and_padding() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        let addr = layout.address_bar();
        assert_eq!(addr.height, layout.metrics.address_bar_height);
        // Padding after the home button and before the right window edge.
        let home = layout.home_button();
        assert_eq!(addr.x, home.x + home.width + layout.metrics.toolbar_padding);
        assert_eq!(
            addr.x + addr.width,
            layout.window_width - layout.metrics.toolbar_padding
        );
    }

    #[test]
    fn toolbar_buttons_are_vertically_aligned_and_ordered() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        let buttons = [
            layout.back_button(),
            layout.forward_button(),
            layout.reload_button(),
            layout.home_button(),
        ];
        for pair in buttons.windows(2) {
            assert_eq!(pair[0].y, pair[1].y, "buttons share a baseline");
            assert!(pair[1].x > pair[0].x, "buttons are ordered left to right");
        }
        // Centered in the toolbar band.
        let toolbar = layout.toolbar();
        let center = toolbar.y + toolbar.height / 2.0;
        let button_center = buttons[0].y + buttons[0].height / 2.0;
        assert!((center - button_center).abs() < 0.51);
    }

    #[test]
    fn resizing_recomputes_layout() {
        let mut layout = ChromeLayout::new(1200.0, 800.0);
        let before = layout.address_bar().width;
        layout.resize(800.0, 600.0);
        let after = layout.address_bar().width;
        assert!(after < before, "narrower window, narrower address bar");
        assert_eq!(layout.page_viewport().width, 800.0);
    }

    #[test]
    fn tabs_sit_at_the_bottom_of_the_strip() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        let tab = layout.tab_rect(0, 1);
        assert_eq!(
            tab.y + tab.height,
            layout.metrics.tab_strip_height,
            "active tab touches the toolbar"
        );
        assert_eq!(tab.height, layout.metrics.tab_height);
        assert!(tab.x >= layout.metrics.tab_strip_padding);
    }

    #[test]
    fn tab_rects_are_computed_and_ordered() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        let a = layout.tab_rect(0, 3);
        let b = layout.tab_rect(1, 3);
        assert!(a.width >= layout.metrics.tab_min_width);
        assert!(a.width <= layout.metrics.tab_max_width);
        assert!(b.x >= a.x + a.width, "tabs do not overlap");
    }

    #[test]
    fn hit_test_active_and_inactive_tabs() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        let ids = tabs(3);
        let t0 = layout.tab_rect(0, 3);
        let t2 = layout.tab_rect(2, 3);
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
        let layout = ChromeLayout::new(1200.0, 800.0);
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
        let layout = ChromeLayout::new(1200.0, 800.0);
        let ids = tabs(2);
        let plus = layout.new_tab_button(2);
        assert_eq!(
            layout.hit_test(plus.x + plus.width / 2.0, plus.y + plus.height / 2.0, &ids),
            Some(ChromeElement::NewTabButton)
        );
    }

    #[test]
    fn hit_test_toolbar_buttons_and_home() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        for (rect, expected) in [
            (layout.back_button(), ChromeElement::BackButton),
            (layout.forward_button(), ChromeElement::ForwardButton),
            (layout.reload_button(), ChromeElement::ReloadButton),
            (layout.home_button(), ChromeElement::HomeButton),
        ] {
            assert_eq!(
                layout.hit_test(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0, &[]),
                Some(expected)
            );
        }
    }

    #[test]
    fn hit_test_address_bar() {
        let layout = ChromeLayout::new(1200.0, 800.0);
        let addr = layout.address_bar();
        assert_eq!(
            layout.hit_test(addr.x + addr.width / 2.0, addr.y + addr.height / 2.0, &[]),
            Some(ChromeElement::AddressBar)
        );
    }

    #[test]
    fn hit_test_page_viewport() {
        let layout = ChromeLayout::new(1200.0, 800.0);
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
