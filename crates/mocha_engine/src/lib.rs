//! The shared rendering pipeline for Mocha Browser.
//!
//! Both the terminal shell (`mocha_shell`) and the desktop shell
//! (`mocha_desktop`) render documents through this one crate, so the pipeline
//! lives in exactly one place:
//!
//! ```text
//! input -> mocha_url -> mocha_nav/mocha_net (load) -> content-type + UTF-8
//!       -> mocha_html -> form state init + inline <script> (mocha_js_dom)
//!       -> subresources: external <link> CSS + <img> images (RGBA)
//!       -> mocha_style -> control metrics (mocha_forms) -> mocha_layout
//!       -> display list (mocha_paint)  => RenderedPage
//! ```
//!
//! The result is a [`RenderedPage`] carrying everything an embedder needs: the
//! document and form state (for hit testing and interaction), the layout tree,
//! the display list, the decoded image pixels (so a rasterizer can resolve
//! `DrawImage`), the document height (for scrolling), and any `console.log`
//! output. The engine performs **no** terminal or window I/O.
//!
//! Milestone 11 uses **coarse full-document rerender**: after any interaction or
//! script mutation, an embedder re-runs the whole pipeline (or, for an
//! already-loaded document, [`render_state`]) rather than invalidating
//! incrementally. Incremental relayout is deferred.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mocha_dom::{Document, NodeId};
use mocha_error::{MochaError, MochaResult};
use mocha_forms::{resolve_control_metrics, FormState};
use mocha_image::RasterImage;
use mocha_layout::{build_layout_tree, LayoutBox, LayoutViewport};
use mocha_nav::NavigationController;
use mocha_net::{DefaultLoader, ResourceResponse, ResourceType};
use mocha_paint::{build_display_list, DisplayCommand};
use mocha_style::{ControlBox, ReplacedBox, StyledNode};
use mocha_url::Url;

pub use mocha_css::Stylesheet;

/// Options controlling a render.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Viewport width in CSS px (drives line wrapping and block widths).
    pub viewport_width: f32,
    /// Viewport height in CSS px (informational for layout; used by scrolling).
    pub viewport_height: f32,
    /// Bypass the in-memory loader cache.
    pub no_cache: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            viewport_width: mocha_layout::DEFAULT_VIEWPORT_WIDTH,
            viewport_height: mocha_layout::DEFAULT_VIEWPORT_HEIGHT,
            no_cache: false,
        }
    }
}

/// Loaded-response metadata, present only for the network/file path.
#[derive(Debug, Clone)]
pub struct ResponseMeta {
    /// The final URL after redirects (the document base URL).
    pub final_url: Url,
    /// The HTTP status, if any.
    pub status: Option<u16>,
    /// The response content type, if any.
    pub content_type: Option<String>,
    /// Whether the response came from the loader cache.
    pub from_cache: bool,
}

/// A fully rendered document and everything an embedder needs to display and
/// interact with it.
pub struct RenderedPage {
    /// Loaded-response metadata (`None` for in-memory HTML with no base URL).
    pub meta: Option<ResponseMeta>,
    /// The final (post-script) document.
    pub document: Document,
    /// The final form-control state.
    pub form_state: FormState,
    /// The resolved author stylesheets (inline `<style>` plus loaded external
    /// `<link>` CSS), kept so an embedder can [`relayout`] after an interaction
    /// without re-fetching them.
    pub stylesheets: Vec<Stylesheet>,
    /// The layout tree (for hit testing and re-paint).
    pub layout_root: LayoutBox,
    /// The display list (for rasterization / terminal dump).
    pub display_list: Vec<DisplayCommand>,
    /// Decoded image pixels, indexed by the `image_id` carried in
    /// `DrawImage`/`Image` boxes.
    pub images: Vec<RasterImage>,
    /// Total laid-out document height in px (for scroll clamping).
    pub document_height: f32,
    /// Captured `console.log` output from inline scripts.
    pub console_output: Vec<String>,
    /// The form a script asked to submit via `form.submit()` (the embedder
    /// decides whether to act on it); `None` if no request was made.
    pub submitted_form: Option<NodeId>,
    /// Non-fatal diagnostics gathered while rendering (Milestone 23). When a
    /// stylesheet, script, or image cannot be processed it is **skipped** — the
    /// page still renders — and the reason is recorded here. Empty when the page
    /// rendered with no skipped features.
    pub diagnostics: Vec<String>,
}

