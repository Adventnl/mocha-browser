//! The browser chrome theme: every colour the desktop shell paints with.
//!
//! Chrome rendering must take its colours from a [`BrowserTheme`] instead of
//! hardcoding them, so the palette stays consistent and changeable in one
//! place. The default theme is a modern neutral light theme (loosely in the
//! spirit of mainstream browsers, without copying anyone's branding).

use mocha_layout::Color;

/// Build an opaque colour from 8-bit channels.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

/// All colours used by the desktop browser chrome and native views.
#[derive(Debug, Clone)]
pub struct BrowserTheme {
    pub window_background: Color,
    pub tab_strip_background: Color,
    pub toolbar_background: Color,
    pub active_tab_background: Color,
    pub inactive_tab_background: Color,
    pub tab_hover_background: Color,
    pub tab_text: Color,
    pub address_bar_background: Color,
    pub address_bar_border: Color,
    pub address_bar_focused_border: Color,
    pub address_bar_text: Color,
    pub address_bar_placeholder: Color,
    pub button_hover_background: Color,
    pub button_active_background: Color,
    pub page_background: Color,
    // -- beyond the core palette ------------------------------------------------
    /// Toolbar icon strokes.
    pub icon: Color,
    /// Disabled toolbar icon strokes (faded back/forward).
    pub icon_disabled: Color,
    /// Secondary text (subtitles, hints, notes).
    pub text_secondary: Color,
    /// Hairline separating chrome from the page.
    pub chrome_border: Color,
    /// Card surfaces on native views (new tab, error page).
    pub card_background: Color,
    /// Card borders on native views.
    pub card_border: Color,
    /// Error-page accent (title), kept subtle.
    pub error_accent: Color,
}

impl Default for BrowserTheme {
    fn default() -> BrowserTheme {
        // A modern light theme in the spirit of current mainstream browsers
        // (neutral greys, a blue accent), without copying any brand palette.
        BrowserTheme {
            window_background: rgb(0xde, 0xe1, 0xe6),
            tab_strip_background: rgb(0xde, 0xe1, 0xe6),
            toolbar_background: rgb(0xff, 0xff, 0xff),
            active_tab_background: rgb(0xff, 0xff, 0xff),
            inactive_tab_background: rgb(0xd2, 0xd6, 0xdc),
            tab_hover_background: rgb(0xe8, 0xea, 0xed),
            tab_text: rgb(0x20, 0x21, 0x24),
            address_bar_background: rgb(0xf1, 0xf3, 0xf4),
            address_bar_border: rgb(0xf1, 0xf3, 0xf4),
            address_bar_focused_border: rgb(0x1a, 0x73, 0xe8),
            address_bar_text: rgb(0x20, 0x21, 0x24),
            address_bar_placeholder: rgb(0x5f, 0x63, 0x68),
            button_hover_background: rgb(0xe8, 0xea, 0xed),
            button_active_background: rgb(0xd8, 0xda, 0xdd),
            page_background: rgb(0xff, 0xff, 0xff),
            icon: rgb(0x5f, 0x63, 0x68),
            icon_disabled: rgb(0xbd, 0xc1, 0xc6),
            text_secondary: rgb(0x5f, 0x63, 0x68),
            chrome_border: rgb(0xda, 0xdc, 0xe0),
            card_background: rgb(0xff, 0xff, 0xff),
            card_border: rgb(0xe3, 0xe5, 0xe8),
            error_accent: rgb(0xd9, 0x3a, 0x2e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_exists_with_sane_palette() {
        let theme = BrowserTheme::default();
        // Light neutral chrome: backgrounds are light, text is dark.
        assert!(theme.toolbar_background.r > 0xE0);
        assert!(theme.tab_strip_background.r > 0xD0);
        assert_eq!(theme.active_tab_background, rgb(0xff, 0xff, 0xff));
        assert!(theme.tab_text.r < 0x40);
        // The focused address-bar border is a blue accent.
        assert!(theme.address_bar_focused_border.b > theme.address_bar_focused_border.r);
        // Disabled icons are lighter than enabled icons.
        assert!(theme.icon_disabled.r > theme.icon.r);
    }
}
