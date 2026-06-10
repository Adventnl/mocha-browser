//! End-to-end pipeline tests for Mocha Browser.
//!
//! Covers the basic example (`examples/basic`), the styled example
//! (`examples/styled`), and the Milestone 3 layout examples (`examples/layout`),
//! verifying the full HTML → CSS → style → layout → display-list pipeline. This
//! file is declared as a `[[test]]` of the `mocha_shell` crate (see its
//! `Cargo.toml`), so it can use that crate's dependencies directly.
//!
//! NOTE: inline layout now splits text into per-word [`DisplayCommand::DrawText`]
//! runs, so phrase assertions reconstruct text by joining run strings with
//! spaces rather than expecting one command per text node.

use mocha_dom::NodeKind;
use mocha_paint::DisplayCommand;
use mocha_shell::{dump_layout_file, run_file, run_html};
use mocha_style::Color;

const BASIC_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/basic/index.html"
);
const STYLED_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/styled/index.html"
);
const ARTICLE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/layout/article.html"
);
const INLINE_WRAP_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/layout/inline-wrap.html"
);
const BOX_MODEL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/layout/box-model.html"
);

/// All drawn text runs as (text, color), in paint order.
fn drawn_runs(commands: &[DisplayCommand]) -> Vec<(String, Color)> {
    commands
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, color, .. } => Some((text.clone(), *color)),
            _ => None,
        })
        .collect()
}

/// All drawn text joined with single spaces, for phrase `contains` checks.
fn joined_text(commands: &[DisplayCommand]) -> String {
    drawn_runs(commands)
        .into_iter()
        .map(|(text, _)| text)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The color of the first run whose text exactly equals `word`.
fn color_of_word(commands: &[DisplayCommand], word: &str) -> Color {
    drawn_runs(commands)
        .into_iter()
        .find(|(text, _)| text == word)
        .unwrap_or_else(|| panic!("no text run equal to {word:?}"))
        .1
}

#[test]
fn basic_example_still_works() {
    let source = std::fs::read_to_string(BASIC_PATH).expect("basic example should exist");
    let document = mocha_html::parse_html(&source).expect("basic HTML should parse");
    let text_present = document
        .traverse_depth_first(document.root_id())
        .unwrap()
        .into_iter()
        .any(|id| {
            matches!(&document.node(id).unwrap().kind,
            NodeKind::Text(data) if data.text.contains("Hello Mocha"))
        });
    assert!(text_present, "DOM should contain the headline text");

    let commands = run_file(BASIC_PATH).expect("basic pipeline should succeed");
    let joined = joined_text(&commands);
    assert!(joined.contains("Hello Mocha"), "got: {joined}");
    assert!(
        joined.contains("This is the first local HTML page"),
        "got: {joined}"
    );
}

#[test]
fn styled_example_works() {
    let commands = run_file(STYLED_PATH).expect("styled pipeline should succeed");
    assert!(joined_text(&commands).contains("Styled Mocha"));
}

#[test]
fn style_tag_css_affects_display_list_and_is_not_painted() {
    let commands = run_file(STYLED_PATH).unwrap();
    // The class selector `.intro { color: blue }` colors the intro paragraph,
    // which is the only paragraph containing the word "class".
    assert_eq!(color_of_word(&commands, "class"), Color::rgb(0, 0, 255));
    // The CSS source itself must never be painted.
    assert!(
        !joined_text(&commands).contains("background-color"),
        "CSS text leaked into the display list"
    );
}

#[test]
fn inline_style_beats_stylesheet() {
    let commands = run_file(STYLED_PATH).unwrap();
    // The span "Inline style wins here." has inline color:red.
    assert_eq!(color_of_word(&commands, "wins"), Color::rgb(255, 0, 0));
}

#[test]
fn id_selector_beats_class_selector() {
    let html = r#"<html><body><style>
        .intro { color: blue; }
        #intro { color: green; }
    </style><p id="intro" class="intro">Hi</p></body></html>"#;
    let commands = run_html(html).unwrap();
    assert_eq!(color_of_word(&commands, "Hi"), Color::rgb(0, 128, 0));
}

#[test]
fn descendant_selector_applies() {
    let commands = run_file(STYLED_PATH).unwrap();
    // Only the descendant paragraph contains the word "descendant"; `div p` is green.
    assert_eq!(
        color_of_word(&commands, "descendant"),
        Color::rgb(0, 128, 0)
    );
}

#[test]
fn highlight_background_creates_draw_rect() {
    let commands = run_file(STYLED_PATH).unwrap();
    let has_bg = commands.iter().any(|command| {
        matches!(command, DisplayCommand::DrawRect { color, .. }
            if *color == Color::rgb(0xf2, 0xe5, 0xd7))
    });
    assert!(has_bg, "expected the highlight background DrawRect");
}

#[test]
fn span_text_shares_a_line_with_surrounding_text() {
    // "Hello", "red", "world" should all be drawn at the same y (one line).
    let html = "<html><body><p>Hello \
                <span style=\"color: red;\">red</span> world</p></body></html>";
    let commands = run_html(html).unwrap();
    let ys: Vec<f32> = commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::DrawText { y, .. } => Some(*y),
            _ => None,
        })
        .collect();
    assert_eq!(ys.len(), 3, "expected three word runs");
    assert!(ys.iter().all(|y| *y == ys[0]), "all runs share one line");
    assert_eq!(color_of_word(&commands, "red"), Color::rgb(255, 0, 0));
}

#[test]
fn long_paragraph_wraps_in_layout_dump() {
    // The article example contains a long wrapping paragraph; its layout dump
    // must contain several line boxes.
    let dump = dump_layout_file(ARTICLE_PATH).unwrap();
    let line_boxes = dump.matches("LineBox").count();
    assert!(
        line_boxes >= 2,
        "expected multiple line boxes, dump:\n{dump}"
    );
    assert!(dump.contains("TextRun"));
}

#[test]
fn layout_examples_run() {
    for path in [ARTICLE_PATH, INLINE_WRAP_PATH, BOX_MODEL_PATH] {
        let commands = run_file(path).unwrap_or_else(|e| panic!("{path} failed: {e}"));
        assert!(!commands.is_empty(), "{path} produced no commands");
    }
}

#[test]
fn inline_wrap_example_has_multiple_lines_and_colored_spans() {
    let commands = run_file(INLINE_WRAP_PATH).unwrap();
    let dump = dump_layout_file(INLINE_WRAP_PATH).unwrap();
    assert!(
        dump.matches("LineBox").count() >= 2,
        "inline-wrap should wrap, dump:\n{dump}"
    );
    // It should contain at least one non-black colored run from a span.
    let has_color = drawn_runs(&commands)
        .iter()
        .any(|(_, color)| *color != Color::BLACK);
    assert!(has_color, "expected a colored span run");
}

#[test]
fn box_model_example_emits_backgrounds_and_borders() {
    let commands = run_file(BOX_MODEL_PATH).unwrap();
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawRect { .. })),
        "expected a background rect"
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, DisplayCommand::DrawBorder { .. })),
        "expected a border"
    );
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
