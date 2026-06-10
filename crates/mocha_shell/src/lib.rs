//! The Mocha Browser command-line shell library.
//!
//! This crate wires the rendering pipeline together:
//!
//! ```text
//! path -> mocha_url -> read file -> mocha_html -> collect <style>
//!      -> mocha_style (computed style) -> mocha_layout -> mocha_paint
//! ```
//!
//! It exposes testable entry points — [`run_file`]/[`run_html`] (display list)
//! and [`dump_layout_file`]/[`dump_layout_html`] (formatted layout tree) — which
//! the binary ([`main`](../main.rs)) calls and prints. The shell does **not**
//! open a window, perform any networking, or run an interactive UI.

use mocha_error::{MochaError, MochaResult};
use mocha_layout::{
    build_layout_tree, format_layout_tree, LayoutBox, LayoutViewport, DEFAULT_VIEWPORT_WIDTH,
};
use mocha_url::{Scheme, Url};

pub use mocha_paint::{format_display_list, DisplayCommand};

/// Load a local HTML file and produce its display list.
///
/// Parses the location, rejects non-`file` schemes (networking is out of scope),
/// reads the file, and runs [`run_html`] on its contents.
pub fn run_file(path: &str) -> MochaResult<Vec<DisplayCommand>> {
    run_html(&read_file(path)?)
}

/// Load a local HTML file and produce its formatted layout-tree dump.
pub fn dump_layout_file(path: &str) -> MochaResult<String> {
    dump_layout_html(&read_file(path)?)
}

/// Run the full HTML + CSS pipeline on an in-memory HTML string, producing a
/// display list.
///
/// Parses HTML into a DOM, collects and parses `<style>` stylesheets, computes
/// styles (UA defaults → author rules → inline), lays the styled tree out at the
/// default viewport width, and builds a display list.
pub fn run_html(input: &str) -> MochaResult<Vec<DisplayCommand>> {
    mocha_paint::build_display_list(&layout_html(input)?)
}

/// Run the pipeline up to layout and return the formatted layout tree.
pub fn dump_layout_html(input: &str) -> MochaResult<String> {
    Ok(format_layout_tree(&layout_html(input)?))
}

/// Parse a location and read the referenced local file, rejecting non-`file`
/// schemes (networking is out of scope).
fn read_file(path: &str) -> MochaResult<String> {
    let url = Url::parse(path)?;
    match url.scheme {
        Scheme::File => {}
        Scheme::Http | Scheme::Https => {
            return Err(MochaError::UnsupportedFeature(
                "network loading is not available in Milestone 3".to_string(),
            ));
        }
    }
    std::fs::read_to_string(&url.path)
        .map_err(|error| MochaError::Io(format!("cannot read {}: {error}", url.path)))
}

/// Parse HTML, compute style, and build the layout tree.
fn layout_html(input: &str) -> MochaResult<LayoutBox> {
    let document = mocha_html::parse_html(input)?;
    let stylesheets = mocha_style::collect_stylesheets(&document)?;
    let styled = mocha_style::build_style_tree(&document, &stylesheets)?;
    let viewport = LayoutViewport {
        width: DEFAULT_VIEWPORT_WIDTH,
        ..LayoutViewport::default()
    };
    build_layout_tree(&styled, viewport)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_url_returns_unsupported_feature() {
        let error = run_file("http://example.com/index.html").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn missing_file_returns_clear_error() {
        let error = run_file("definitely/does/not/exist.html").unwrap_err();
        match error {
            MochaError::Io(message) => assert!(message.contains("cannot read")),
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn empty_path_returns_invalid_url() {
        let error = run_file("").unwrap_err();
        assert!(matches!(error, MochaError::InvalidUrl(_)));
    }

    #[test]
    fn styled_html_produces_colored_text() {
        let html = "<html><body><style>p { color: red; }</style><p>Hi</p></body></html>";
        let commands = run_html(html).unwrap();
        let red = commands.iter().any(|c| {
            matches!(c, DisplayCommand::DrawText { text, color, .. }
                if text == "Hi" && color.r == 255 && color.g == 0 && color.b == 0)
        });
        assert!(red, "expected red 'Hi' text, got {commands:?}");
    }

    #[test]
    fn style_tag_css_is_not_painted_as_text() {
        let html = "<html><body><style>p { color: red; }</style><p>Hi</p></body></html>";
        let commands = run_html(html).unwrap();
        let leaked = commands
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text.contains("color")));
        assert!(!leaked, "CSS text should not be painted, got {commands:?}");
    }

    #[test]
    fn unsupported_css_property_fails_clearly() {
        let html = "<html><body><style>p { float: left; }</style><p>Hi</p></body></html>";
        let error = run_html(html).unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn dump_layout_html_includes_line_boxes_and_text_runs() {
        let html = "<html><body><p>Hello world</p></body></html>";
        let dump = dump_layout_html(html).unwrap();
        assert!(dump.contains("Block"));
        assert!(dump.contains("LineBox"));
        assert!(dump.contains("TextRun"));
    }

    #[test]
    fn inline_span_shares_line_and_keeps_its_color() {
        let html = "<html><body><p>Hello \
                    <span style=\"color: red;\">red</span> world</p></body></html>";
        let commands = run_html(html).unwrap();
        let texts: Vec<(String, f32, mocha_layout::Color)> = commands
            .iter()
            .filter_map(|c| match c {
                DisplayCommand::DrawText { text, y, color, .. } => Some((text.clone(), *y, *color)),
                _ => None,
            })
            .collect();

        let drawn: Vec<&str> = texts.iter().map(|(t, _, _)| t.as_str()).collect();
        assert_eq!(drawn, vec!["Hello", "red", "world"]);
        // All three share the same line (same y).
        assert!(texts.iter().all(|(_, y, _)| *y == texts[0].1));
        // Only "red" is red.
        assert_eq!(texts[1].2, mocha_layout::Color::rgb(255, 0, 0));
        assert_eq!(texts[0].2, mocha_layout::Color::BLACK);
    }

    #[test]
    fn angle_bracket_in_style_css_comment_flows_through_pipeline() {
        // Raw-text `<style>` keeps the `<` inside the CSS comment; the CSS still
        // parses (comments are ignored) and the `<` is never painted.
        let html = "<html><body><style>/* <not-a-tag> */ p { color: red; }</style>\
                    <p>Hi</p></body></html>";
        let commands = run_html(html).unwrap();
        let red = commands.iter().any(|c| {
            matches!(c, DisplayCommand::DrawText { text, color, .. }
                if text == "Hi" && color.r == 255 && color.g == 0 && color.b == 0)
        });
        assert!(red, "expected red 'Hi', got {commands:?}");
        let leaked = commands.iter().any(
            |c| matches!(c, DisplayCommand::DrawText { text, .. } if text.contains("not-a-tag")),
        );
        assert!(!leaked, "CSS comment text must not be painted");
    }
}