impl RenderedPage {
    /// The document base URL, if the page was loaded (not in-memory).
    pub fn base_url(&self) -> Option<&Url> {
        self.meta.as_ref().map(|meta| &meta.final_url)
    }

    /// Rebuild the display list from the (possibly mutated) layout tree.
    pub fn rebuild_display_list(&mut self) -> MochaResult<()> {
        self.display_list = build_display_list(&self.layout_root)?;
        Ok(())
    }
}

/// Load `input` (file/http) and render it at the given viewport.
pub fn render_url(input: &str, options: &RenderOptions) -> MochaResult<RenderedPage> {
    let response = load(input, options)?;
    let meta = ResponseMeta {
        final_url: response.final_url.clone(),
        status: response.status,
        content_type: response.content_type.clone(),
        from_cache: response.from_cache,
    };
    let html = decode_html(&response)?;
    let base = response.final_url.clone();
    let mut page = render_html_with_base(html, Some(&base), options)?;
    page.meta = Some(meta);
    Ok(page)
}

/// Render an in-memory HTML string (no base URL: external subresources are
/// unsupported and report a clear error).
pub fn render_html(input: &str, options: &RenderOptions) -> MochaResult<RenderedPage> {
    render_html_with_base(input, None, options)
}

/// Just the geometry products of a render: the layout tree, display list, and
/// document height. Returned by [`relayout`].
pub struct LayoutResult {
    /// The layout tree (for hit testing and re-paint).
    pub layout_root: LayoutBox,
    /// The display list (for rasterization / terminal dump).
    pub display_list: Vec<DisplayCommand>,
    /// Total laid-out document height in px.
    pub document_height: f32,
}

/// Re-run style + layout + paint over an already-loaded document and its
/// (possibly mutated) form state, **without** reloading, re-running scripts, or
/// re-fetching stylesheets/images. This is the coarse rerender the desktop shell
/// calls after a click/keystroke changes form state, or after a resize. It
/// clones nothing heavy: the document, `stylesheets`, and `images` are borrowed.
pub fn relayout(
    document: &Document,
    form_state: &mut FormState,
    stylesheets: &[Stylesheet],
    images: &[RasterImage],
    options: &RenderOptions,
) -> MochaResult<LayoutResult> {
    let layout_root = build_layout(document, form_state, stylesheets, images, options)?;
    let display_list = build_display_list(&layout_root)?;
    let document_height = layout_root.rect.height;
    Ok(LayoutResult {
        layout_root,
        display_list,
        document_height,
    })
}

/// The shared core: parse, init form state, run scripts, load subresources,
/// style, lay out, and paint.
fn render_html_with_base(
    html: &str,
    base: Option<&Url>,
    options: &RenderOptions,
) -> MochaResult<RenderedPage> {
    let ScriptOutcome {
        document,
        mut form_state,
        console_output,
        submitted_form,
        mut diagnostics,
    } = run_document_scripts(mocha_html::parse_html(html)?, base);

    // Author CSS is fail-open: if the stylesheets cannot be parsed/loaded, skip
    // them (render with UA defaults) rather than aborting the whole page.
    let stylesheets = match collect_stylesheets(&document, base) {
        Ok(stylesheets) => stylesheets,
        Err(error) => {
            diagnostics.push(format!("author CSS skipped: {error}"));
            Vec::new()
        }
    };
    // Per-declaration/selector/at-rule skips from forgiving CSS parsing are
    // reported (deduplicated) so unsupported features are surfaced, not faked.
    {
        let mut seen = std::collections::HashSet::new();
        for sheet in &stylesheets {
            for note in &sheet.skipped {
                if seen.insert(note.clone()) {
                    diagnostics.push(note.clone());
                }
            }
        }
    }

    let (images, image_diagnostics) = load_images(&document, base);
    diagnostics.extend(image_diagnostics);

    let LayoutResult {
        layout_root,
        display_list,
        document_height,
    } = relayout(&document, &mut form_state, &stylesheets, &images, options)?;

    Ok(RenderedPage {
        meta: None,
        document,
        form_state,
        stylesheets,
        layout_root,
        display_list,
        images,
        document_height,
        console_output,
        submitted_form,
        diagnostics,
    })
}

// === loading ================================================================

