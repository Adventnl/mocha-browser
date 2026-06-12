//! Native browser views: the new-tab (home) page and the load-error page.
//!
//! These are drawn directly by the desktop shell into the page viewport —
//! no HTML, no parsing, no network — so they can look like a real browser's
//! start/error pages without growing the web engine. Layout is a simple
//! centered column with rounded cards; text comes from [`crate::text::Fonts`]
//! and colours from [`crate::theme::BrowserTheme`].

use mocha_layout::Color;
use mocha_raster::Surface;

use crate::chrome::Rect;
use crate::tab::InternalView;
use crate::text::Fonts;
use crate::theme::BrowserTheme;

// --- new-tab content ---------------------------------------------------------

pub const NEW_TAB_TITLE_TEXT: &str = "Mocha Browser";
pub const NEW_TAB_SUBTITLE_TEXT: &str = "Experimental browser engine";
pub const NEW_TAB_HINT_TEXT: &str = "Type a local path or URL in the address bar.";
pub const NEW_TAB_TRY_HEADER: &str = "Try:";
pub const NEW_TAB_EXAMPLES: [&str; 3] = [
    "examples/basic/index.html",
    "examples/styled/index.html",
    "https://example.com/",
];
pub const NEW_TAB_NOTE_TEXT: &str = "Note: many real websites will not render fully yet.";

// --- error-page content --------------------------------------------------------

pub const ERROR_TITLE_TEXT: &str = "Page could not be rendered";
/// Shown when the response arrived but the engine cannot handle the content.
pub const ERROR_ENGINE_EXPLANATION: &str =
    "Mocha loaded the response, but the current HTML engine does not support this page yet.";
/// Shown when the document never arrived (network/file errors).
pub const ERROR_LOAD_EXPLANATION: &str = "Mocha could not load this page.";
pub const ERROR_HINT_TEXT: &str = "Try a local example: examples/basic/index.html";

/// Render `view` into the page viewport region of `surface`.
pub fn render_view(
    view: &InternalView,
    surface: &mut Surface,
    fonts: &mut Fonts,
    theme: &BrowserTheme,
    viewport: Rect,
) {
    match view {
        InternalView::NewTab => render_new_tab(surface, fonts, theme, viewport),
        InternalView::LoadError { input, message } => {
            render_error(surface, fonts, theme, viewport, input, message)
        }
    }
}

fn render_new_tab(surface: &mut Surface, fonts: &mut Fonts, theme: &BrowserTheme, viewport: Rect) {
    fill_viewport(surface, theme.page_background, viewport);
    let center_x = viewport.x + viewport.width / 2.0;

    // Title + subtitle, centered, starting ~22% down the viewport.
    let mut y = viewport.y + (viewport.height * 0.22).max(24.0);
    y += draw_centered(surface, fonts, NEW_TAB_TITLE_TEXT, center_x, y, 36.0, theme.tab_text);
    y += 6.0;
    y += draw_centered(
        surface,
        fonts,
        NEW_TAB_SUBTITLE_TEXT,
        center_x,
        y,
        16.0,
        theme.text_secondary,
    );
    y += 28.0;

    // The hint card: hint line, "Try:" header, and example targets.
    let card_width = (viewport.width - 48.0).clamp(40.0, 560.0);
    let pad = 20.0;
    let hint_height = fonts.line_height(15.0);
    let header_height = fonts.line_height(13.0);
    let example_height = fonts.line_height(14.0);
    let card_height = pad
        + hint_height
        + 14.0
        + header_height
        + 6.0
        + NEW_TAB_EXAMPLES.len() as f32 * (example_height + 6.0)
        + pad
        - 6.0;
    let card_x = center_x - card_width / 2.0;
    surface.draw_rounded_rect(card_x, y, card_width, card_height, 12.0, theme.card_background);
    surface.draw_rounded_rect_outline(card_x, y, card_width, card_height, 12.0, 1.0, theme.card_border);

    let text_x = card_x + pad;
    let mut card_y = y + pad;
    fonts.draw(surface, NEW_TAB_HINT_TEXT, text_x, card_y, 15.0, theme.tab_text);
    card_y += hint_height + 14.0;
    fonts.draw(
        surface,
        NEW_TAB_TRY_HEADER,
        text_x,
        card_y,
        13.0,
        theme.text_secondary,
    );
    card_y += header_height + 6.0;
    for example in NEW_TAB_EXAMPLES {
        fonts.draw(surface, example, text_x + 12.0, card_y, 14.0, theme.tab_text);
        card_y += example_height + 6.0;
    }

    // Small limitations note under the card.
    let note_y = y + card_height + 18.0;
    draw_centered(
        surface,
        fonts,
        NEW_TAB_NOTE_TEXT,
        center_x,
        note_y,
        13.0,
        theme.text_secondary,
    );
}

