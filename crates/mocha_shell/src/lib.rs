//! The Mocha Browser command-line shell library.
//!
//! This crate wires the rendering pipeline together. Loading now goes through
//! `mocha_net` (file/http) and `mocha_nav` (history); the shell only orchestrates
//! and renders:
//!
//! ```text
//! input -> mocha_url -> mocha_nav/mocha_net (load) -> content-type check
//!       -> UTF-8 decode -> mocha_html -> mocha_style -> mocha_layout -> mocha_paint
//! ```
//!
//! Entry points: [`render_request`] (full load+render, returns text to print),
//! [`run_file`]/[`run_html`] (display list) and [`dump_layout_file`]/
//! [`dump_layout_html`] (layout dump). The shell does **not** open a window or run
//! an interactive UI. `https://` is not implemented and fails clearly; subresource
//! loading (external CSS, images, scripts) is not implemented.

use mocha_error::{MochaError, MochaResult};
use mocha_layout::{
    build_layout_tree, format_layout_tree, LayoutBox, LayoutViewport, DEFAULT_VIEWPORT_WIDTH,
};
use mocha_nav::NavigationController;
use mocha_net::{DefaultLoader, ResourceResponse, ResourceType};
use mocha_url::Url;

pub use mocha_layout::NodeId;
pub use mocha_paint::{format_display_list, DisplayCommand};

/// Options controlling a shell run.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Print the layout tree instead of the display list.
    pub dump_layout: bool,
    /// Bypass the in-memory cache when loading.
    pub no_cache: bool,
    /// Print response metadata before the output.
    pub show_headers: bool,
}

/// Load `input` and render it, returning the text the CLI should print.
pub fn render_request(input: &str, options: RunOptions) -> MochaResult<String> {
    let response = load_document(input, options)?;
    let layout = render_to_layout(&response)?;

    let mut output = String::new();
    if options.show_headers {
        output.push_str(&format_headers(&response));
        output.push('\n');
    }
    if options.dump_layout {
        output.push_str(&format_layout_tree(&layout));
    } else {
        output.push_str(&format_display_list(&mocha_paint::build_display_list(
            &layout,
        )?));
    }
    Ok(output)
}

/// Load a location (file or http) and produce its display list.
pub fn run_file(input: &str) -> MochaResult<Vec<DisplayCommand>> {
    let response = load_document(input, RunOptions::default())?;
    mocha_paint::build_display_list(&render_to_layout(&response)?)
}

/// Load a location (file or http) and produce its formatted layout-tree dump.
pub fn dump_layout_file(input: &str) -> MochaResult<String> {
    let response = load_document(input, RunOptions::default())?;
    Ok(format_layout_tree(&render_to_layout(&response)?))
}

/// Load a location and return the DOM node at viewport point `(x, y)`.
pub fn hit_test_file(input: &str, x: f32, y: f32) -> MochaResult<Option<NodeId>> {
    let response = load_document(input, RunOptions::default())?;
    let layout = render_to_layout(&response)?;
    Ok(mocha_layout::hit_test(&layout, x, y))
}

/// Render an in-memory HTML string to a display list (no loading).
pub fn run_html(input: &str) -> MochaResult<Vec<DisplayCommand>> {
    mocha_paint::build_display_list(&layout_html(input)?)
}

/// Render an in-memory HTML string to a layout-tree dump (no loading).
pub fn dump_layout_html(input: &str) -> MochaResult<String> {
    Ok(format_layout_tree(&layout_html(input)?))
}

/// Parse the location and load it through the navigation/loader pipeline.
fn load_document(input: &str, options: RunOptions) -> MochaResult<ResourceResponse> {
    let url = Url::parse(input)?;
    let mut controller = NavigationController::new(DefaultLoader::new());
    if options.no_cache {
        controller.navigate_no_cache(url)
    } else {
        controller.navigate(url)
    }
}

