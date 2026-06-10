//! The Mocha Browser command-line shell library.
//!
//! This crate wires the rendering pipeline together. Loading now goes through
//! `mocha_net` (file/http) and `mocha_nav` (history); the shell only orchestrates
//! and renders:
//!
//! ```text
//! input -> mocha_url -> mocha_nav/mocha_net (load) -> content-type check
//!       -> UTF-8 decode -> mocha_html -> inline <script> execution (mocha_js_dom)
//!       -> subresources: external <link> CSS + <img> images (mocha_resources/mocha_image)
//!       -> mocha_style -> mocha_layout -> mocha_paint
//! ```
//!
//! Entry points: [`render_request`] (full load+render, returns text to print),
//! [`run_file`]/[`run_html`] (display list) and [`dump_layout_file`]/
//! [`dump_layout_html`] (layout dump). The shell does **not** open a window or run
//! an interactive UI. `https://` is not implemented and fails clearly. Inline
//! `<script>` runs once before style/layout (Milestone 7, coarse invalidation);
//! external `<link rel="stylesheet">` CSS (Milestone 8) and `<img>` images
//! (Milestone 9) are loaded against the document base URL. External `<script src>`,
//! CSS `url(...)`, and in-memory subresources (no base URL) remain unsupported.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mocha_dom::{Document, NodeId as DomNodeId};
use mocha_error::{MochaError, MochaResult};
use mocha_image::DecodedImage;
use mocha_layout::{
    build_layout_tree, format_layout_tree, LayoutBox, LayoutViewport, DEFAULT_VIEWPORT_WIDTH,
};
use mocha_nav::NavigationController;
use mocha_net::{DefaultLoader, ResourceResponse, ResourceType};
use mocha_style::{ReplacedBox, StyledNode};
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

/// Evaluate a standalone JavaScript snippet and return its captured console
/// output followed by the result value (omitted when `undefined`).
///
/// This does **not** load a document or touch the DOM — JavaScript is not wired
/// into HTML/`<script>` in Milestone 6.
pub fn eval_js(source: &str) -> MochaResult<String> {
    let mut runtime = mocha_js::JsRuntime::new();
    let result = runtime.eval(source)?;
    let mut lines = runtime.take_console_output();
    if !matches!(result, mocha_js::JsValue::Undefined) {
        lines.push(result.stringify());
    }
    Ok(lines.join("\n"))
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
    layout_document(html, Some(&response.final_url))
}

/// Render in-memory HTML with no base URL (external subresources unsupported).
fn layout_html(input: &str) -> MochaResult<LayoutBox> {
    layout_document(input, None)
}

/// Parse HTML, execute inline scripts, collect stylesheets (inline `<style>` plus
/// external `<link rel="stylesheet">` resolved against `base`), compute style, and
/// build the layout tree. With no `base` (in-memory rendering) external
/// stylesheets are unsupported. Subresources are collected once, after scripts run.
fn layout_document(input: &str, base: Option<&Url>) -> MochaResult<LayoutBox> {
    let document = run_document_scripts(mocha_html::parse_html(input)?)?;
    let stylesheets = match base {
        Some(base) => {
            let mut loader = mocha_net::DefaultLoader::new();
            mocha_resources::collect_document_stylesheets(&document, base, &mut loader)?
        }
        None => mocha_resources::collect_inline_stylesheets(&document)?,
    };
    let images = load_document_images(&document, base)?;
    let mut styled = mocha_style::build_style_tree(&document, &stylesheets)?;
    attach_images(&mut styled, &document, &images);
    let viewport = LayoutViewport {
        width: DEFAULT_VIEWPORT_WIDTH,
        ..LayoutViewport::default()
    };
    build_layout_tree(&styled, viewport)
}

/// Decoded images for one document, keyed by the `<img>`'s DOM node id.
#[derive(Default)]
struct ImageStore {
    by_node: HashMap<DomNodeId, usize>,
    decoded: Vec<DecodedImage>,
}