fn render_error(
    surface: &mut Surface,
    fonts: &mut Fonts,
    theme: &BrowserTheme,
    viewport: Rect,
    input: &str,
    message: &str,
) {
    fill_viewport(surface, theme.page_background, viewport);
    let center_x = viewport.x + viewport.width / 2.0;

    let card_width = (viewport.width - 48.0).clamp(40.0, 640.0);
    let card_x = center_x - card_width / 2.0;
    let pad = 24.0;
    let inner_width = card_width - pad * 2.0;

    // Details (monospace-look, via the debug pixel font) wrap to the card.
    let detail_lines: Vec<String> = [format!("Reason: {message}"), format!("URL: {input}")]
        .iter()
        .flat_map(|line| wrap_debug_text(line, inner_width - 24.0, 1))
        .collect();
    let detail_line_height = 12.0;
    let details_height = detail_lines.len() as f32 * detail_line_height + 20.0;

    let explanation = explanation_for(message);
    let explanation_lines = wrap_text(fonts, explanation, 14.0, inner_width);

    let title_height = fonts.line_height(22.0);
    let explanation_height = explanation_lines.len() as f32 * fonts.line_height(14.0);
    let hint_height = fonts.line_height(13.0) + 16.0;
    let card_height = pad
        + title_height
        + 14.0
        + details_height
        + 16.0
        + explanation_height
        + 18.0
        + hint_height
        + pad;

    let card_y = (viewport.y + (viewport.height - card_height) * 0.30).max(viewport.y + 16.0);
    surface.draw_rounded_rect(card_x, card_y, card_width, card_height, 12.0, theme.card_background);
    surface.draw_rounded_rect_outline(
        card_x,
        card_y,
        card_width,
        card_height,
        12.0,
        1.0,
        theme.card_border,
    );

    let text_x = card_x + pad;
    let mut y = card_y + pad;
    fonts.draw(surface, ERROR_TITLE_TEXT, text_x, y, 22.0, theme.error_accent);
    y += title_height + 14.0;

    // Details panel.
    let panel = Rect::new(text_x, y, inner_width, details_height);
    surface.draw_rounded_rect(
        panel.x,
        panel.y,
        panel.width,
        panel.height,
        6.0,
        crate::theme::rgb(0xf2, 0xf3, 0xf4),
    );
    let mut detail_y = panel.y + 10.0;
    for line in &detail_lines {
        surface.draw_text_at(
            line,
            (panel.x + 12.0) as i32,
            detail_y as i32,
            1,
            theme.tab_text,
        );
        detail_y += detail_line_height;
    }
    y += details_height + 16.0;

    for line in &explanation_lines {
        fonts.draw(surface, line, text_x, y, 14.0, theme.text_secondary);
        y += fonts.line_height(14.0);
    }
    y += 18.0;

    // A button-like hint pill (informational, not clickable).
    let hint_width = fonts.measure(ERROR_HINT_TEXT, 13.0) + 32.0;
    let hint_rect = Rect::new(text_x, y, hint_width.min(inner_width), hint_height);
    surface.draw_pill(
        hint_rect.x,
        hint_rect.y,
        hint_rect.width,
        hint_rect.height,
        crate::theme::rgb(0xee, 0xf1, 0xf6),
    );
    fonts.draw(
        surface,
        ERROR_HINT_TEXT,
        hint_rect.x + 16.0,
        hint_rect.y + 8.0,
        13.0,
        theme.tab_text,
    );
}

/// Pick the honest explanation: a transport failure never produced a response,
/// while engine-side failures mean the response arrived but cannot render.
fn explanation_for(message: &str) -> &'static str {
    if message.starts_with("network error") || message.starts_with("io error") {
        ERROR_LOAD_EXPLANATION
    } else {
        ERROR_ENGINE_EXPLANATION
    }
}

fn fill_viewport(surface: &mut Surface, color: Color, viewport: Rect) {
    surface.draw_rect(
        viewport.x as i32,
        viewport.y as i32,
        viewport.width.ceil() as i32,
        viewport.height.ceil() as i32,
        color,
    );
}

/// Draw `text` horizontally centered on `center_x`; returns the line height.
fn draw_centered(
    surface: &mut Surface,
    fonts: &mut Fonts,
    text: &str,
    center_x: f32,
    y: f32,
    size: f32,
    color: Color,
) -> f32 {
    let width = fonts.measure(text, size);
    fonts.draw(surface, text, center_x - width / 2.0, y, size, color);
    fonts.line_height(size)
}

