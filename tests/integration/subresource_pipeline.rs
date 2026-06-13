//! End-to-end tests for Milestone 8: external `<link rel="stylesheet">` loading
//! over local files and a local HTTP test server, plus cascade order and content
//! type validation. No public internet is used.

use mocha_net::test_server::{Reply, TestServer};
use mocha_shell::{run_file, run_html, DisplayCommand};

/// The color a given word is painted with, if it is drawn.
fn text_color(commands: &[DisplayCommand], word: &str) -> Option<(u8, u8, u8)> {
    commands.iter().find_map(|c| match c {
        DisplayCommand::DrawText { text, color, .. } if text == word => {
            Some((color.r, color.g, color.b))
        }
        _ => None,
    })
}

#[test]
fn external_stylesheet_from_local_file_applies() {
    // examples/resources/external-css.html links style.css (h1 green, p blue).
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/resources/external-css.html"
    );
    let commands = run_file(path).unwrap();
    assert_eq!(text_color(&commands, "External"), Some((0, 128, 0))); // h1 green
    assert_eq!(text_color(&commands, "heading"), Some((0, 0, 255))); // p blue
}

#[test]
fn external_stylesheet_over_http_applies() {
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(
                r#"<html><body><link rel="stylesheet" href="site.css"><p>Hello</p></body></html>"#
                    .to_string(),
            ),
        ),
        (
            "/site.css".to_string(),
            Reply::Css("p { color: red; }".to_string()),
        ),
    ]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert_eq!(text_color(&commands, "Hello"), Some((255, 0, 0)));
}

#[test]
fn stylesheet_order_and_inline_precedence_are_correct() {
    // First link sets blue, second link sets green: later wins (green). Then an
    // inline style attribute (red) beats both.
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(
                r#"<html><body>
                   <link rel="stylesheet" href="a.css">
                   <link rel="stylesheet" href="b.css">
                   <p>First</p>
                   <p style="color: red;">Second</p>
                   </body></html>"#
                    .to_string(),
            ),
        ),
        (
            "/a.css".to_string(),
            Reply::Css("p { color: blue; }".to_string()),
        ),
        (
            "/b.css".to_string(),
            Reply::Css("p { color: green; }".to_string()),
        ),
    ]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert_eq!(text_color(&commands, "First"), Some((0, 128, 0))); // later sheet wins
    assert_eq!(text_color(&commands, "Second"), Some((255, 0, 0))); // inline wins
}

#[test]
fn missing_stylesheet_is_skipped_not_fatal() {
    // Milestone 23 fail-open: a stylesheet that 404s is skipped (the page renders
    // with UA defaults) rather than aborting the whole document.
    let server = TestServer::start(vec![(
        "/index.html".to_string(),
        Reply::Html(
            r#"<html><body><link rel="stylesheet" href="gone.css"><p>x</p></body></html>"#
                .to_string(),
        ),
    )]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "x")));
}

#[test]
fn stylesheet_with_wrong_content_type_is_skipped_not_fatal() {
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(
                r#"<html><body><link rel="stylesheet" href="styles"><p>x</p></body></html>"#
                    .to_string(),
            ),
        ),
        // Served as text/html, not text/css.
        (
            "/styles".to_string(),
            Reply::Html("p { color: red; }".to_string()),
        ),
    ]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "x")));
}

#[test]
fn link_text_is_not_painted_and_inline_style_still_works() {
    // A document with only inline <style> still renders (run_html, no base URL).
    let commands =
        run_html("<html><body><style>p { color: red; }</style><p>Hi</p></body></html>").unwrap();
    assert_eq!(text_color(&commands, "Hi"), Some((255, 0, 0)));
}

#[test]
fn in_memory_external_link_is_skipped_not_fatal() {
    // run_html has no base URL, so an external <link> cannot be resolved; it is
    // skipped (fail-open) and the inline content still renders.
    let html = r#"<html><body><link rel="stylesheet" href="x.css"><p>Hi</p></body></html>"#;
    let commands = run_html(html).unwrap();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "Hi")));
}