/// Discover, resolve, load, and decode every `<img>` in the document. With no
/// `base` (in-memory rendering) any `<img>` is unsupported. A missing `src`,
/// failed load, wrong content type, or decode failure is a clear error.
fn load_document_images(document: &Document, base: Option<&Url>) -> MochaResult<ImageStore> {
    let images = mocha_resources::discover_images(document)?;
    if images.is_empty() {
        return Ok(ImageStore::default());
    }
    let base = base.ok_or_else(|| {
        MochaError::UnsupportedFeature(
            "images require a document base URL (load via file/http, not in-memory HTML)"
                .to_string(),
        )
    })?;
    let mut loader = mocha_net::DefaultLoader::new();
    let mut store = ImageStore::default();
    for (node, src) in images {
        let decoded = mocha_resources::load_image(&src, base, &mut loader)?;
        let id = store.decoded.len();
        store.decoded.push(decoded);
        store.by_node.insert(node, id);
    }
    Ok(store)
}

/// Attach decoded-image boxes to the matching `<img>` styled nodes, resolving each
/// image's final size from CSS, then `width`/`height` attributes, then intrinsic
/// dimensions (preserving aspect ratio when only one dimension is specified).
fn attach_images(styled: &mut StyledNode, document: &Document, store: &ImageStore) {
    if let Some(&image_id) = store.by_node.get(&styled.node_id) {
        let decoded = store.decoded[image_id];
        let (width, height) = resolve_image_size(styled, document, &decoded);
        styled.replaced = Some(ReplacedBox {
            image_id,
            width,
            height,
        });
    }
    for child in &mut styled.children {
        attach_images(child, document, store);
    }
}

fn resolve_image_size(
    styled: &StyledNode,
    document: &Document,
    decoded: &DecodedImage,
) -> (f32, f32) {
    let intrinsic_w = decoded.width as f32;
    let intrinsic_h = decoded.height as f32;
    let attr = |name: &str| -> Option<f32> {
        document
            .get_attribute(styled.node_id, name)
            .ok()
            .flatten()
            .and_then(|value| value.trim().parse::<f32>().ok())
            .filter(|value| *value >= 0.0)
    };
    // CSS wins over attributes, which win over the intrinsic size.
    let spec_w = styled.style.width.or_else(|| attr("width"));
    let spec_h = styled.style.height.or_else(|| attr("height"));
    match (spec_w, spec_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) if intrinsic_w > 0.0 => (w, w * intrinsic_h / intrinsic_w),
        (Some(w), None) => (w, intrinsic_h),
        (None, Some(h)) if intrinsic_h > 0.0 => (h * intrinsic_w / intrinsic_h, h),
        (None, Some(h)) => (intrinsic_w, h),
        (None, None) => (intrinsic_w, intrinsic_h),
    }
}