fn load(input: &str, options: &RenderOptions) -> MochaResult<ResourceResponse> {
    let url = Url::parse(input)?;
    let mut controller = NavigationController::new(DefaultLoader::new());
    if options.no_cache {
        controller.navigate_no_cache(url)
    } else {
        controller.navigate(url)
    }
}

/// Validate the content type and decode the body as UTF-8.
pub fn decode_html(response: &ResourceResponse) -> MochaResult<&str> {
    if response.resource_type() != ResourceType::Html {
        return Err(MochaError::UnsupportedFeature(
            "only text/html documents can be rendered".to_string(),
        ));
    }
    std::str::from_utf8(&response.body).map_err(|_| {
        MochaError::UnsupportedFeature(
            "character encodings other than UTF-8 are not supported".to_string(),
        )
    })
}

// === scripts ================================================================

struct ScriptOutcome {
    document: Document,
    form_state: FormState,
    console_output: Vec<String>,
    submitted_form: Option<NodeId>,
    diagnostics: Vec<String>,
}

/// Initialize form state, run inline `<script>`s in document order, then pending
/// timers, returning the mutated document, final form state, captured console
/// output, any `form.submit()` request, and non-fatal diagnostics.
///
/// Milestone 23 makes this **fail-open**: an external `<script src>`, a script
/// parse/runtime error, an unsupported control type, or a timer error is recorded
/// as a diagnostic and skipped — the document still renders. (Per-feature
/// JavaScript depth grows in a later milestone; here a script that cannot run is
/// simply not run.)
fn run_document_scripts(document: Document, base: Option<&Url>) -> ScriptOutcome {
    let scripts = match mocha_js_dom::collect_inline_scripts(&document) {
        Ok(scripts) => scripts,
        // External `<script src>` (or another collection failure): skip script
        // execution entirely, but still render the document and its form state.
        Err(error) => {
            return no_script_outcome(document, vec![format!("scripts skipped: {error}")])
        }
    };
    if scripts.is_empty() {
        return no_script_outcome(document, Vec::new());
    }

    let shared = Rc::new(RefCell::new(document));
    // The document URL gives scripts an origin for `document.cookie` and web
    // storage (Milestone 15); `None` for in-memory HTML leaves them unavailable.
    let mut runtime = mocha_js_dom::DomRuntime::with_url(shared.clone(), base.cloned());
    let mut diagnostics = Vec::new();
    if let Err(error) = runtime.init_form_state() {
        diagnostics.push(format!("form controls skipped: {error}"));
    }
    for source in &scripts {
        if let Err(error) = runtime.run_script(source) {
            diagnostics.push(format!("script error: {error}"));
        }
    }
    if let Err(error) = runtime.run_pending_timers() {
        diagnostics.push(format!("timer error: {error}"));
    }
    let console_output = runtime.take_console_output();
    let submitted_form = runtime.take_pending_submission();
    // JS closures captured in listeners/timers may still reference the bridge, so
    // clone the final document and form state out rather than unwrapping the Rcs.
    let document = shared.borrow().clone();
    let form_state = runtime.form_state().borrow().clone();
    ScriptOutcome {
        document,
        form_state,
        console_output,
        submitted_form,
        diagnostics,
    }
}

/// The no-scripts outcome: build form state (fail-open on unsupported controls)
/// and carry any prior diagnostics.
fn no_script_outcome(document: Document, mut diagnostics: Vec<String>) -> ScriptOutcome {
    let form_state = match mocha_forms::build_form_state(&document) {
        Ok(form_state) => form_state,
        Err(error) => {
            diagnostics.push(format!("form controls skipped: {error}"));
            FormState::new()
        }
    };
    ScriptOutcome {
        document,
        form_state,
        console_output: Vec::new(),
        submitted_form: None,
        diagnostics,
    }
}

// === stylesheets ============================================================

fn collect_stylesheets(document: &Document, base: Option<&Url>) -> MochaResult<Vec<Stylesheet>> {
    match base {
        Some(base) => {
            let mut loader = DefaultLoader::new();
            mocha_resources::collect_document_stylesheets(document, base, &mut loader)
        }
        None => mocha_resources::collect_inline_stylesheets(document),
    }
}

// === images =================================================================