/// Greedy word-wrap for sans text; words longer than the budget are hard-split.
fn wrap_text(fonts: &mut Fonts, text: &str, size: f32, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if fonts.measure(&candidate, size) <= max_width || current.is_empty() {
            current = candidate;
            // A single word can still overflow: hard-split it into the longest
            // prefixes that fit (always at least one char, so this terminates).
            while fonts.measure(&current, size) > max_width && current.chars().count() > 1 {
                let chars: Vec<char> = current.chars().collect();
                let mut take = 1;
                while take < chars.len() - 1 {
                    let prefix: String = chars[..take + 1].iter().collect();
                    if fonts.measure(&prefix, size) > max_width {
                        break;
                    }
                    take += 1;
                }
                lines.push(chars[..take].iter().collect());
                current = chars[take..].iter().collect();
            }
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Word-wrap for the fixed-advance debug font at `scale`.
fn wrap_debug_text(text: &str, max_width: f32, scale: i32) -> Vec<String> {
    let advance = (mocha_raster::GLYPH_ADVANCE as i32 * scale) as f32;
    let per_line = ((max_width / advance) as usize).max(8);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if candidate_len <= per_line {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        } else {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            // Hard-split words longer than a line.
            let mut rest: Vec<char> = word.chars().collect();
            while rest.len() > per_line {
                lines.push(rest.drain(..per_line).collect());
            }
            current = rest.into_iter().collect();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::new(0.0, 85.0, 1200.0, 715.0)
    }

    fn render(view: &InternalView) -> Vec<u32> {
        let mut surface = Surface::new(1200, 800);
        let mut fonts = Fonts::load();
        let theme = BrowserTheme::default();
        render_view(view, &mut surface, &mut fonts, &theme, viewport());
        surface.buffer().to_vec()
    }

    fn non_white(buffer: &[u32]) -> usize {
        buffer.iter().filter(|&&p| p != 0x00ff_ffff).count()
    }

    #[test]
    fn new_tab_view_renders_content_without_network() {
        // Pure pixel drawing: no loader, no URL, just a surface.
        let pixels = render(&InternalView::NewTab);
        assert!(non_white(&pixels) > 500, "title, card, and text painted");
    }

    #[test]
    fn new_tab_content_strings_are_present() {
        assert_eq!(NEW_TAB_TITLE_TEXT, "Mocha Browser");
        assert!(NEW_TAB_SUBTITLE_TEXT.contains("Experimental"));
        assert!(NEW_TAB_HINT_TEXT.contains("address bar"));
        assert!(NEW_TAB_EXAMPLES.contains(&"https://example.com/"));
        assert!(NEW_TAB_NOTE_TEXT.contains("not render fully"));
    }

    #[test]
    fn error_view_renders_and_message_affects_pixels() {
        let a = render(&InternalView::LoadError {
            input: "https://example.com/".to_string(),
            message: "unsupported feature: tag <head> is not supported".to_string(),
        });
        let b = render(&InternalView::LoadError {
            input: "https://example.com/".to_string(),
            message: "network error: cannot connect".to_string(),
        });
        assert!(non_white(&a) > 500);
        assert_ne!(a, b, "the real error message is drawn, not hidden");
    }

    #[test]
    fn error_view_includes_the_url() {
        let a = render(&InternalView::LoadError {
            input: "https://example.com/".to_string(),
            message: "parse error: x".to_string(),
        });
        let b = render(&InternalView::LoadError {
            input: "https://other.example/".to_string(),
            message: "parse error: x".to_string(),
        });
        assert_ne!(a, b, "the attempted URL is drawn");
    }

    #[test]
    fn explanation_distinguishes_load_from_render_failures() {
        assert_eq!(
            explanation_for("network error: cannot connect"),
            ERROR_LOAD_EXPLANATION
        );
        assert_eq!(
            explanation_for("io error: missing"),
            ERROR_LOAD_EXPLANATION
        );
        assert_eq!(
            explanation_for("unsupported feature: tag <head>"),
            ERROR_ENGINE_EXPLANATION
        );
    }

    #[test]
    fn wrap_text_fits_budget_and_splits_long_words() {
        let mut fonts = Fonts::load();
        let lines = wrap_text(
            &mut fonts,
            "a reasonably long explanation that should wrap onto several lines",
            14.0,
            120.0,
        );
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(fonts.measure(line, 14.0) <= 120.0 + 0.01, "line fits: {line}");
        }
        // One enormous unbroken word still terminates and splits.
        let monster = "x".repeat(300);
        let split = wrap_text(&mut fonts, &monster, 14.0, 100.0);
        assert!(split.len() > 1);
    }

    #[test]
    fn wrap_debug_text_hard_splits_long_urls() {
        let url = format!("https://example.com/{}", "a".repeat(200));
        let lines = wrap_debug_text(&url, 300.0, 1);
        assert!(lines.len() > 1);
        let per_line = (300.0 / mocha_raster::GLYPH_ADVANCE as f32) as usize;
        for line in &lines {
            assert!(line.chars().count() <= per_line);
        }
    }

    #[test]
    fn long_error_messages_do_not_panic() {
        let pixels = render(&InternalView::LoadError {
            input: format!("https://example.com/{}", "path/".repeat(100)),
            message: format!("network error: {}", "very long detail ".repeat(50)),
        });
        assert!(non_white(&pixels) > 0);
    }
}
