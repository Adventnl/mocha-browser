//! Paint the whole browser window — page content (or a native view) plus the
//! chrome — into a [`Surface`].
//!
//! This lives in the library (not the `gui` window driver) so chrome rendering
//! is testable headlessly: the minifb event loop only pumps input and hands
//! the finished buffer to the OS window. All colours come from
//! [`BrowserTheme`]; all sizes from [`crate::chrome::ChromeMetrics`].

use mocha_raster::{rasterize_at, Surface};

use crate::browser_app::{BrowserAppState, BrowserFocus};
use crate::chrome::{ChromeElement, Rect};
use crate::icons;
use crate::text::Fonts;
use crate::theme::BrowserTheme;

/// The address bar's placeholder when it is empty and unfocused.
pub const ADDRESS_PLACEHOLDER: &str = "Search or enter address";

/// Transient per-frame input state that affects chrome painting only.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChromeInput {
    /// The chrome element currently under the mouse, if any.
    pub hover: Option<ChromeElement>,
    /// Whether the primary mouse button is held (pressed-button styling).
    pub mouse_down: bool,
    /// Whether the address-bar caret is in the visible phase of its blink.
    pub caret_visible: bool,
}

/// Render the full browser (page or native view, then chrome) into `surface`.
pub fn render_browser(
    surface: &mut Surface,
    app: &BrowserAppState,
    fonts: &mut Fonts,
    theme: &BrowserTheme,
    input: ChromeInput,
) {
    let viewport = app.chrome.page_viewport();
    if let Some(view) = app.tabs.active().internal_view() {
        surface.clear(theme.page_background);
        crate::views::render_view(view, surface, fonts, theme, viewport);
    } else {
        let chrome_top = app.chrome.total_chrome_height as i32;
        rasterize_at(
            surface,
            app.display_list(),
            app.images(),
            app.scroll_y(),
            chrome_top,
        );
    }
    render_chrome(surface, app, fonts, theme, input);
}

/// Paint the chrome bands (tab strip, toolbar, hairline) and their contents.
fn render_chrome(
    surface: &mut Surface,
    app: &BrowserAppState,
    fonts: &mut Fonts,
    theme: &BrowserTheme,
    input: ChromeInput,
) {
    let chrome = &app.chrome;
    let metrics = chrome.metrics;

    // Opaque chrome bands (also cover any page content scrolled underneath).
    let strip = chrome.tab_strip();
    surface.draw_rect(
        strip.x as i32,
        strip.y as i32,
        strip.width.ceil() as i32,
        strip.height.ceil() as i32,
        theme.tab_strip_background,
    );
    let toolbar = chrome.toolbar();
    surface.draw_rect(
        toolbar.x as i32,
        toolbar.y as i32,
        toolbar.width.ceil() as i32,
        toolbar.height.ceil() as i32,
        theme.toolbar_background,
    );
    // Hairline between chrome and page.
    surface.draw_rect(
        0,
        (toolbar.y + toolbar.height) as i32,
        chrome.window_width.ceil() as i32,
        metrics.page_padding_top.max(1.0) as i32,
        theme.chrome_border,
    );

    render_tab_strip(surface, app, fonts, theme, input);
    render_toolbar(surface, app, fonts, theme, input);
}

