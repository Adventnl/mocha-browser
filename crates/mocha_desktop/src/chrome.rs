//! Browser chrome layout: toolbar, address bar, buttons, page viewport.

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
        Self { x, y, width, height }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// Chrome elements that can be hit-tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeElement {
    BackButton,
    ForwardButton,
    ReloadButton,
    AddressBar,
    PageViewport,
}

/// Layout of browser chrome: toolbar and address bar above the page viewport.
pub struct ChromeLayout {
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
        let toolbar_height = 40.0;
        let button_size = 28.0;
        let button_spacing = 6.0;
        let address_bar_height = 28.0;
        let total_chrome_height = toolbar_height + address_bar_height;

        Self {
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

    /// Rectangle of the back button in the toolbar.
    pub fn back_button(&self) -> Rect {
        let x = 6.0;
        let y = 6.0;
        Rect::new(x, y, self.button_size, self.button_size)
    }

    /// Rectangle of the forward button in the toolbar.
    pub fn forward_button(&self) -> Rect {
        let x = self.back_button().x + self.button_size + self.button_spacing;
        let y = 6.0;
        Rect::new(x, y, self.button_size, self.button_size)
    }

    /// Rectangle of the reload button in the toolbar.
    pub fn reload_button(&self) -> Rect {
        let x = self.forward_button().x + self.button_size + self.button_spacing;
        let y = 6.0;
        Rect::new(x, y, self.button_size, self.button_size)
    }

    /// Rectangle of the address bar (right side of toolbar, below the buttons).
    pub fn address_bar(&self) -> Rect {
        let x = 6.0;
        let y = self.toolbar_height + 6.0;
        let width = self.window_width - 12.0;
        Rect::new(x, y, width.max(0.0), self.address_bar_height)
    }

    /// Rectangle of the page viewport (below chrome).
    pub fn page_viewport(&self) -> Rect {
        let y = self.total_chrome_height;
        let height = (self.window_height - y).max(0.0);
        Rect::new(0.0, y, self.window_width, height)
    }

    /// Hit-test a window coordinate; returns the chrome element if hit, or None.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<ChromeElement> {
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

    #[test]
    fn chrome_layout_computes_button_rects() {
        let layout = ChromeLayout::new(800.0, 600.0);
        assert!(layout.back_button().width > 0.0);
        assert!(layout.forward_button().width > 0.0);
        assert!(layout.reload_button().width > 0.0);
    }

    #[test]
    fn address_bar_spans_width() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let addr = layout.address_bar();
        assert!(addr.width > 0.0);
        assert!(addr.y > layout.toolbar_height);
    }

    #[test]
    fn page_viewport_is_below_chrome() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let viewport = layout.page_viewport();
        assert_eq!(viewport.y, layout.total_chrome_height);
        assert!(viewport.height > 0.0);
    }

    #[test]
    fn hit_test_buttons() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let back = layout.back_button();
        assert_eq!(
            layout.hit_test(back.x + back.width / 2.0, back.y + back.height / 2.0),
            Some(ChromeElement::BackButton)
        );
    }

    #[test]
    fn hit_test_address_bar() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let addr = layout.address_bar();
        assert_eq!(
            layout.hit_test(addr.x + addr.width / 2.0, addr.y + addr.height / 2.0),
            Some(ChromeElement::AddressBar)
        );
    }

    #[test]
    fn hit_test_page_viewport() {
        let layout = ChromeLayout::new(800.0, 600.0);
        let viewport = layout.page_viewport();
        assert_eq!(
            layout.hit_test(viewport.x + viewport.width / 2.0, viewport.y + viewport.height / 2.0),
            Some(ChromeElement::PageViewport)
        );
    }
}
