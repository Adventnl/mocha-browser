//! Crash / robustness corpus (Milestone 20).
//!
//! Feeds deterministic malformed HTML, CSS, JS, and URL inputs to the parsers
//! and engine and asserts only one thing: **they never panic, never overflow the
//! stack, and never hang.** A malformed input must come back as `Ok` (lenient
//! recovery) or a clear `Err(MochaError)` — never a crash.
//!
//! The corpora live under `tests/corpus/{html,css,js,url}` and are read at
//! runtime, so adding a regression case is just dropping in a file. A handful of
//! generated torture cases (deep nesting, large text, infinite loops) are
//! included inline.
//!
//! This is not a fuzzer and makes no claim of exhaustiveness; it is a fast,
//! no-nightly safety net that runs inside `cargo test --all`.

use std::fs;
use std::path::PathBuf;

use mocha_error::MochaError;

fn corpus_dir(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/corpus")).join(name)
}

/// All `(filename, contents)` in a corpus directory, sorted for determinism.
fn read_corpus(name: &str) -> Vec<(String, String)> {
    let dir = corpus_dir(name);
    let mut entries: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read corpus {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path.is_file() {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            let body = fs::read_to_string(&path).ok()?;
            Some((name, body))
        })
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "corpus {name} is empty");
    entries
}

/// Every result is acceptable as long as it is a *result* (no panic/hang). A
/// clear `MochaError` is the expected outcome for most malformed inputs.
fn assert_clean<T>(label: &str, result: Result<T, MochaError>) {
    // Reaching here at all means the call returned rather than panicking. Touch
    // the error so the variant is exercised; both Ok and Err are acceptable.
    if let Err(error) = result {
        let message = error.to_string();
        assert!(!message.is_empty(), "{label}: empty error message");
    }
}

#[test]
fn html_corpus_never_panics() {
    for (name, source) in read_corpus("html") {
        assert_clean(&format!("html/{name}"), mocha_html::parse_html(&source));
    }
}

#[test]
fn css_corpus_never_panics() {
    for (name, source) in read_corpus("css") {
        assert_clean(&format!("css/{name}"), mocha_css::parse_stylesheet(&source));
        // Selector lists and inline styles are separate entry points worth hitting.
        assert_clean(
            &format!("css-selectors/{name}"),
            mocha_css::parse_selector_list(&source),
        );
        assert_clean(
            &format!("css-inline/{name}"),
            mocha_css::parse_inline_style(&source),
        );
    }
}

#[test]
fn js_corpus_never_panics() {
    for (name, source) in read_corpus("js") {
        let mut runtime = mocha_js::JsRuntime::new();
        assert_clean(&format!("js/{name}"), runtime.eval(&source));
    }
}

#[test]
fn url_corpus_never_panics() {
    let (_, body) = read_corpus("url")
        .into_iter()
        .find(|(name, _)| name == "urls.txt")
        .expect("url corpus has urls.txt");
    for line in body.lines() {
        assert_clean(&format!("url/{line:?}"), mocha_url::Url::parse(line));
    }
}

// === generated torture cases ===============================================

#[test]
fn deeply_nested_html_does_not_overflow() {
    // 500 levels deep: the arena-based tree builder must handle this without a
    // stack overflow, either succeeding or erroring clearly.
    let depth = 500;
    let mut html = String::from("<html><body>");
    html.push_str(&"<div>".repeat(depth));
    html.push_str("deep");
    html.push_str(&"</div>".repeat(depth));
    html.push_str("</body></html>");
    assert_clean("deep-html", mocha_html::parse_html(&html));
}

#[test]
fn large_text_node_is_handled() {
    let mut html = String::from("<html><body><p>");
    html.push_str(&"lorem ipsum ".repeat(20_000)); // ~240 KB of text
    html.push_str("</p></body></html>");
    assert_clean("large-text", mocha_html::parse_html(&html));
}

#[test]
fn nested_js_expression_does_not_overflow() {
    // 64 nested parens: deep but within a sane recursive-descent budget.
    let depth = 64;
    let source = format!("{}1{};", "(".repeat(depth), ")".repeat(depth));
    let mut runtime = mocha_js::JsRuntime::new();
    assert_clean("nested-js", runtime.eval(&source));
}

#[test]
fn infinite_js_loop_hits_the_step_limit() {
    let mut runtime = mocha_js::JsRuntime::new();
    let error = runtime
        .eval("while (true) { }")
        .expect_err("an infinite loop must be stopped by the step budget");
    assert!(
        matches!(error, MochaError::JavaScript(message) if message.contains("step limit")),
        "expected a step-limit error, got a different failure"
    );
}

#[test]
fn malformed_inputs_through_full_engine_do_not_panic() {
    // Run a few malformed HTML documents through the whole in-memory pipeline.
    for source in [
        "<html><body><p>unclosed",
        "<<<>>> garbage <p<>>",
        "<html><body><style>p{color:</style><p>x</p></body></html>",
        "<html><body><script>var x = </script></body></html>",
    ] {
        assert_clean(
            "engine",
            mocha_engine::render_html(source, &mocha_engine::RenderOptions::default()),
        );
    }
}