/// Discover, load, and decode every `<img>` to RGBA. Returns the decoded pixels
/// (one entry per discovered `<img>` in document order, indexed by `image_id`)
/// plus non-fatal diagnostics.
///
/// Milestone 23 makes images fail-open: an `<img>` that cannot be loaded or
/// decoded (404, unsupported format, missing base URL) becomes a 1×1 transparent
/// placeholder so `image_id` indices stay aligned with discovery order, and the
/// reason is recorded as a diagnostic. The page renders without that image.
fn load_images(document: &Document, base: Option<&Url>) -> (Vec<RasterImage>, Vec<String>) {
    let discovered = discover_images_lenient(document);
    if discovered.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let Some(base) = base else {
        return (
            Vec::new(),
            vec!["images skipped: in-memory HTML has no base URL".to_string()],
        );
    };
    let mut loader = DefaultLoader::new();
    let mut decoded = Vec::with_capacity(discovered.len());
    let mut diagnostics = Vec::new();
    for (_node, src) in &discovered {
        match mocha_resources::load_image_rgba(src, base, &mut loader) {
            Ok(image) => decoded.push(image),
            Err(error) => {
                diagnostics.push(format!("image '{src}' skipped: {error}"));
                decoded.push(placeholder_image());
            }
        }
    }
    (decoded, diagnostics)
}

/// A 1×1 transparent stand-in for an `<img>` that could not be loaded/decoded.
fn placeholder_image() -> RasterImage {
    RasterImage {
        width: 1,
        height: 1,
        rgba: vec![0, 0, 0, 0],
    }
}

/// Discover `<img>` elements with a non-empty `src`, in document order, **without
/// erroring** on a src-less `<img>` (it is simply skipped). This is the lenient
/// counterpart to `mocha_resources::discover_images`; the decode pass and the
/// layout-time node→image map both use it so they agree on order and indexing.
fn discover_images_lenient(document: &Document) -> Vec<(NodeId, String)> {
    let mut images = Vec::new();
    let Ok(order) = document.traverse_depth_first(document.root_id()) else {
        return images;
    };
    for id in order {
        if document.tag_name(id).ok().flatten() != Some("img") {
            continue;
        }
        if let Some(src) = document
            .get_attribute(id, "src")
            .ok()
            .flatten()
            .filter(|src| !src.is_empty())
        {
            images.push((id, src.to_string()));
        }
    }
    images
}

// === style + layout =========================================================

/// Build the styled tree, attach image and control boxes, and lay it out.
fn build_layout(
    document: &Document,
    form_state: &mut FormState,
    stylesheets: &[Stylesheet],
    images: &[RasterImage],
    options: &RenderOptions,
) -> MochaResult<LayoutBox> {
    // Map node -> image_id by matching the `<img>` discovery order against the
    // decoded list (the same order `load_images` used).
    let by_node = image_node_map(document, images.len())?;
    let mut styled = mocha_style::build_style_tree(document, stylesheets)?;
    attach_images(&mut styled, document, images, &by_node);
    attach_controls(&mut styled, document, form_state)?;
    let viewport = LayoutViewport {
        width: options.viewport_width,
        height: options.viewport_height,
    };
    build_layout_tree(&styled, viewport)
}

/// Rebuild the `<img>` node→image_id map (discovery order matches decode order).
/// Uses the same lenient discovery as [`load_images`] so indices line up even
/// when some images were skipped (placeholders).
fn image_node_map(document: &Document, image_count: usize) -> MochaResult<HashMap<NodeId, usize>> {
    if image_count == 0 {
        return Ok(HashMap::new());
    }
    Ok(discover_images_lenient(document)
        .into_iter()
        .enumerate()
        .map(|(id, (node, _src))| (node, id))
        .collect())
}

fn attach_images(
    styled: &mut StyledNode,
    document: &Document,
    images: &[RasterImage],
    by_node: &HashMap<NodeId, usize>,
) {
    if let Some(&image_id) = by_node.get(&styled.node_id) {
        // `images.get` rather than index: a stale image list (e.g. a mismatched
        // `render_state` call) skips the box instead of panicking.
        if let Some(decoded) = images.get(image_id) {
            let (width, height) = resolve_image_size(styled, document, decoded);
            styled.replaced = Some(ReplacedBox {
                image_id,
                width,
                height,
            });
        }
    }
    for child in &mut styled.children {
        attach_images(child, document, images, by_node);
    }
}

