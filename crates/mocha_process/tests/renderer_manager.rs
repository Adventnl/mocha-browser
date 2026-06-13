use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use mocha_process::{RendererManager, RendererProcess};
use mocha_sandbox::{prepare_document, RendererSandboxPolicy, SandboxStatus};
use mocha_url::Url;

fn renderer_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mocha_renderer"))
}

fn example_path(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate is under crates/mocha_process")
        .join("examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn spawn_ping_and_shutdown_renderer() {
    let mut renderer = RendererProcess::spawn_with_path(renderer_bin()).unwrap();
    renderer.ping().unwrap();
    assert!(renderer.is_alive());
    renderer.shutdown().unwrap();
    assert!(!renderer.is_alive());
}

#[test]
fn render_basic_document_in_child_process() {
    let mut renderer = RendererProcess::spawn_with_path(renderer_bin()).unwrap();
    let page = renderer
        .render_document(&example_path("basic/index.html"), 800, 600)
        .unwrap();
    assert!(page.final_url.as_deref().unwrap().starts_with("file://"));
    assert!(page.document_height > 0.0);
    assert!(page.display_list_len > 0);
    renderer.shutdown().unwrap();
}

#[test]
fn render_error_returns_error_not_panic() {
    // A missing local file is a deterministic, offline render failure: the child
    // returns a "renderer error" rather than panicking, and stays alive. (A real
    // URL is no longer a reliable error source now that the engine renders real
    // pages instead of rejecting them — Milestone 23.)
    let mut renderer = RendererProcess::spawn_with_path(renderer_bin()).unwrap();
    let err = renderer
        .render_document(&example_path("does/not/exist.html"), 800, 600)
        .unwrap_err();
    assert!(err.to_string().contains("renderer error"));
    assert!(renderer.is_alive());
    renderer.shutdown().unwrap();
}

#[test]
fn crash_is_detected_and_manager_respawns() {
    let mut manager = RendererManager::spawn_with_path(renderer_bin()).unwrap();
    manager.renderer_mut().ping().unwrap();
    manager.renderer_mut().crash_for_test().unwrap();
    for _ in 0..20 {
        if !manager.renderer_mut().is_alive() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!manager.renderer_mut().is_alive());

    manager.respawn().unwrap();
    manager.renderer_mut().ping().unwrap();
    manager.renderer_mut().shutdown().unwrap();
}

#[test]
fn sandboxed_renderer_renders_prepared_document() {
    let policy = RendererSandboxPolicy::default_renderer();
    let (mut manager, status) =
        RendererManager::spawn_sandboxed_with_path(renderer_bin(), &policy).unwrap();
    assert_eq!(status, SandboxStatus::CapabilityRestrictedOnly);

    let final_url = Url::parse("file:///tmp/prepared.html").unwrap();
    let page = manager
        .renderer_mut()
        .render_prepared_document(
            prepare_document(
                Some(&final_url),
                "<html><body><p>prepared</p></body></html>",
            ),
            800,
            600,
        )
        .unwrap();
    assert_eq!(page.final_url.as_deref(), Some("file:///tmp/prepared.html"));
    assert!(page.display_list_len > 0);
    manager.renderer_mut().shutdown().unwrap();
}

#[test]
fn sandboxed_renderer_rejects_direct_document_load() {
    let policy = RendererSandboxPolicy::default_renderer();
    let (mut manager, _status) =
        RendererManager::spawn_sandboxed_with_path(renderer_bin(), &policy).unwrap();
    let err = manager
        .renderer_mut()
        .render_document(&example_path("basic/index.html"), 800, 600)
        .unwrap_err();
    assert!(err.to_string().contains("sandbox violation"));
    assert!(manager.renderer_mut().is_alive());
    manager.renderer_mut().shutdown().unwrap();
}