/// Validate the content type, decode the body as UTF-8, and run the engine.
fn render_to_layout(response: &ResourceResponse) -> MochaResult<LayoutBox> {
    if response.resource_type() != ResourceType::Html {
        return Err(MochaError::UnsupportedFeature(
            "only text/html documents can be rendered in Milestone 4".to_string(),
        ));
    }
    let html = std::str::from_utf8(&response.body).map_err(|_| {
        MochaError::UnsupportedFeature(
            "character encodings other than UTF-8 are not supported in Milestone 4".to_string(),
        )
    })?;
    layout_html(html)
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

fn format_headers(response: &ResourceResponse) -> String {
    let mut lines = vec![format!("url: {}", response.final_url.normalized())];
    if let Some(status) = response.status {
        lines.push(format!("status: {status}"));
    }
    if let Some(content_type) = &response.content_type {
        lines.push(format!("content-type: {content_type}"));
    }
    lines.push(format!("from-cache: {}", response.from_cache));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_net::test_server::{Reply, TestServer};

    #[test]
    fn https_returns_unsupported_feature() {
        // https is rejected before any network access.
        let error = run_file("https://example.com/index.html").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn missing_file_returns_clear_error() {
        let error = run_file("definitely/does/not/exist.html").unwrap_err();
        assert!(matches!(error, MochaError::Io(_)));
    }

    #[test]
    fn empty_path_returns_invalid_url() {
        let error = run_file("").unwrap_err();
        assert!(matches!(error, MochaError::InvalidUrl(_)));
    }

    #[test]
    fn http_html_renders() {
        let server = TestServer::start(vec![(
            "/index.html".to_string(),
            Reply::Html("<html><body><p>Hi</p></body></html>".to_string()),
        )]);
        let commands = run_file(&server.url("/index.html")).unwrap();
        assert!(commands
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "Hi")));
    }

    #[test]
    fn http_text_plain_is_not_rendered() {
        let server = TestServer::start(vec![(
            "/note.txt".to_string(),
            Reply::Text("hello".to_string()),
        )]);
        let error = run_file(&server.url("/note.txt")).unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn dump_layout_works_for_http() {
        let server = TestServer::start(vec![(
            "/index.html".to_string(),
            Reply::Html("<html><body><p>Hello world</p></body></html>".to_string()),
        )]);
        let dump = dump_layout_file(&server.url("/index.html")).unwrap();
        assert!(dump.contains("LineBox"));
        assert!(dump.contains("TextRun"));
    }

    #[test]
    fn show_headers_includes_status_and_content_type() {
        let server = TestServer::start(vec![(
            "/index.html".to_string(),
            Reply::Html("<html><body><p>Hi</p></body></html>".to_string()),
        )]);
        let out = render_request(
            &server.url("/index.html"),
            RunOptions {
                show_headers: true,
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert!(out.contains("status: 200"));
        assert!(out.contains("content-type: text/html"));
    }

    #[test]
    fn non_utf8_body_is_rejected() {
        // Serve invalid UTF-8 bytes from a temp .html file.
        let path = std::env::temp_dir().join("mocha_non_utf8.html");
        std::fs::write(&path, [0x3c, 0xff, 0x3e]).unwrap(); // "<\xff>"
        let error = run_file(path.to_str().unwrap()).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn styled_html_produces_colored_text() {
        let html = "<html><body><style>p { color: red; }</style><p>Hi</p></body></html>";
        let commands = run_html(html).unwrap();
        assert!(commands.iter().any(|c| matches!(c,
            DisplayCommand::DrawText { text, color, .. }
                if text == "Hi" && color.r == 255 && color.g == 0 && color.b == 0)));
    }

    #[test]
    fn style_tag_css_is_not_painted_as_text() {
        let html = "<html><body><style>p { color: red; }</style><p>Hi</p></body></html>";
        let commands = run_html(html).unwrap();
        assert!(!commands
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text.contains("color"))));
    }

    #[test]
    fn unsupported_css_property_fails_clearly() {
        let html = "<html><body><style>p { float: left; }</style><p>Hi</p></body></html>";
        assert!(matches!(
            run_html(html).unwrap_err(),
            MochaError::UnsupportedFeature(_)
        ));
    }
}