fn render_tab_strip(
    surface: &mut Surface,
    app: &BrowserAppState,
    fonts: &mut Fonts,
    theme: &BrowserTheme,
    input: ChromeInput,
) {
    let chrome = &app.chrome;
    let metrics = chrome.metrics;
    let count = app.tabs.len();
    let active_id = app.tabs.active_id();

    for (index, tab) in app.tabs.tabs().iter().enumerate() {
        let rect = chrome.tab_rect(index, count);
        let is_active = tab.id == active_id;
        let is_hovered = matches!(input.hover, Some(ChromeElement::Tab(id)) if id == tab.id)
            || matches!(input.hover, Some(ChromeElement::TabClose(id)) if id == tab.id);
        let background = if is_active {
            theme.active_tab_background
        } else if is_hovered {
            theme.tab_hover_background
        } else {
            theme.inactive_tab_background
        };
        surface.draw_rounded_rect_top(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            metrics.tab_radius,
            background,
        );

        // Title, ellipsized to the space left of the close button.
        let close = chrome.tab_close_rect(index, count);
        let title_x = rect.x + 12.0;
        let title_budget = (close.x - 8.0 - title_x).max(0.0);
        let title = fonts.ellipsize(tab.title(), 13.0, title_budget);
        let title_y = rect.y + (rect.height - fonts.line_height(13.0)) / 2.0;
        fonts.draw(surface, &title, title_x, title_y, 13.0, theme.tab_text);

        // Close button on the active or hovered tab.
        if is_active || is_hovered {
            if matches!(input.hover, Some(ChromeElement::TabClose(id)) if id == tab.id) {
                surface.draw_rounded_rect(
                    close.x,
                    close.y,
                    close.width,
                    close.height,
                    4.0,
                    theme.button_hover_background,
                );
            }
            icons::draw_close_icon(surface, close, theme.icon);
        }
    }

    // New-tab (+) button.
    let plus = chrome.new_tab_button(count);
    if input.hover == Some(ChromeElement::NewTabButton) {
        let background = if input.mouse_down {
            theme.button_active_background
        } else {
            theme.button_hover_background
        };
        surface.draw_rounded_rect(plus.x, plus.y, plus.width, plus.height, 6.0, background);
    }
    icons::draw_plus_icon(surface, plus, theme.icon);
}

fn render_toolbar(
    surface: &mut Surface,
    app: &BrowserAppState,
    fonts: &mut Fonts,
    theme: &BrowserTheme,
    input: ChromeInput,
) {
    let chrome = &app.chrome;
    let metrics = chrome.metrics;

    // Navigation buttons: (rect, element, icon, enabled).
    let buttons: [(
        Rect,
        ChromeElement,
        fn(&mut Surface, Rect, mocha_layout::Color),
        bool,
    ); 4] = [
        (
            chrome.back_button(),
            ChromeElement::BackButton,
            icons::draw_back_icon,
            app.can_go_back(),
        ),
        (
            chrome.forward_button(),
            ChromeElement::ForwardButton,
            icons::draw_forward_icon,
            app.can_go_forward(),
        ),
        (
            chrome.reload_button(),
            ChromeElement::ReloadButton,
            icons::draw_reload_icon,
            true,
        ),
        (
            chrome.home_button(),
            ChromeElement::HomeButton,
            icons::draw_home_icon,
            true,
        ),
    ];
    for (rect, element, draw_icon, enabled) in buttons {
        if enabled && input.hover == Some(element) {
            let background = if input.mouse_down {
                theme.button_active_background
            } else {
                theme.button_hover_background
            };
            // Circular hover backdrop (radius = half size).
            surface.draw_rounded_rect(
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                rect.width / 2.0,
                background,
            );
        }
        let color = if enabled {
            theme.icon
        } else {
            theme.icon_disabled
        };
        draw_icon(surface, rect, color);
    }

    // Address bar pill.
    let addr = chrome.address_bar();
    let focused = app.focus == BrowserFocus::AddressBar;
    surface.draw_pill(addr.x, addr.y, addr.width, addr.height, theme.address_bar_background);
    if focused {
        surface.draw_pill_outline(
            addr.x,
            addr.y,
            addr.width,
            addr.height,
            2.0,
            theme.address_bar_focused_border,
        );
    } else {
        surface.draw_pill_outline(
            addr.x,
            addr.y,
            addr.width,
            addr.height,
            1.0,
            theme.address_bar_border,
        );
    }

    let text_pad = metrics.address_bar_height / 2.0 + 2.0;
    let text_size = 14.0;
    let text_budget = (addr.width - text_pad * 2.0).max(0.0);
    let text_y = addr.y + (addr.height - fonts.line_height(text_size)) / 2.0;
    let draft = app.address_bar.draft_text.as_str();
    if draft.is_empty() && !focused {
        fonts.draw(
            surface,
            ADDRESS_PLACEHOLDER,
            addr.x + text_pad,
            text_y,
            text_size,
            theme.address_bar_placeholder,
        );
    } else {
        // While editing, keep the end (caret side) visible.
        let visible = tail_that_fits(fonts, draft, text_size, text_budget);
        let width = fonts.draw(
            surface,
            &visible,
            addr.x + text_pad,
            text_y,
            text_size,
            theme.address_bar_text,
        );
        if focused && input.caret_visible {
            let caret_x = addr.x + text_pad + width + 1.0;
            surface.draw_line(
                caret_x,
                addr.y + 8.0,
                caret_x,
                addr.y + addr.height - 8.0,
                1.0,
                theme.address_bar_text,
            );
        }
    }
}