/// Run every inline `<script>` in document order, then any pending zero-delay
/// timers, returning the mutated document. Milestone 7 uses coarse invalidation:
/// scripts run once, then style/layout/paint run once over the final DOM. A script
/// (parse or runtime) error aborts the render with a clear [`MochaError`].
/// `console.log` output is written to stderr so it never corrupts the rendered
/// stdout. External `<script src>` is unsupported.
fn run_document_scripts(document: Document) -> MochaResult<Document> {
    let scripts = mocha_js_dom::collect_inline_scripts(&document)?;
    if scripts.is_empty() {
        return Ok(document);
    }
    let shared = Rc::new(RefCell::new(document));
    let mut runtime = mocha_js_dom::DomRuntime::new(shared.clone());
    for source in &scripts {
        runtime.run_script(source)?;
    }
    runtime.run_pending_timers()?;
    for line in runtime.take_console_output() {
        eprintln!("{line}");
    }
    // Reclaim the final document. JS closures captured in listeners/timers may
    // still reference the bridge, so we clone the document out rather than trying
    // to unwrap the shared `Rc`.
    let final_document = shared.borrow().clone();
    Ok(final_document)
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

    #[test]
    fn eval_js_returns_result_and_console() {
        assert_eq!(eval_js("let x = 1 + 2 * 3; x;").unwrap(), "7");
        assert_eq!(
            eval_js("function add(a, b) { return a + b; } add(2, 3);").unwrap(),
            "5"
        );
        assert_eq!(
            eval_js("console.log(\"hello\", 123);").unwrap(),
            "hello 123"
        );
    }

    #[test]
    fn eval_js_reports_errors() {
        assert!(matches!(
            eval_js("missing;").unwrap_err(),
            MochaError::JavaScript(_)
        ));
        assert!(matches!(
            eval_js("let = ;").unwrap_err(),
            MochaError::Parse(_)
        ));
    }

    // --- Milestone 7: inline scripts in the render pipeline -----------------

    fn drawn_text(commands: &[DisplayCommand]) -> Vec<String> {
        commands
            .iter()
            .filter_map(|c| match c {
                DisplayCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn inline_script_text_content_change_reaches_display_list() {
        let html = r#"<html><body><h1 id="t">Before</h1>
            <script>document.getElementById("t").textContent = "After";</script>
            </body></html>"#;
        let commands = run_html(html).unwrap();
        let texts = drawn_text(&commands);
        assert!(texts.contains(&"After".to_string()));
        assert!(!texts.contains(&"Before".to_string()));
    }

    #[test]
    fn script_created_element_appears_in_display_list() {
        let html = r#"<html><body id="b">
            <script>
              let p = document.createElement("p");
              p.textContent = "Injected";
              document.body.appendChild(p);
            </script></body></html>"#;
        assert!(drawn_text(&run_html(html).unwrap()).contains(&"Injected".to_string()));
    }

    #[test]
    fn script_style_mutation_changes_color_and_font_size() {
        let html = r#"<html><body><p id="n">Hi</p>
            <script>document.getElementById("n").setAttribute("style", "color: red; font-size: 24px;");</script>
            </body></html>"#;
        let commands = run_html(html).unwrap();
        assert!(commands.iter().any(|c| matches!(c,
            DisplayCommand::DrawText { text, color, font_size, .. }
                if text == "Hi" && color.r == 255 && color.g == 0 && color.b == 0 && *font_size == 24.0)));
    }

    #[test]
    fn script_class_change_flips_selector_match() {
        let html = r#"<html><body><style>.hot { color: red; }</style><p id="n">Hi</p>
            <script>document.getElementById("n").className = "hot";</script>
            </body></html>"#;
        let commands = run_html(html).unwrap();
        assert!(commands.iter().any(|c| matches!(c,
            DisplayCommand::DrawText { text, color, .. }
                if text == "Hi" && color.r == 255 && color.g == 0)));
    }

    #[test]
    fn script_text_is_not_painted() {
        let html = r#"<html><body><p>Visible</p>
            <script>let secret = "DONOTPAINT"; document.getElementById;</script>
            </body></html>"#;
        let texts = drawn_text(&run_html(html).unwrap());
        assert!(texts.contains(&"Visible".to_string()));
        assert!(!texts.iter().any(|t| t.contains("DONOTPAINT")));
    }

    #[test]
    fn script_error_aborts_render_clearly() {
        let html = r#"<html><body><script>noSuchThing.boom();</script></body></html>"#;
        assert!(matches!(
            run_html(html).unwrap_err(),
            MochaError::JavaScript(_)
        ));
    }

    #[test]
    fn external_script_src_is_unsupported() {
        let html = r#"<html><body><script src="app.js"></script></body></html>"#;
        assert!(matches!(
            run_html(html).unwrap_err(),
            MochaError::UnsupportedFeature(_)
        ));
    }
}
