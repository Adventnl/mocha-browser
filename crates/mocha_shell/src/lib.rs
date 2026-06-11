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
use mocha_forms::{ControlKind, FormState};
use mocha_image::DecodedImage;
use mocha_layout::{
    build_layout_tree, format_layout_tree, LayoutBox, LayoutViewport, DEFAULT_VIEWPORT_WIDTH,
};
use mocha_nav::NavigationController;
use mocha_net::{DefaultLoader, ResourceResponse, ResourceType};
use mocha_style::{ControlBox, ReplacedBox, StyledNode};
use mocha_url::Url;

pub use mocha_layout::NodeId;
pub use mocha_paint::{format_display_list, DisplayCommand};

/// Options controlling a shell run.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Print the layout tree instead of the display list.
    pub dump_layout: bool,
    /// Print the form-control state instead of the display list.
    pub dump_form_state: bool,
    /// Bypass the in-memory cache when loading.
    pub no_cache: bool,
    /// Print response metadata before the output.
    pub show_headers: bool,
}

/// Load `input` and render it, returning the text the CLI should print.
pub fn render_request(input: &str, options: RunOptions) -> MochaResult<String> {
    let response = load_document(input, options)?;

    let mut output = String::new();
    if options.show_headers {
        output.push_str(&format_headers(&response));
        output.push('\n');
    }
    if options.dump_form_state {
        let html = decode_html(&response)?;
        let (document, mut forms) = run_document_scripts(mocha_html::parse_html(html)?)?;
        output.push_str(&format_form_state(&document, &mut forms)?);
    } else {
        let layout = render_to_layout(&response)?;
        if options.dump_layout {
            output.push_str(&format_layout_tree(&layout));
        } else {
            output.push_str(&format_display_list(&mocha_paint::build_display_list(
                &layout,
            )?));
        }
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
/// This is the standalone `--eval-js` path: it does **not** load a document and
/// does **not** install DOM bindings. Inline document `<script>` is a separate
/// path, executed against the DOM by `mocha_js_dom` during the HTML rendering
/// pipeline (see `run_document_scripts`).
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

/// Validate the content type and decode the body as UTF-8.
fn decode_html(response: &ResourceResponse) -> MochaResult<&str> {
    if response.resource_type() != ResourceType::Html {
        return Err(MochaError::UnsupportedFeature(
            "only text/html documents can be rendered in Milestone 4".to_string(),
        ));
    }
    std::str::from_utf8(&response.body).map_err(|_| {
        MochaError::UnsupportedFeature(
            "character encodings other than UTF-8 are not supported in Milestone 4".to_string(),
        )
    })
}

/// Validate the content type, decode the body as UTF-8, and run the engine.
fn render_to_layout(response: &ResourceResponse) -> MochaResult<LayoutBox> {
    layout_document(decode_html(response)?, Some(&response.final_url))
}

/// Render in-memory HTML with no base URL (external subresources unsupported).
fn layout_html(input: &str) -> MochaResult<LayoutBox> {
    layout_document(input, None)
}

/// Parse HTML, execute inline scripts, collect stylesheets (inline `<style>` plus
/// external `<link rel="stylesheet">` resolved against `base`), compute style, and
/// build the layout tree. With no `base` (in-memory rendering) external
/// stylesheets are unsupported. Subresources are collected once, after scripts run.
/// Form-control state (initialized from attributes, mutated by scripts) is
/// resolved into control boxes before layout.
fn layout_document(input: &str, base: Option<&Url>) -> MochaResult<LayoutBox> {
    let (document, mut forms) = run_document_scripts(mocha_html::parse_html(input)?)?;
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
    attach_controls(&mut styled, &document, &mut forms)?;
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
/// timers, returning the mutated document and the final form-control state.
/// Milestone 7 uses coarse invalidation: scripts run once, then
/// style/layout/paint run once over the final DOM. A script (parse or runtime)
/// error aborts the render with a clear [`MochaError`]. `console.log` output is
/// written to stderr so it never corrupts the rendered stdout. External
/// `<script src>` is unsupported. Form state initializes from attributes before
/// scripts run (unsupported control types error here); a `form.submit()` request
/// is noted on stderr but never navigates.
fn run_document_scripts(document: Document) -> MochaResult<(Document, FormState)> {
    let scripts = mocha_js_dom::collect_inline_scripts(&document)?;
    if scripts.is_empty() {
        let forms = mocha_forms::build_form_state(&document)?;
        return Ok((document, forms));
    }
    let shared = Rc::new(RefCell::new(document));
    let mut runtime = mocha_js_dom::DomRuntime::new(shared.clone());
    runtime.init_form_state()?;
    for source in &scripts {
        runtime.run_script(source)?;
    }
    runtime.run_pending_timers()?;
    for line in runtime.take_console_output() {
        eprintln!("{line}");
    }
    if runtime.take_pending_submission().is_some() {
        eprintln!(
            "mocha: a script called form.submit(); form navigation is not performed by the shell"
        );
    }
    // Reclaim the final document and form state. JS closures captured in
    // listeners/timers may still reference the bridge, so we clone both out
    // rather than trying to unwrap the shared `Rc`s.
    let final_document = shared.borrow().clone();
    let forms = runtime.form_state().borrow().clone();
    Ok((final_document, forms))
}

/// Attach resolved [`ControlBox`]es to form-control styled nodes (the forms
/// counterpart of [`attach_images`]). Hidden inputs get no box at all.
fn attach_controls(
    styled: &mut StyledNode,
    document: &Document,
    forms: &mut FormState,
) -> MochaResult<()> {
    styled.control = resolve_control_box(styled, document, forms)?;
    for child in &mut styled.children {
        attach_controls(child, document, forms)?;
    }
    Ok(())
}

/// Resolve one node's control box: its display type, painted value/label,
/// checked/disabled state, and final size (CSS `width`/`height` override the
/// control defaults).
fn resolve_control_box(
    styled: &StyledNode,
    document: &Document,
    forms: &mut FormState,
) -> MochaResult<Option<ControlBox>> {
    let node = styled.node_id;
    // Only the box-generating control elements; options render inside their
    // select, and labels/forms are ordinary flow content.
    if !matches!(
        document.tag_name(node)?,
        Some("input" | "button" | "textarea" | "select")
    ) {
        return Ok(None);
    }
    let is_button_element = document.tag_name(node)? == Some("button");
    let Some(control) = forms.ensure_control(document, node)? else {
        return Ok(None);
    };
    let (kind, value, checked, disabled) = (
        control.kind,
        control.value.clone(),
        control.checked,
        control.disabled,
    );

    let font_size = styled.style.font_size;
    let (control_type, display_value, checked_state, default_width, default_height) = match kind {
        ControlKind::Hidden | ControlKind::Option => return Ok(None),
        ControlKind::Text | ControlKind::Password => {
            (kind.as_str(), Some(value), None, 160.0, 24.0)
        }
        ControlKind::Checkbox | ControlKind::Radio => {
            (kind.as_str(), None, Some(checked), 13.0, 13.0)
        }
        ControlKind::Submit | ControlKind::Reset | ControlKind::Button => {
            // The visible label: a <button>'s text content, an <input>'s value,
            // or the UA default. Width estimates the label with the same metric
            // layout uses for text (chars * font * 0.6) plus padding.
            let label = if is_button_element {
                document.text_content(node)?.trim().to_string()
            } else {
                value
            };
            let label = if label.is_empty() {
                match kind {
                    ControlKind::Reset => "Reset".to_string(),
                    _ => "Submit".to_string(),
                }
            } else {
                label
            };
            let width = (label.chars().count() as f32 * font_size * 0.6).round() + 16.0;
            let control_type = if is_button_element {
                "button"
            } else {
                kind.as_str()
            };
            (control_type, Some(label), None, width.max(40.0), 26.0)
        }
        ControlKind::TextArea => {
            let dimension = |name: &str| {
                document
                    .get_attribute(node, name)
                    .ok()
                    .flatten()
                    .and_then(|attr| attr.trim().parse::<f32>().ok())
                    .filter(|parsed| *parsed > 0.0)
            };
            let width = dimension("cols").map(|cols| cols * 8.0).unwrap_or(200.0);
            let height = dimension("rows").map(|rows| rows * 18.0).unwrap_or(80.0);
            ("textarea", Some(value), None, width, height)
        }
        ControlKind::Select => {
            let value = mocha_forms::select_value(document, forms, node)?;
            ("select", value, None, 160.0, 24.0)
        }
    };

    Ok(Some(ControlBox {
        control_type: control_type.to_string(),
        value: display_value,
        checked: checked_state,
        disabled,
        width: styled.style.width.unwrap_or(default_width),
        height: styled.style.height.unwrap_or(default_height),
    }))
}

/// Format the form-control state of a document as one line per form/control, in
/// document order (the `--dump-form-state` output).
fn format_form_state(document: &Document, forms: &mut FormState) -> MochaResult<String> {
    let mut lines = Vec::new();
    for node in document.traverse_depth_first(document.root_id())? {
        if document.tag_name(node)? == Some("form") {
            lines.push(format!(
                "form node=#{} action={:?} method={:?}",
                node.0,
                document.get_attribute(node, "action")?.unwrap_or(""),
                document.get_attribute(node, "method")?.unwrap_or("get"),
            ));
            continue;
        }
        let Some(control) = forms.ensure_control(document, node)? else {
            continue;
        };
        let mut line = format!(
            "{} node=#{} name={:?} value={:?}",
            control.kind.as_str(),
            node.0,
            control.name.as_deref().unwrap_or(""),
            control.value,
        );
        if matches!(control.kind, ControlKind::Checkbox | ControlKind::Radio) {
            line.push_str(&format!(" checked={}", control.checked));
        }
        if control.kind == ControlKind::Option {
            line.push_str(&format!(" selected={}", control.selected));
        }
        line.push_str(&format!(" disabled={}", control.disabled));
        lines.push(line);
    }
    Ok(lines.join("\n"))
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

    // --- Milestone 10: forms in the render pipeline -------------------------

    /// All `DrawControl` commands as `(type, width, height, value, checked, disabled)`.
    #[allow(clippy::type_complexity)]
    fn draw_controls(
        commands: &[DisplayCommand],
    ) -> Vec<(String, f32, f32, Option<String>, Option<bool>, bool)> {
        commands
            .iter()
            .filter_map(|c| match c {
                DisplayCommand::DrawControl {
                    control_type,
                    width,
                    height,
                    value,
                    checked,
                    disabled,
                    ..
                } => Some((
                    control_type.clone(),
                    *width,
                    *height,
                    value.clone(),
                    *checked,
                    *disabled,
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn text_input_emits_draw_control_with_value_and_default_size() {
        let html =
            r#"<html><body><form action="/s"><input name="q" value="mocha"></form></body></html>"#;
        let controls = draw_controls(&run_html(html).unwrap());
        assert_eq!(
            controls,
            vec![(
                "text".to_string(),
                160.0,
                24.0,
                Some("mocha".to_string()),
                None,
                false
            )]
        );
    }

    #[test]
    fn checkbox_and_radio_emit_checked_state_and_square_size() {
        let html = r#"<html><body><form>
            <input type="checkbox" name="agree" checked>
            <input type="radio" name="size" value="small">
        </form></body></html>"#;
        let controls = draw_controls(&run_html(html).unwrap());
        assert_eq!(controls.len(), 2);
        assert_eq!(
            controls[0],
            ("checkbox".to_string(), 13.0, 13.0, None, Some(true), false)
        );
        assert_eq!(
            controls[1],
            ("radio".to_string(), 13.0, 13.0, None, Some(false), false)
        );
    }

    #[test]
    fn button_width_grows_with_its_label() {
        let short = r#"<html><body><form><button>Go</button></form></body></html>"#;
        let long = r#"<html><body><form><button>A much longer label</button></form></body></html>"#;
        let short_controls = draw_controls(&run_html(short).unwrap());
        let long_controls = draw_controls(&run_html(long).unwrap());
        assert_eq!(short_controls[0].0, "button");
        assert_eq!(short_controls[0].3.as_deref(), Some("Go"));
        assert_eq!(short_controls[0].2, 26.0, "button height");
        assert!(
            long_controls[0].1 > short_controls[0].1,
            "longer label, wider button"
        );
    }

    #[test]
    fn submit_input_label_falls_back_to_submit() {
        let html = r#"<html><body><form><input type="submit"></form></body></html>"#;
        let controls = draw_controls(&run_html(html).unwrap());
        assert_eq!(controls[0].0, "submit");
        assert_eq!(controls[0].3.as_deref(), Some("Submit"));
    }

    #[test]
    fn textarea_size_uses_rows_cols_with_fallback() {
        let sized = r#"<html><body><form><textarea name="m" rows="4" cols="20">x</textarea></form></body></html>"#;
        let controls = draw_controls(&run_html(sized).unwrap());
        assert_eq!((controls[0].1, controls[0].2), (160.0, 72.0)); // 20*8 x 4*18

        let bare = r#"<html><body><form><textarea name="m">x</textarea></form></body></html>"#;
        let controls = draw_controls(&run_html(bare).unwrap());
        assert_eq!((controls[0].1, controls[0].2), (200.0, 80.0));
    }

    #[test]
    fn select_emits_the_selected_option_value() {
        let html = r#"<html><body><form><select name="c">
            <option value="a">Alpha</option>
            <option value="b" selected>Beta</option>
        </select></form></body></html>"#;
        let controls = draw_controls(&run_html(html).unwrap());
        assert_eq!(controls[0].0, "select");
        assert_eq!(controls[0].3.as_deref(), Some("b"));
        // Option labels are not painted as separate text.
        let texts = drawn_text(&run_html(html).unwrap());
        assert!(!texts.contains(&"Alpha".to_string()));
    }

    #[test]
    fn css_width_and_height_override_control_size() {
        let html = r#"<html><body><form><input name="q" style="width: 300px; height: 40px;"></form></body></html>"#;
        let controls = draw_controls(&run_html(html).unwrap());
        assert_eq!((controls[0].1, controls[0].2), (300.0, 40.0));
    }

    #[test]
    fn hidden_input_paints_nothing() {
        let html =
            r#"<html><body><form><input type="hidden" name="t" value="x"></form></body></html>"#;
        assert!(draw_controls(&run_html(html).unwrap()).is_empty());
    }

    #[test]
    fn display_none_control_is_not_painted() {
        let html =
            r#"<html><body><form><input name="q" style="display: none;"></form></body></html>"#;
        assert!(draw_controls(&run_html(html).unwrap()).is_empty());
    }

    #[test]
    fn disabled_state_reaches_the_display_list() {
        let html = r#"<html><body><form><input name="q" value="x" disabled></form></body></html>"#;
        let controls = draw_controls(&run_html(html).unwrap());
        assert!(controls[0].5, "disabled included in DrawControl");
    }

    #[test]
    fn label_text_and_control_share_a_line() {
        let html = r#"<html><body><form><label for="q">Search</label> <input id="q" name="q"></form></body></html>"#;
        let commands = run_html(html).unwrap();
        let text_y = commands
            .iter()
            .find_map(|c| match c {
                DisplayCommand::DrawText { text, y, .. } if text == "Search" => Some(*y),
                _ => None,
            })
            .expect("label text painted");
        let control_y = commands
            .iter()
            .find_map(|c| match c {
                DisplayCommand::DrawControl { y, .. } => Some(*y),
                _ => None,
            })
            .expect("control painted");
        assert_eq!(text_y, control_y, "label and input share a line top");
    }

    #[test]
    fn js_form_state_changes_reach_the_display_list() {
        let html = r#"<html><body><form>
            <input id="name" name="name" value="Before">
            <input id="agree" name="agree" type="checkbox">
        </form>
        <script>
          document.getElementById("name").value = "After";
          document.getElementById("agree").checked = true;
        </script></body></html>"#;
        let controls = draw_controls(&run_html(html).unwrap());
        assert_eq!(controls[0].3.as_deref(), Some("After"));
        assert_eq!(controls[1].4, Some(true));
    }

    #[test]
    fn unsupported_input_type_fails_the_render_clearly() {
        let html = r#"<html><body><form><input type="date" name="d"></form></body></html>"#;
        assert!(matches!(
            run_html(html).unwrap_err(),
            MochaError::UnsupportedFeature(_)
        ));
    }

    #[test]
    fn dump_form_state_lists_forms_and_controls() {
        let server = TestServer::start(vec![(
            "/form.html".to_string(),
            Reply::Html(
                r#"<html><body><form action="/search" method="get">
                    <input name="q" value="mocha">
                    <input type="checkbox" name="agree" checked>
                </form></body></html>"#
                    .to_string(),
            ),
        )]);
        let out = render_request(
            &server.url("/form.html"),
            RunOptions {
                dump_form_state: true,
                ..RunOptions::default()
            },
        )
        .unwrap();
        assert!(out.contains(r#"form node=#"#), "form line present: {out}");
        assert!(out.contains(r#"action="/search""#));
        assert!(out.contains(r#"text node=#"#));
        assert!(out.contains(r#"value="mocha""#));
        assert!(out.contains("checked=true"));
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