/// The longest suffix of `text` that fits `max_width` (editing keeps the caret
/// end visible; the clipped head is irrelevant while typing).
fn tail_that_fits(fonts: &mut Fonts, text: &str, size: f32, max_width: f32) -> String {
    if fonts.measure(text, size) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let candidate: String = chars[start..].iter().collect();
        if fonts.measure(&candidate, size) <= max_width {
            return candidate;
        }
        start += 1;
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::rgb;
    use mocha_layout::Color;

    fn pack(color: Color) -> u32 {
        ((color.r as u32) << 16) | ((color.g as u32) << 8) | (color.b as u32)
    }

    fn app() -> BrowserAppState {
        BrowserAppState::start(1200, 800).unwrap()
    }

    fn render(app: &BrowserAppState, input: ChromeInput) -> Surface {
        let mut surface = Surface::new(1200, 800);
        let mut fonts = Fonts::load();
        let theme = BrowserTheme::default();
        render_browser(&mut surface, app, &mut fonts, &theme, input);
        surface
    }

    #[test]
    fn theme_colors_reach_the_chrome_bands() {
        let app = app();
        let theme = BrowserTheme::default();
        let surface = render(&app, ChromeInput::default());
        // Tab-strip background right of the tabs.
        assert_eq!(
            surface.pixel(1100, 8),
            Some(pack(theme.tab_strip_background))
        );
        // Toolbar background between home button and address bar.
        assert_eq!(surface.pixel(1195, 60), Some(pack(theme.toolbar_background)));
        // Active tab interior.
        let tab = app.chrome.tab_rect(0, 1);
        assert_eq!(
            surface.pixel((tab.x + tab.width / 2.0) as u32, (tab.y + 6.0) as u32),
            Some(pack(theme.active_tab_background))
        );
        // Address bar interior.
        let addr = app.chrome.address_bar();
        assert_eq!(
            surface.pixel((addr.x + addr.width / 2.0) as u32, (addr.y + 4.0) as u32),
            Some(pack(theme.address_bar_background))
        );
        // Hairline below the toolbar.
        let toolbar = app.chrome.toolbar();
        assert_eq!(
            surface.pixel(600, (toolbar.y + toolbar.height) as u32),
            Some(pack(theme.chrome_border))
        );
    }

    #[test]
    fn new_tab_view_fills_the_viewport_with_page_background() {
        let app = app();
        let theme = BrowserTheme::default();
        let surface = render(&app, ChromeInput::default());
        let viewport = app.chrome.page_viewport();
        // A corner of the viewport (outside the centered column) is page bg.
        assert_eq!(
            surface.pixel(20, (viewport.y + viewport.height - 20.0) as u32),
            Some(pack(theme.page_background))
        );
    }

    #[test]
    fn hover_changes_button_pixels() {
        let app = app();
        let plain = render(&app, ChromeInput::default());
        let hovered = render(
            &app,
            ChromeInput {
                hover: Some(ChromeElement::ReloadButton),
                ..ChromeInput::default()
            },
        );
        assert_ne!(plain.buffer(), hovered.buffer());
        let pressed = render(
            &app,
            ChromeInput {
                hover: Some(ChromeElement::ReloadButton),
                mouse_down: true,
                ..ChromeInput::default()
            },
        );
        assert_ne!(hovered.buffer(), pressed.buffer(), "pressed looks darker");
    }

    #[test]
    fn disabled_back_button_is_faded() {
        // The start app cannot go back: its back icon uses the disabled colour.
        let app = app();
        assert!(!app.can_go_back());
        let theme = BrowserTheme::default();
        let surface = render(&app, ChromeInput::default());
        let back = app.chrome.back_button();
        let mut found_disabled = false;
        for dy in 0..back.height as u32 {
            for dx in 0..back.width as u32 {
                if surface.pixel(back.x as u32 + dx, back.y as u32 + dy)
                    == Some(pack(theme.icon_disabled))
                {
                    found_disabled = true;
                }
            }
        }
        assert!(found_disabled, "faded icon pixels present");
    }

    #[test]
    fn empty_unfocused_address_bar_shows_placeholder() {
        let app = app();
        let empty = render(&app, ChromeInput::default());
        // Placeholder pixels exist inside the address bar (not plain white).
        let addr = app.chrome.address_bar();
        let mut non_white = 0;
        for dy in 8..(addr.height as u32 - 8) {
            for dx in 14..200 {
                let p = empty
                    .pixel(addr.x as u32 + dx, addr.y as u32 + dy)
                    .unwrap();
                if p != 0x00ff_ffff {
                    non_white += 1;
                }
            }
        }
        assert!(non_white > 10, "placeholder text drawn");
    }

    #[test]
    fn focused_address_bar_uses_focused_border_and_caret_blinks() {
        let mut app = app();
        let addr = app.chrome.address_bar();
        app.click(addr.x + 30.0, addr.y + addr.height / 2.0).unwrap();
        assert_eq!(app.focus, BrowserFocus::AddressBar);
        let theme = BrowserTheme::default();
        let caret_on = render(
            &app,
            ChromeInput {
                caret_visible: true,
                ..ChromeInput::default()
            },
        );
        let caret_off = render(&app, ChromeInput::default());
        // Focused border colour appears on the pill edge.
        let mut found_focus_border = false;
        for dx in 0..addr.width as u32 {
            if caret_on.pixel(addr.x as u32 + dx, addr.y as u32 + 1)
                == Some(pack(theme.address_bar_focused_border))
            {
                found_focus_border = true;
            }
        }
        assert!(found_focus_border);
        assert_ne!(caret_on.buffer(), caret_off.buffer(), "caret blinks");
    }

    #[test]
    fn loaded_page_renders_content_below_chrome() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/basic/index.html"
        );
        let app = BrowserAppState::load(path, 1200, 800).unwrap();
        assert!(app.tabs.active().internal_view().is_none());
        let surface = render(&app, ChromeInput::default());
        // Page text pixels exist below the chrome.
        let viewport = app.chrome.page_viewport();
        let mut non_white = 0;
        for dy in 0..60 {
            for dx in 0..400 {
                if surface.pixel(dx, viewport.y as u32 + dy) != Some(0x00ff_ffff) {
                    non_white += 1;
                }
            }
        }
        assert!(non_white > 0, "page content painted in the viewport");
    }

    #[test]
    fn long_tab_titles_are_ellipsized() {
        let mut fonts = Fonts::load();
        let long = "an-extremely-long-page-title-that-cannot-fit-in-a-tab.html";
        let fitted = fonts.ellipsize(long, 13.0, 150.0);
        assert!(fitted.chars().count() < long.chars().count());
        assert!(fonts.measure(&fitted, 13.0) <= 150.0);
    }

    #[test]
    fn tail_that_fits_keeps_the_end_of_long_drafts() {
        let mut fonts = Fonts::load();
        let long = format!("https://example.com/{}", "segment/".repeat(40));
        let tail = tail_that_fits(&mut fonts, &long, 14.0, 200.0);
        assert!(long.ends_with(&tail));
        assert!(fonts.measure(&tail, 14.0) <= 200.0);
    }

    #[test]
    fn error_view_renders_for_failed_initial_load() {
        let app = BrowserAppState::load_or_error_page("missing/nope.html", 1200, 800).unwrap();
        let surface = render(&app, ChromeInput::default());
        let viewport = app.chrome.page_viewport();
        // The error card paints non-background pixels in the viewport.
        let mut non_bg = 0;
        for dy in (0..viewport.height as u32).step_by(7) {
            for dx in (0..viewport.width as u32).step_by(7) {
                let p = surface.pixel(dx, viewport.y as u32 + dy).unwrap();
                if p != pack(rgb(0xff, 0xff, 0xff)) {
                    non_bg += 1;
                }
            }
        }
        assert!(non_bg > 20, "error card visible");
    }
}
