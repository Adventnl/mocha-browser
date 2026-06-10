//! End-to-end pipeline tests for Mocha Browser.
//!
//! Covers both the original Milestone 1 example (`examples/basic`) and the
//! Milestone 2 styled example (`examples/styled`), verifying the full
//! HTML → CSS → style → layout → display-list pipeline. This file is declared as
//! a `[[test]]` of the `mocha_shell` crate (see its `Cargo.toml`), so it can use
//! that crate's dependencies directly.

use mocha_dom::NodeKind;
use mocha_paint::DisplayCommand;
use mocha_shell::{run_file, run_html};
use mocha_style::Color;

/// Absolute paths to the examples, independent of the test's working directory.
const BASIC_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/basic/index.html"
);
const STYLED_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/styled/index.html"
);

/// Collect every text string in the document, in depth-first order.
fn collect_text_nodes(document: &mocha_dom::Document) -> Vec<String> {
    document
        .traverse_depth_first(document.root_id())
        .unwrap()
        .into_iter()
        .filter_map(|id| match &document.node(id).unwrap().kind {
            NodeKind::Text(data) => Some(data.text.clone()),
            _ => None,
        })
        .collect()
}

fn drawn_text(commands: &[DisplayCommand]) -> Vec<(String, Color)> {
    commands
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, color, .. } => Some((text.clone(), *color)),
            _ => None,
        })
        .collect()
}

#[test]
fn basic_example_still_works() {
    let source = std::fs::read_to_string(BASIC_PATH).expect("basic example should exist");
    let document = mocha_html::parse_html(&source).expect("basic HTML should parse");
    let text_nodes = collect_text_nodes(&document);
    assert!(
        text_nodes.iter().any(|text| text == "Hello Mocha"),
        "expected a text node 'Hello Mocha', got {text_nodes:?}"
    );

    let commands = run_file(BASIC_PATH).expect("basic pipeline should succeed");
    let texts: Vec<String> = drawn_text(&commands).into_iter().map(|(t, _)| t).collect();
    assert!(
        texts.contains(&"Hello Mocha".to_string()),
        "display list should draw the headline, got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("This is the first local HTML page")),
        "display list should draw the first paragraph, got {texts:?}"
    );
}

#[test]
fn styled_example_works() {
    let commands = run_file(STYLED_PATH).expect("styled pipeline should succeed");
    let texts = drawn_text(&commands);
    assert!(
        texts.iter().any(|(text, _)| text == "Styled Mocha"),
        "expected the headline text, got {texts:?}"
    );
}

#[test]
fn style_tag_css_affects_display_list_and_is_not_painted() {
    let commands = run_file(STYLED_PATH).unwrap();
    let texts = drawn_text(&commands);

    // The class selector `.intro { color: blue }` colors the intro paragraph.
    let intro = texts
        .iter()
        .find(|(text, _)| text.contains("class selector"))
        .expect("intro paragraph should be drawn");
    assert_eq!(intro.1, Color::rgb(0, 0, 255), "intro should be blue");

    // The CSS source itself must never be painted as text.
    assert!(
        !texts
            .iter()
            .any(|(text, _)| text.contains("background-color")),
        "CSS text leaked into the display list: {texts:?}"
    );
}

#[test]
fn inline_style_beats_stylesheet() {
    let commands = run_file(STYLED_PATH).unwrap();
    let texts = drawn_text(&commands);
    let inline = texts
        .iter()
        .find(|(text, _)| text.contains("Inline style wins"))
        .expect("inline span should be drawn");
    assert_eq!(
        inline.1,
        Color::rgb(255, 0, 0),
        "inline color:red should win"
    );
}

#[test]
fn id_selector_beats_class_selector() {
    // #intro (id) should beat .intro (class) for the same element.
    let html = r#"<html><body><style>
        .intro { color: blue; }
        #intro { color: green; }
    </style><p id="intro" class="intro">Hi</p></body></html>"#;
    let commands = run_html(html).unwrap();
    let hi = drawn_text(&commands)
        .into_iter()
        .find(|(text, _)| text == "Hi")
        .expect("text should be drawn");
    assert_eq!(
        hi.1,
        Color::rgb(0, 128, 0),
        "id selector should win (green)"
    );
}

#[test]
fn descendant_selector_applies() {
    let commands = run_file(STYLED_PATH).unwrap();
    let texts = drawn_text(&commands);
    let descendant = texts
        .iter()
        .find(|(text, _)| text.contains("descendant selector"))
        .expect("descendant paragraph should be drawn");
    assert_eq!(descendant.1, Color::rgb(0, 128, 0), "div p should be green");
}

#[test]
fn highlight_background_creates_draw_rect() {
    let commands = run_file(STYLED_PATH).unwrap();
    // #highlight has background-color #f2e5d7.
    let has_bg = commands.iter().any(|command| {
        matches!(command, DisplayCommand::DrawRect { color, .. }
            if *color == Color::rgb(0xf2, 0xe5, 0xd7))
    });
    assert!(has_bg, "expected the highlight background DrawRect");
}

#[test]
fn unsupported_css_property_fails_clearly() {
    let html = "<html><body><style>p { float: left; }</style><p>x</p></body></html>";
    let error = run_html(html).unwrap_err();
    assert!(matches!(
        error,
        mocha_error::MochaError::UnsupportedFeature(_)
    ));
}

#[test]
fn unsupported_css_unit_fails_clearly() {
    let html = "<html><body><style>p { font-size: 2em; }</style><p>x</p></body></html>";
    let error = run_html(html).unwrap_err();
    assert!(matches!(
        error,
        mocha_error::MochaError::UnsupportedFeature(_)
    ));
}

#[test]
fn http_url_still_unsupported() {
    let error = run_file("http://example.com/index.html").unwrap_err();
    assert!(matches!(
        error,
        mocha_error::MochaError::UnsupportedFeature(_)
    ));
}
