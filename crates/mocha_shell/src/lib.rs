//! The Mocha Browser terminal shell library.
//!
//! This crate is a thin terminal frontend over `mocha_engine`, which owns the
//! whole rendering pipeline (load → parse → scripts → subresources → style →
//! layout → display list). The shell renders through the engine and prints the
//! display list (default), the layout tree (`--dump-layout`), or the
//! form-control state (`--dump-form-state`), optionally preceded by response
//! headers. It does **not** open a window — that is `mocha_desktop` (Milestone
//! 11). `https://` loads over TLS since Milestone 21. `--eval-js` evaluates a
//! standalone JavaScript snippet with no DOM.

use mocha_devtools::{format_snapshot, snapshot_rendered_page};
use mocha_engine::{render_html, render_url, RenderOptions, ResponseMeta};
use mocha_error::MochaResult;
use mocha_layout::{format_layout_tree, hit_test};

pub use mocha_layout::NodeId;
pub use mocha_paint::{format_display_list, DisplayCommand};

/// Options controlling a shell run.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Print the layout tree instead of the display list.
    pub dump_layout: bool,
    /// Print the form-control state instead of the display list.
    pub dump_form_state: bool,
    /// Bypass the in-memory loader cache.
    pub no_cache: bool,
    /// Print response metadata before the output.
    pub show_headers: bool,
    /// Print a headless DevTools snapshot instead of shell display output.
    pub devtools_snapshot: bool,
}

fn render_options(options: RunOptions) -> RenderOptions {
    RenderOptions {
        no_cache: options.no_cache,
        ..RenderOptions::default()
    }
}

/// Load `input` and render it, returning the text the CLI should print.
pub fn render_request(input: &str, options: RunOptions) -> MochaResult<String> {
    let mut page = render_url(input, &render_options(options))?;
    report_side_effects(&page.console_output, page.submitted_form.is_some());

    let mut output = String::new();
    if options.show_headers {
        output.push_str(&format_headers(page.meta.as_ref()));
        output.push('\n');
    }
    if options.devtools_snapshot {
        output.push_str(&format_snapshot(&snapshot_rendered_page(
            &page,
            Some(input.to_string()),
        )?));
    } else if options.dump_form_state {
        output.push_str(&mocha_engine::format_form_state(
            &page.document,
            &mut page.form_state,
        )?);
    } else if options.dump_layout {
        output.push_str(&format_layout_tree(&page.layout_root));
    } else {
        output.push_str(&format_display_list(&page.display_list));
    }
    Ok(output)
}

/// Load `input` and render a deterministic headless DevTools snapshot.
pub fn devtools_snapshot_request(input: &str) -> MochaResult<String> {
    render_request(
        input,
        RunOptions {
            devtools_snapshot: true,
            ..RunOptions::default()
        },
    )
}

/// Load a location (file or http) and produce its display list.
pub fn run_file(input: &str) -> MochaResult<Vec<DisplayCommand>> {
    Ok(render_url(input, &RenderOptions::default())?.display_list)
}

/// Load a location (file or http) and produce its formatted layout-tree dump.
pub fn dump_layout_file(input: &str) -> MochaResult<String> {
    Ok(format_layout_tree(
        &render_url(input, &RenderOptions::default())?.layout_root,
    ))
}

/// Evaluate a standalone JavaScript snippet and return its captured console
/// output followed by the result value (omitted when `undefined`).
///
/// This is the standalone `--eval-js` path: it does **not** load a document and
/// does **not** install DOM bindings.
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
    let page = render_url(input, &RenderOptions::default())?;
    Ok(hit_test(&page.layout_root, x, y))
}

/// Render an in-memory HTML string to a display list (no loading).
pub fn run_html(input: &str) -> MochaResult<Vec<DisplayCommand>> {
    Ok(render_html(input, &RenderOptions::default())?.display_list)
}

/// Render an in-memory HTML string to a layout-tree dump (no loading).
pub fn dump_layout_html(input: &str) -> MochaResult<String> {
    Ok(format_layout_tree(
        &render_html(input, &RenderOptions::default())?.layout_root,
    ))
}

/// Print captured `console.log` output and any `form.submit()` note to stderr,
/// so they never corrupt the rendered stdout.
fn report_side_effects(console_output: &[String], submitted: bool) {
    for line in console_output {
        eprintln!("{line}");
    }
    if submitted {
        eprintln!(
            "mocha: a script called form.submit(); form navigation is not performed by the shell"
        );
    }
}

fn format_headers(meta: Option<&ResponseMeta>) -> String {
    let Some(meta) = meta else {
        return String::new();
    };
    let mut lines = vec![format!("url: {}", meta.final_url.normalized())];
    if let Some(status) = meta.status {
        lines.push(format!("status: {status}"));
    }
    if let Some(content_type) = &meta.content_type {
        lines.push(format!("content-type: {content_type}"));
    }
    lines.push(format!("from-cache: {}", meta.from_cache));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_error::MochaError;
    use mocha_net::test_server::{Reply, TestServer};

    #[test]
    fn https_connection_failure_is_a_clear_network_error() {
        // https is supported since Milestone 21. Nothing listens on port 1, so
        // the connection is refused locally and the test stays offline.
        let error = run_file("https://127.0.0.1:1/index.html").unwrap_err();
        assert!(matches!(error, MochaError::Network(_)));
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
    fn devtools_snapshot_includes_inspector_sections() {
        let server = TestServer::start(vec![(
            "/index.html".to_string(),
            Reply::Html("<html><body><p>Hi</p></body></html>".to_string()),
        )]);
        let out = devtools_snapshot_request(&server.url("/index.html")).unwrap();
        assert!(out.contains("DevToolsSnapshot"));
        assert!(out.contains("DOM\n"));
        assert!(out.contains("DrawText \"Hi\""));
        assert!(out.contains("Network\n  document"));
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
