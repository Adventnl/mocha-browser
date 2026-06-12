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
    /// Bookmark-the-current-page star (right of the address bar).
    BookmarkButton,
    /// Overflow / hamburger menu button (far right).
    MenuButton,
    /// A button on the bookmarks bar.
    BookmarksBarItem(usize),
    /// A row in the address-bar suggestions dropdown.
    SuggestionRow(usize),
    /// An item in the open overflow menu.
    MenuItem(usize),
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
    /// Height of the bookmarks bar band (when shown).
    pub bookmarks_bar_height: f32,
    /// Width of a single bookmarks-bar button.
    pub bookmark_item_width: f32,
    /// Gap between bookmarks-bar buttons.
    pub bookmark_item_gap: f32,
    /// Height of a single address-bar suggestion row.
    pub suggestion_row_height: f32,
    /// Height of a single overflow-menu item.
    pub menu_item_height: f32,
    /// Width of the overflow menu popup.
    pub menu_width: f32,
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
            bookmarks_bar_height: 30.0,
            bookmark_item_width: 150.0,
            bookmark_item_gap: 4.0,
            suggestion_row_height: 36.0,
            menu_item_height: 34.0,
            menu_width: 220.0,
        }
    }
}

/// Layout of browser chrome for a given window size.
pub struct ChromeLayout {
    pub metrics: ChromeMetrics,
    pub total_chrome_height: f32,
    pub window_width: f32,
    pub window_height: f32,
    /// Whether the bookmarks bar band is shown below the toolbar.
    pub show_bookmarks_bar: bool,
    /// Number of buttons currently on the bookmarks bar (for hit testing).
    pub bookmark_count: usize,
    /// Number of address-bar suggestion rows currently shown (0 = hidden).
    pub suggestion_count: usize,
    /// Whether the overflow menu popup is open.
    pub menu_open: bool,
    /// Number of items in the overflow menu.
    pub menu_item_count: usize,
}

impl ChromeLayout {
    pub fn new(window_width: f32, window_height: f32) -> Self {
        let metrics = ChromeMetrics::default();
        let mut layout = Self {
            metrics,
            total_chrome_height: 0.0,
            window_width: window_width.max(1.0),
            window_height: window_height.max(1.0),
            show_bookmarks_bar: true,
            bookmark_count: 0,
            suggestion_count: 0,
            menu_open: false,
            menu_item_count: 0,
        };
        layout.recompute_height();
        layout
    }

    fn recompute_height(&mut self) {
        let m = &self.metrics;
        self.total_chrome_height = m.tab_strip_height
            + m.toolbar_height
            + if self.show_bookmarks_bar {
                m.bookmarks_bar_height
            } else {
                0.0
            }
            + m.page_padding_top;
    }

    /// Show or hide the bookmarks bar (recomputes chrome height).
    pub fn set_bookmarks_bar_visible(&mut self, visible: bool) {
        self.show_bookmarks_bar = visible;
        self.recompute_height();
    }

    /// Set the number of bookmarks-bar buttons (for layout/hit testing).
    pub fn set_bookmark_count(&mut self, count: usize) {
        self.bookmark_count = count;
    }

    /// Set the number of visible suggestion rows (0 hides the dropdown).
    pub fn set_suggestion_count(&mut self, count: usize) {
        self.suggestion_count = count;
    }