fn resolve_image_size(
    styled: &StyledNode,
    document: &Document,
    decoded: &RasterImage,
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

/// Attach resolved control boxes to form-control styled nodes, using the shared
/// [`resolve_control_metrics`] sizing rules (no sizing logic lives here).
fn attach_controls(
    styled: &mut StyledNode,
    document: &Document,
    form_state: &mut FormState,
) -> MochaResult<()> {
    let metrics = resolve_control_metrics(
        document,
        form_state,
        styled.node_id,
        styled.style.width,
        styled.style.height,
        styled.style.font_size,
    )?;
    styled.control = metrics.map(|m| ControlBox {
        control_type: m.control_type,
        value: m.value,
        checked: m.checked,
        disabled: m.disabled,
        width: m.width,
        height: m.height,
    });
    for child in &mut styled.children {
        attach_controls(child, document, form_state)?;
    }
    Ok(())
}

// === form-state dump (shared by --dump-form-state) ==========================

/// Format the form-control state of a document as one line per form/control, in
/// document order (the `--dump-form-state` output).
pub fn format_form_state(document: &Document, form_state: &mut FormState) -> MochaResult<String> {
    use mocha_forms::ControlKind;
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
        let Some(control) = form_state.ensure_control(document, node)? else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_net::test_server::{Reply, TestServer};

    fn drawn_text(page: &RenderedPage) -> Vec<String> {
        page.display_list
            .iter()
            .filter_map(|c| match c {
                DisplayCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn draw_controls(page: &RenderedPage) -> Vec<(String, Option<String>, Option<bool>)> {
        page.display_list
            .iter()
            .filter_map(|c| match c {
                DisplayCommand::DrawControl {
                    control_type,
                    value,
                    checked,
                    ..
                } => Some((control_type.clone(), value.clone(), *checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn render_html_produces_display_list_and_height() {
        let page = render_html(
            "<html><body><p>Hello world</p></body></html>",
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(drawn_text(&page).contains(&"Hello".to_string()));
        assert!(page.document_height > 0.0);
        assert!(page.meta.is_none());
    }

    #[test]
    fn render_html_runs_scripts_and_captures_console() {
        let page = render_html(
            r#"<html><body><p id="t">Before</p>
               <script>console.log("hi"); document.getElementById("t").textContent = "After";</script>
               </body></html>"#,
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(drawn_text(&page).contains(&"After".to_string()));
        assert!(!drawn_text(&page).contains(&"Before".to_string()));
        assert_eq!(page.console_output, vec!["hi".to_string()]);
    }

    #[test]
    fn render_html_resolves_control_boxes() {
        let page = render_html(
            r#"<html><body><form><input name="q" value="mocha"><input type="checkbox" checked></form></body></html>"#,
            &RenderOptions::default(),
        )
        .unwrap();
        assert_eq!(
            draw_controls(&page),
            vec![
                ("text".to_string(), Some("mocha".to_string()), None),
                ("checkbox".to_string(), None, Some(true)),
            ]
        );
    }

    #[test]
    fn form_submit_is_recorded_not_navigated() {
        let page = render_html(
            r#"<html><body><form id="f" action="/go"><input name="q" value="x"></form>
               <script>document.getElementById("f").submit();</script></body></html>"#,
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(page.submitted_form.is_some());
    }

    #[test]
    fn render_url_over_http_carries_meta_and_renders() {
        let server = TestServer::start(vec![(
            "/index.html".to_string(),
            Reply::Html("<html><body><p>Hi</p></body></html>".to_string()),
        )]);
        let page = render_url(&server.url("/index.html"), &RenderOptions::default()).unwrap();
        assert!(drawn_text(&page).contains(&"Hi".to_string()));
        let meta = page.meta.expect("loaded page has meta");
        assert_eq!(meta.status, Some(200));
        assert!(meta
            .content_type
            .as_deref()
            .unwrap()
            .starts_with("text/html"));
    }

    #[test]
    fn render_url_decodes_image_pixels() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let server = TestServer::start(vec![
            (
                "/index.html".to_string(),
                Reply::Html(r#"<html><body><img src="p.png"></body></html>"#.to_string()),
            ),
            (
                "/p.png".to_string(),
                Reply::Bytes {
                    content_type: "image/png".to_string(),
                    body: png,
                },
            ),
        ]);
        let page = render_url(&server.url("/index.html"), &RenderOptions::default()).unwrap();
        assert_eq!(page.images.len(), 1);
        assert_eq!((page.images[0].width, page.images[0].height), (4, 3));
        assert!(page
            .display_list
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawImage { image_id: 0, .. })));
    }

    #[test]
    fn realistic_document_with_head_and_unknown_tags_renders() {
        // A real-world-shaped document: doctype, a <head> with metadata, and a
        // <body> using tags outside the old allow-list. None of this aborts now.
        let page = render_html(
            r#"<!doctype html>
            <html>
              <head><meta charset="utf-8"><title>Example</title></head>
              <body>
                <header><h1>Welcome</h1></header>
                <nav><ul><li>Home</li><li>About</li></ul></nav>
                <main><p>Hello&nbsp;from the real web &amp; beyond.</p></main>
              </body>
            </html>"#,
            &RenderOptions::default(),
        )
        .unwrap();
        let text = drawn_text(&page);
        assert!(text.contains(&"Welcome".to_string()));
        assert!(text.iter().any(|t| t.contains("Home")));
        // The list marker was emitted as leading inline text.
        assert!(text.iter().any(|t| t.contains('•')));
        // The entities decoded: `&amp;` became a literal `&` in the text runs.
        let joined = text.join(" ");
        assert!(
            joined.contains('&') && !joined.contains("&amp;"),
            "entities should decode; joined runs: {joined}"
        );
        assert!(
            page.diagnostics.is_empty(),
            "clean page: {:?}",
            page.diagnostics
        );
    }

    #[test]
    fn unsupported_css_is_skipped_not_fatal() {
        // A stylesheet using unsupported CSS must not abort the page; the content
        // still renders and the skip is recorded as a diagnostic.
        let page = render_html(
            r#"<html><body><style>p:hover { color: rgb(1, 2, 3); }</style><p>Hi</p></body></html>"#,
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(drawn_text(&page).contains(&"Hi".to_string()));
        assert!(
            page.diagnostics.iter().any(|d| d.contains("CSS")),
            "expected a CSS diagnostic, got {:?}",
            page.diagnostics
        );
    }

    #[test]
    fn external_script_is_skipped_not_fatal() {
        let page = render_html(
            r#"<html><body><script src="app.js"></script><p>Hi</p></body></html>"#,
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(drawn_text(&page).contains(&"Hi".to_string()));
        assert!(
            page.diagnostics.iter().any(|d| d.contains("script")),
            "expected a script diagnostic, got {:?}",
            page.diagnostics
        );
    }

    #[test]
    fn https_connection_failure_is_a_clear_network_error() {
        // https is supported since Milestone 21. Nothing listens on port 1,
        // so the connection is refused locally — the test stays offline while
        // proving https routes into the real network path.
        assert!(matches!(
            render_url("https://127.0.0.1:1/", &RenderOptions::default()),
            Err(MochaError::Network(_))
        ));
    }

    #[test]
    fn relayout_reflects_mutated_form_state() {
        // Render once, flip a checkbox in the form state, relayout without scripts.
        let page = render_html(
            r#"<html><body><form><input id="c" type="checkbox" name="a"></form></body></html>"#,
            &RenderOptions::default(),
        )
        .unwrap();
        let checkbox = page.document.get_element_by_id("c").unwrap().unwrap();
        let mut state = page.form_state.clone();
        state.control_mut(checkbox).unwrap().checked = true;
        let result = relayout(
            &page.document,
            &mut state,
            &page.stylesheets,
            &page.images,
            &RenderOptions::default(),
        )
        .unwrap();
        let checked = result.display_list.iter().any(|c| {
            matches!(c, DisplayCommand::DrawControl { control_type, checked: Some(true), .. }
                if control_type == "checkbox")
        });
        assert!(checked, "the relayout reflects the flipped checkbox");
    }

    #[test]
    fn narrower_viewport_increases_document_height() {
        let html = "<html><body><p>alpha beta gamma delta epsilon zeta eta theta</p></body></html>";
        let wide = render_html(
            html,
            &RenderOptions {
                viewport_width: 800.0,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        let narrow = render_html(
            html,
            &RenderOptions {
                viewport_width: 120.0,
                ..RenderOptions::default()
            },
        )
        .unwrap();
        assert!(narrow.document_height > wide.document_height);
    }
}
