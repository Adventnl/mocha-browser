//! Integration tests for Milestone 4 networking + navigation, driven through the
//! shell and a localhost test server (no external network). Declared as a
//! `[[test]]` of `mocha_shell` (see its `Cargo.toml`).

use mocha_nav::NavigationController;
use mocha_net::test_server::{Reply, TestServer};
use mocha_net::{DefaultLoader, LoadRequest, ResourceLoader, ResourceType};
use mocha_paint::DisplayCommand;
use mocha_shell::{run_file, RunOptions};
use mocha_url::Url;

fn html_text(commands: &[DisplayCommand]) -> String {
    commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::DrawText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn shell_renders_http_html_from_local_server() {
    let server = TestServer::start(vec![(
        "/index.html".to_string(),
        Reply::Html("<html><body><h1>Net Mocha</h1></body></html>".to_string()),
    )]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert!(html_text(&commands).contains("Net Mocha"));
}

#[test]
fn shell_renders_redirected_html() {
    let server = TestServer::start(vec![
        (
            "/start".to_string(),
            Reply::Redirect {
                status: 302,
                location: "/dest.html".to_string(),
            },
        ),
        (
            "/dest.html".to_string(),
            Reply::Html("<html><body><p>arrived</p></body></html>".to_string()),
        ),
    ]);
    let commands = run_file(&server.url("/start")).unwrap();
    assert!(html_text(&commands).contains("arrived"));
}

#[test]
fn shell_redirect_loop_errors_clearly() {
    let server = TestServer::start(vec![(
        "/loop".to_string(),
        Reply::Redirect {
            status: 302,
            location: "/loop".to_string(),
        },
    )]);
    let error = run_file(&server.url("/loop")).unwrap_err();
    assert!(matches!(error, mocha_error::MochaError::Network(_)));
}

#[test]
fn shell_text_plain_is_rejected() {
    let server = TestServer::start(vec![(
        "/note.txt".to_string(),
        Reply::Text("not html".to_string()),
    )]);
    let error = run_file(&server.url("/note.txt")).unwrap_err();
    assert!(matches!(
        error,
        mocha_error::MochaError::UnsupportedFeature(_)
    ));
}

#[test]
fn dump_layout_over_http_no_cache() {
    let server = TestServer::start(vec![(
        "/p.html".to_string(),
        Reply::Html("<html><body><p>Hello world wide web</p></body></html>".to_string()),
    )]);
    let out = mocha_shell::render_request(
        &server.url("/p.html"),
        RunOptions {
            dump_layout: true,
            no_cache: true,
            ..RunOptions::default()
        },
    )
    .unwrap();
    assert!(out.contains("TextRun"));
}

#[test]
fn navigation_history_stores_final_redirect_url_and_caches() {
    // Drive the navigation controller directly against the test server.
    let server = TestServer::start(vec![
        (
            "/a".to_string(),
            Reply::RedirectToSelf {
                status: 301,
                path: "/a-final.html".to_string(),
            },
        ),
        (
            "/a-final.html".to_string(),
            Reply::Html("<html><body>A</body></html>".to_string()),
        ),
        (
            "/b.html".to_string(),
            Reply::Html("<html><body>B</body></html>".to_string()),
        ),
    ]);
    let mut nav = NavigationController::new(DefaultLoader::new());

    let a = nav
        .navigate(Url::parse(&server.url("/a")).unwrap())
        .unwrap();
    assert_eq!(a.final_url.path, "/a-final.html");
    assert_eq!(a.resource_type(), ResourceType::Html);

    nav.navigate(Url::parse(&server.url("/b.html")).unwrap())
        .unwrap();
    // Going back to A should serve the redirected final URL from cache.
    let back = nav.back().unwrap();
    assert_eq!(back.final_url.path, "/a-final.html");
    assert!(back.from_cache, "back navigation should hit the cache");
}

#[test]
fn file_url_renders_local_example() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/basic/index.html"
    );
    let file_url = format!("file://{path}");
    let commands = run_file(&file_url).unwrap();
    assert!(html_text(&commands).contains("Hello Mocha"));
}

#[test]
fn loader_trait_object_is_usable() {
    // Sanity: DefaultLoader is usable behind the ResourceLoader trait.
    let mut loader: Box<dyn ResourceLoader> = Box::new(DefaultLoader::new());
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/basic/index.html"
    );
    let response = loader
        .load(LoadRequest::get(Url::parse(path).unwrap()))
        .unwrap();
    assert_eq!(response.resource_type(), ResourceType::Html);
}
