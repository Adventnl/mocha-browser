//! The Mocha Browser command-line shell library.
//!
//! This crate wires the Milestone 2 pipeline together:
//!
//! ```text
//! path -> mocha_url -> read file -> mocha_html -> collect <style>
//!      -> mocha_style (computed style) -> mocha_layout -> mocha_paint
//! ```
//!
//! It exposes testable entry points, [`run_file`] and [`run_html`], which the
//! binary ([`main`](../main.rs)) calls and prints. The shell does **not** open a
//! window, perform any networking, or run an interactive UI.

use mocha_error::{MochaError, MochaResult};
use mocha_layout::{build_layout_tree, LayoutViewport, DEFAULT_VIEWPORT_WIDTH};
use mocha_url::{Scheme, Url};

pub use mocha_paint::{format_display_list, DisplayCommand};

/// Load a local HTML file and produce its display list.
///
/// Parses the location, rejects non-`file` schemes (networking is out of scope),
/// reads the file, and runs [`run_html`] on its contents.
pub fn run_file(path: &str) -> MochaResult<Vec<DisplayCommand>> {
    let url = Url::parse(path)?;
    match url.scheme {
        Scheme::File => {}
        Scheme::Http | Scheme::Https => {
            return Err(MochaError::UnsupportedFeature(
                "network loading is not available in Milestone 2".to_string(),
            ));
        }
    }

    let source = std::fs::read_to_string(&url.path)
        .map_err(|error| MochaError::Io(format!("cannot read {}: {error}", url.path)))?;

    run_html(&source)
}

/// Run the full HTML + CSS pipeline on an in-memory HTML string.
///
/// Parses HTML into a DOM, collects and parses `<style>` stylesheets, computes
/// styles (UA defaults → author rules → inline), lays the styled tree out at the
/// default viewport width, and builds a display list.
pub fn run_html(input: &str) -> MochaResult<Vec<DisplayCommand>> {
    let document = mocha_html::parse_html(input)?;
    let stylesheets = mocha_style::collect_stylesheets(&document)?;
    let styled = mocha_style::build_style_tree(&document, &stylesheets)?;
    let viewport = LayoutViewport {
        width: DEFAULT_VIEWPORT_WIDTH,
        ..LayoutViewport::default()
    };
    let layout = build_layout_tree(&styled, viewport)?;
    mocha_paint::build_display_list(&layout)
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
}