    /// Open/close the overflow menu, with `item_count` items when open.
    pub fn set_menu(&mut self, open: bool, item_count: usize) {
        self.menu_open = open;
        self.menu_item_count = if open { item_count } else { 0 };
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

    /// A toolbar button anchored from the right edge (`slot` 0 = rightmost).
    fn toolbar_button_right(&self, slot: usize) -> Rect {
        let m = &self.metrics;
        let size = m.toolbar_button_size;
        let x = self.window_width - m.toolbar_padding - size - slot as f32 * (size + m.button_gap);
        let y = m.tab_strip_height + (m.toolbar_height - size) / 2.0;
        Rect::new(x, y, size, size)
    }

    /// Rectangle of the overflow / hamburger menu button (far right).
    pub fn menu_button(&self) -> Rect {
        self.toolbar_button_right(0)
    }

    /// Rectangle of the bookmark-the-page star button (left of the menu).
    pub fn bookmark_button(&self) -> Rect {
        self.toolbar_button_right(1)
    }

    /// Rectangle of the address bar (between the left buttons and the right
    /// buttons).
    pub fn address_bar(&self) -> Rect {
        let m = &self.metrics;
        let home = self.home_button();
        let x = home.x + home.width + m.toolbar_padding;
        let y = m.tab_strip_height + (m.toolbar_height - m.address_bar_height) / 2.0;
        let right = self.bookmark_button().x - m.toolbar_padding;
        let width = (right - x).max(0.0);
        Rect::new(x, y, width, m.address_bar_height)
    }

    // --- bookmarks bar -------------------------------------------------------

    /// The bookmarks-bar band (empty rect when hidden).
    pub fn bookmarks_bar(&self) -> Rect {
        if !self.show_bookmarks_bar {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        let m = &self.metrics;
        let y = m.tab_strip_height + m.toolbar_height;
        Rect::new(0.0, y, self.window_width, m.bookmarks_bar_height)
    }

    /// The rectangle of bookmarks-bar button `index`.
    pub fn bookmark_item_rect(&self, index: usize) -> Rect {
        let m = &self.metrics;
        let bar = self.bookmarks_bar();
        let x = m.toolbar_padding + index as f32 * (m.bookmark_item_width + m.bookmark_item_gap);
        let h = m.bookmarks_bar_height - 6.0;
        Rect::new(x, bar.y + 3.0, m.bookmark_item_width, h)
    }

    // --- suggestions dropdown ------------------------------------------------

    /// The rectangle of suggestion row `index` (drawn over the page).
    pub fn suggestion_row(&self, index: usize) -> Rect {
        let m = &self.metrics;
        let addr = self.address_bar();
        let y = addr.y + addr.height + 4.0 + index as f32 * m.suggestion_row_height;
        Rect::new(addr.x, y, addr.width, m.suggestion_row_height)
    }

    /// The bounding rectangle of the whole suggestions dropdown (for painting a
    /// backing panel), or `None` when no suggestions are shown.
    pub fn suggestions_panel(&self) -> Option<Rect> {
        if self.suggestion_count == 0 {
            return None;
        }
        let first = self.suggestion_row(0);
        let h = self.suggestion_count as f32 * self.metrics.suggestion_row_height;
        Some(Rect::new(first.x, first.y, first.width, h))
    }

    // --- overflow menu -------------------------------------------------------

    /// The rectangle of overflow-menu item `index` (drawn over the page).
    pub fn menu_item_rect(&self, index: usize) -> Rect {
        let m = &self.metrics;
        let button = self.menu_button();
        let x = (button.x + button.width - m.menu_width).max(4.0);
        let top = button.y + button.height + 4.0;
        Rect::new(
            x,
            top + index as f32 * m.menu_item_height,
            m.menu_width,
            m.menu_item_height,
        )
    }

    /// The bounding rectangle of the open overflow menu, or `None` when closed.
    pub fn menu_panel(&self) -> Option<Rect> {
        if !self.menu_open || self.menu_item_count == 0 {
            return None;
        }
        let first = self.menu_item_rect(0);
        let h = self.menu_item_count as f32 * self.metrics.menu_item_height;
        Some(Rect::new(first.x, first.y, first.width, h))
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

        // Open overflow menu wins over everything beneath it.
        if self.menu_open {
            for i in 0..self.menu_item_count {
                if self.menu_item_rect(i).contains(x, y) {
                    return Some(ChromeElement::MenuItem(i));
                }
            }
        }

        // Toolbar buttons + address bar.
        if self.back_button().contains(x, y) {
            return Some(ChromeElement::BackButton);
        }
        if self.forward_button().contains(x, y) {
            return Some(ChromeElement::ForwardButton);
        }
        if self.reload_button().contains(x, y) {
            return Some(ChromeElement::ReloadButton);
        }
        if self.home_button().contains(x, y) {
            return Some(ChromeElement::HomeButton);
        }
        if self.menu_button().contains(x, y) {
            return Some(ChromeElement::MenuButton);
        }
        if self.bookmark_button().contains(x, y) {
            return Some(ChromeElement::BookmarkButton);
        }
        if self.address_bar().contains(x, y) {
            return Some(ChromeElement::AddressBar);
        }

        // Suggestions dropdown floats over the page/bookmarks band.
        for i in 0..self.suggestion_count {
            if self.suggestion_row(i).contains(x, y) {
                return Some(ChromeElement::SuggestionRow(i));
            }
        }

        // Bookmarks bar band.
        if self.show_bookmarks_bar && self.bookmarks_bar().contains(x, y) {
            for i in 0..self.bookmark_count {
                if self.bookmark_item_rect(i).contains(x, y) {
                    return Some(ChromeElement::BookmarksBarItem(i));
                }
            }
            return None;
        }

        if self.page_viewport().contains(x, y) {
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
        // The bookmarks bar is shown by default, so it adds to the chrome height.
        assert_eq!(
            layout.total_chrome_height,
            layout.metrics.tab_strip_height
                + layout.metrics.toolbar_height
                + layout.metrics.bookmarks_bar_height
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
        // Padding after the home button and before the right-side buttons.
        let home = layout.home_button();
        assert_eq!(addr.x, home.x + home.width + layout.metrics.toolbar_padding);
        assert_eq!(
            addr.x + addr.width,
            layout.bookmark_button().x - layout.metrics.toolbar_padding
        );
        // The right-side buttons (bookmark star, menu) sit in order to the edge.
        assert!(layout.bookmark_button().x < layout.menu_button().x);
        assert_eq!(
            layout.menu_button().x + layout.menu_button().width,
            layout.window_width - layout.metrics.toolbar_padding
        );
    }

    #[test]
    fn bookmarks_bar_toggles_chrome_height() {
        let mut layout = ChromeLayout::new(1200.0, 800.0);
        let with_bar = layout.total_chrome_height;
        layout.set_bookmarks_bar_visible(false);
        assert!(layout.total_chrome_height < with_bar);
        assert_eq!(
            with_bar - layout.total_chrome_height,
            layout.metrics.bookmarks_bar_height
        );
    }

    #[test]
    fn hit_test_right_buttons_and_menu_and_suggestions() {
        let mut layout = ChromeLayout::new(1200.0, 800.0);
        let star = layout.bookmark_button();
        assert_eq!(
            layout.hit_test(star.x + star.width / 2.0, star.y + star.height / 2.0, &[]),
            Some(ChromeElement::BookmarkButton)
        );
        let menu = layout.menu_button();
        assert_eq!(
            layout.hit_test(menu.x + menu.width / 2.0, menu.y + menu.height / 2.0, &[]),
            Some(ChromeElement::MenuButton)
        );
        // Open the menu: items hit-test on top of the page.
        layout.set_menu(true, 5);
        let item = layout.menu_item_rect(2);
        assert_eq!(
            layout.hit_test(item.x + 10.0, item.y + item.height / 2.0, &[]),
            Some(ChromeElement::MenuItem(2))
        );
        layout.set_menu(false, 5);
        // Suggestions dropdown.
        layout.set_suggestion_count(3);
        let row = layout.suggestion_row(1);
        assert_eq!(
            layout.hit_test(row.x + 10.0, row.y + row.height / 2.0, &[]),
            Some(ChromeElement::SuggestionRow(1))
        );
    }

    #[test]
    fn hit_test_bookmarks_bar_items() {
        let mut layout = ChromeLayout::new(1200.0, 800.0);
        layout.set_bookmark_count(3);
        let item = layout.bookmark_item_rect(1);
        assert_eq!(
            layout.hit_test(item.x + 5.0, item.y + item.height / 2.0, &[]),
            Some(ChromeElement::BookmarksBarItem(1))
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
