//! End-to-end tests for Milestone 9: `<img>` loading, decoding, replaced-element
//! layout (intrinsic / attribute / CSS sizing), and `DrawImage` display commands.
//! Uses the checked-in PNG asset over local files and a local HTTP test server;
//! no public internet.

use std::fs;

use mocha_net::test_server::{Reply, TestServer};
use mocha_shell::{run_file, DisplayCommand};

/// Build a path to a file under `examples/` relative to this crate.
fn example(rel: &str) -> String {
    format!("{}/../../examples/{}", env!("CARGO_MANIFEST_DIR"), rel)
}

fn test_png() -> Vec<u8> {
    fs::read(example("assets/mocha-test.png")).expect("read test PNG asset")
}

/// All `DrawImage` commands as `(image_id, x, y, width, height)`.
fn draw_images(commands: &[DisplayCommand]) -> Vec<(usize, f32, f32, f32, f32)> {
    commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::DrawImage {
                image_id,
                x,
                y,
                width,
                height,
            } => Some((*image_id, *x, *y, *width, *height)),
            _ => None,
        })
        .collect()
}

fn command_index<F: Fn(&DisplayCommand) -> bool>(commands: &[DisplayCommand], pred: F) -> usize {
    commands.iter().position(pred).expect("command present")
}

#[test]
fn local_image_produces_drawimage_with_intrinsic_size() {
    // basic-image.html has no width/height, so the 16x16 intrinsic size is used.
    let commands = run_file(&example("images/basic-image.html")).unwrap();
    let images = draw_images(&commands);
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].3, images[0].4), (16.0, 16.0));
}

#[test]
fn width_and_height_attributes_override_intrinsic_size() {
    let commands = run_file(&example("images/sized-image.html")).unwrap();
    let images = draw_images(&commands);
    assert_eq!((images[0].3, images[0].4), (120.0, 80.0));
}

#[test]
fn inline_image_shares_line_with_text_in_document_order() {
    let commands = run_file(&example("images/inline-image.html")).unwrap();
    let images = draw_images(&commands);
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].3, images[0].4), (24.0, 24.0));

    // "before" <img> "after.": the image is painted between the two words.
    let before = command_index(
        &commands,
        |c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "before"),
    );
    let image = command_index(&commands, |c| matches!(c, DisplayCommand::DrawImage { .. }));
    let after = command_index(
        &commands,
        |c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "after."),
    );
    assert!(before < image && image < after);
    // Inline image shares the line with the text (same top).
    let text_y = commands.iter().find_map(|c| match c {
        DisplayCommand::DrawText { text, y, .. } if text == "before" => Some(*y),
        _ => None,
    });
    assert_eq!(text_y, Some(images[0].2));
}

#[test]
fn http_image_loads_and_decodes() {
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(r#"<html><body><img src="pic.png"></body></html>"#.to_string()),
        ),
        (
            "/pic.png".to_string(),
            Reply::Bytes {
                content_type: "image/png".to_string(),
                body: test_png(),
            },
        ),
    ]);
    let images = draw_images(&run_file(&server.url("/index.html")).unwrap());
    assert_eq!(images.len(), 1);
    assert_eq!((images[0].3, images[0].4), (16.0, 16.0));
}

#[test]
fn css_width_height_override_attributes() {
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(
                r#"<html><body><style>img { width: 50px; height: 40px; }</style><img src="pic.png" width="120" height="80"></body></html>"#
                    .to_string(),
            ),
        ),
        (
            "/pic.png".to_string(),
            Reply::Bytes {
                content_type: "image/png".to_string(),
                body: test_png(),
            },
        ),
    ]);
    let images = draw_images(&run_file(&server.url("/index.html")).unwrap());
    assert_eq!((images[0].3, images[0].4), (50.0, 40.0));
}

#[test]
fn block_images_stack_vertically() {
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(
                r#"<html><body><style>img { display: block; }</style><img src="p.png"><img src="p.png"></body></html>"#
                    .to_string(),
            ),
        ),
        (
            "/p.png".to_string(),
            Reply::Bytes {
                content_type: "image/png".to_string(),
                body: test_png(),
            },
        ),
    ]);
    let images = draw_images(&run_file(&server.url("/index.html")).unwrap());
    assert_eq!(images.len(), 2);
    // The second block image stacks below the first.
    assert!(images[1].2 >= images[0].2 + images[0].4);
}

// Milestone 23 fail-open: a `<img>` that cannot be loaded/decoded becomes a
// transparent placeholder and the page still renders (no abort). Earlier
// milestones aborted the whole render on these cases; that is no longer fatal.

#[test]
fn http_text_plain_image_is_skipped_not_fatal() {
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(r#"<html><body><img src="pic.png"><p>after</p></body></html>"#.to_string()),
        ),
        (
            "/pic.png".to_string(),
            Reply::Text("definitely not an image".to_string()),
        ),
    ]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "after")));
}

#[test]
fn missing_image_is_skipped_not_fatal() {
    let server = TestServer::start(vec![(
        "/index.html".to_string(),
        Reply::Html(r#"<html><body><img src="gone.png"><p>after</p></body></html>"#.to_string()),
    )]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "after")));
}

#[test]
fn missing_src_image_is_skipped_not_fatal() {
    let server = TestServer::start(vec![(
        "/index.html".to_string(),
        Reply::Html(r#"<html><body><img alt="no source"><p>after</p></body></html>"#.to_string()),
    )]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "after")));
}

#[test]
fn corrupt_image_is_skipped_not_fatal() {
    let server = TestServer::start(vec![
        (
            "/index.html".to_string(),
            Reply::Html(r#"<html><body><img src="pic.png"><p>after</p></body></html>"#.to_string()),
        ),
        (
            "/pic.png".to_string(),
            Reply::Bytes {
                content_type: "image/png".to_string(),
                body: b"\x89PNG\r\n\x1a\n not a real png".to_vec(),
            },
        ),
    ]);
    let commands = run_file(&server.url("/index.html")).unwrap();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "after")));
}
