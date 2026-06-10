//! End-to-end tests for Milestone 7: inline `<script>` execution wired into the
//! render pipeline, plus JavaScript event listeners dispatched against the DOM.
//!
//! These use only local in-memory HTML — no network, no real window.

use std::cell::RefCell;
use std::rc::Rc;

use mocha_dom::Document;
use mocha_error::MochaError;
use mocha_js_dom::{collect_inline_scripts, DomRuntime};
use mocha_shell::{run_html, DisplayCommand};

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
fn inline_script_mutates_dom_and_display_list_changes() {
    let html = r#"<html><body><h1 id="t">Before</h1>
        <script>document.getElementById("t").textContent = "After";</script>
        </body></html>"#;
    let texts = drawn_text(&run_html(html).unwrap());
    assert!(texts.contains(&"After".to_string()));
    assert!(!texts.contains(&"Before".to_string()));
}

#[test]
fn script_created_element_is_rendered() {
    // Text is laid out as per-word runs, so assert on a single-word string.
    let html = r#"<html><body id="b">
        <script>
          let span = document.createElement("span");
          span.textContent = "Injected";
          document.body.appendChild(span);
        </script></body></html>"#;
    assert!(drawn_text(&run_html(html).unwrap()).contains(&"Injected".to_string()));
}

#[test]
fn script_style_mutation_changes_final_paint() {
    let html = r#"<html><body><p id="n">Styled</p>
        <script>document.getElementById("n").setAttribute("style", "color: red; font-size: 24px;");</script>
        </body></html>"#;
    let commands = run_html(html).unwrap();
    assert!(commands.iter().any(|c| matches!(c,
        DisplayCommand::DrawText { text, color, font_size, .. }
            if text == "Styled" && color.r == 255 && color.g == 0 && color.b == 0 && *font_size == 24.0)));
}

#[test]
fn script_error_fails_render_clearly() {
    let html = r#"<html><body><script>boom.bang();</script></body></html>"#;
    assert!(matches!(
        run_html(html).unwrap_err(),
        MochaError::JavaScript(_)
    ));
}

#[test]
fn external_script_is_unsupported() {
    let html = r#"<html><body><script src="x.js"></script></body></html>"#;
    assert!(matches!(
        run_html(html).unwrap_err(),
        MochaError::UnsupportedFeature(_)
    ));
}

#[test]
fn js_click_listener_changes_dom_and_prevent_default_suppresses_navigation() {
    // Build the document, run its script to register a click listener, then
    // dispatch a click programmatically (there is no real window).
    let html = r#"<html><body><a id="link" href="/next.html">Click me</a>
        <script>
          let link = document.getElementById("link");
          link.addEventListener("click", function (event) {
            event.preventDefault();
            link.textContent = "Clicked without navigation";
          });
        </script></body></html>"#;
    let document: Document = mocha_html::parse_html(html).unwrap();
    let shared = Rc::new(RefCell::new(document));
    let scripts = collect_inline_scripts(&shared.borrow()).unwrap();
    let mut runtime = DomRuntime::new(shared.clone());
    for source in &scripts {
        runtime.run_script(source).unwrap();
    }
    let link = shared.borrow().get_element_by_id("link").unwrap().unwrap();

    let proceed = runtime.dispatch_event("click", link).unwrap();
    assert!(
        !proceed,
        "preventDefault should suppress the anchor's navigation"
    );
    assert_eq!(
        shared.borrow().text_content(link).unwrap(),
        "Clicked without navigation"
    );
}

#[test]
fn timer_callback_mutates_dom_during_render() {
    // timer.html-style: a zero-delay timer runs after scripts and is reflected in
    // the final display list.
    let html = r#"<html><body><p id="s">waiting</p>
        <script>
          let s = document.getElementById("s");
          setTimeout(function () { s.textContent = "done"; }, 0);
        </script></body></html>"#;
    let texts = drawn_text(&run_html(html).unwrap());
    assert!(texts.contains(&"done".to_string()));
    assert!(!texts.contains(&"waiting".to_string()));
}
