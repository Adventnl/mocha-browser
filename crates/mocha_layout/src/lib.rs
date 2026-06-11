//! A small but real layout engine for Mocha Browser.
//!
//! Layout consumes a [`StyledNode`] tree (computed style lives in `mocha_style`;
//! this crate does **no CSS parsing** and does not depend on `mocha_dom`). It
//! implements:
//!
//! - **block formatting** — block-level children stack vertically with a simple
//!   margin/border/padding box model (no margin collapse, floats, or positioning),
//! - **inline formatting** — runs of inline content are broken into [line
//!   boxes](LayoutBoxKind::LineBox) of [text runs](LayoutBoxKind::TextRun) with
//!   word wrapping, so text and `<span>`s share a line until the width runs out,
//! - **anonymous block boxes** — inline content sitting among block siblings is
//!   wrapped in [`LayoutBoxKind::AnonymousBlock`].
//!
//! Text is measured by estimate (`chars * font_size * 0.6`), not real font
//! metrics; line height is `max_font_size * 1.2`.

mod block;
mod box_tree;
mod context;
mod debug;
mod geometry;
mod inline;
mod line;

pub use box_tree::{LayoutBox, LayoutBoxKind};
pub use context::{LayoutViewport, DEFAULT_VIEWPORT_HEIGHT, DEFAULT_VIEWPORT_WIDTH};
pub use debug::format_layout_tree;
pub use geometry::{EdgeSizes, Rect};
pub use mocha_style::{Color, ControlBox, NodeId};

use mocha_error::{MochaError, MochaResult};
use mocha_style::{Display, StyledNode};

/// Build a layout tree from a styled tree for the given `viewport`.
///
/// The styled root (the document node) is laid out as a block container filling
/// the viewport width. `display: none` nodes (and their subtrees) produce no
/// boxes.
pub fn build_layout_tree(
    styled_root: &StyledNode,
    viewport: LayoutViewport,
) -> MochaResult<LayoutBox> {
    if styled_root.style.display == Display::None {
        return Err(MochaError::Layout(
            "the document root has display:none and produced no layout box".to_string(),
        ));
    }
    Ok(block::layout_block(styled_root, 0.0, 0.0, viewport.width))
}

/// Find the DOM node at viewport point `(x, y)`.
///
/// Returns the deepest box containing the point that has a `node_id` (so text
/// runs map to their source text node). `display: none` nodes produce no box and
/// are never hit. This is a minimal bridge: there is no z-index, transform,
/// clipping, scrolling, or `pointer-events` handling.
pub fn hit_test(root: &LayoutBox, x: f32, y: f32) -> Option<NodeId> {
    // Recurse into children first (deepest match wins); fall back to this box.
    for child in root.children.iter().rev() {
        if let Some(hit) = hit_test(child, x, y) {
            return Some(hit);
        }
    }
    if root.node_id.is_some() && contains(&root.rect, x, y) {
        return root.node_id;
    }
    None
}

fn contains(rect: &Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_style::{ComputedStyle, Display, EdgeSizes as StyleEdges, StyledNode};

    // --- builders -----------------------------------------------------------

    fn block_style() -> ComputedStyle {
        let mut style = ComputedStyle::initial();
        style.display = Display::Block;
        style
    }

    fn element(node_id: usize, style: ComputedStyle, children: Vec<StyledNode>) -> StyledNode {
        StyledNode {
            node_id: NodeId(node_id),
            text: None,
            style,
            replaced: None,
            control: None,
            children,
        }
    }

    fn text(node_id: usize, content: &str, font_size: f32, color: Color) -> StyledNode {
        let mut style = ComputedStyle::initial();
        style.display = Display::Inline;
        style.font_size = font_size;
        style.color = color;
        StyledNode {
            node_id: NodeId(node_id),
            text: Some(content.to_string()),
            style,
            replaced: None,
            control: None,
            children: Vec::new(),
        }
    }

    fn inline_span(
        node_id: usize,
        font_size: f32,
        color: Color,
        children: Vec<StyledNode>,
    ) -> StyledNode {
        let mut style = ComputedStyle::initial();
        style.display = Display::Inline;
        style.font_size = font_size;
        style.color = color;
        StyledNode {
            node_id: NodeId(node_id),
            text: None,
            style,
            replaced: None,
            control: None,
            children,
        }
    }

    fn layout(root: &StyledNode, width: f32) -> LayoutBox {
        build_layout_tree(
            root,
            LayoutViewport {
                width,
                height: 600.0,
            },
        )
        .unwrap()
    }

    // --- tree walking helpers ----------------------------------------------

    fn collect<'a>(node: &'a LayoutBox, out: &mut Vec<&'a LayoutBox>) {
        out.push(node);
        for child in &node.children {
            collect(child, out);
        }
    }

    fn all_boxes(root: &LayoutBox) -> Vec<&LayoutBox> {
        let mut out = Vec::new();
        collect(root, &mut out);
        out
    }

    fn line_boxes(root: &LayoutBox) -> Vec<&LayoutBox> {
        all_boxes(root)
            .into_iter()
            .filter(|b| b.kind == LayoutBoxKind::LineBox)
            .collect()
    }

    fn text_runs(root: &LayoutBox) -> Vec<&LayoutBox> {
        all_boxes(root)
            .into_iter()
            .filter(|b| matches!(b.kind, LayoutBoxKind::TextRun(_)))
            .collect()
    }

    fn run_text(b: &LayoutBox) -> &str {
        match &b.kind {
            LayoutBoxKind::TextRun(text) => text,
            _ => panic!("not a text run"),
        }
    }

    fn find(root: &LayoutBox, id: usize) -> Option<&LayoutBox> {
        all_boxes(root)
            .into_iter()
            .find(|b| b.node_id == Some(NodeId(id)))
    }

    // --- inline formatting --------------------------------------------------

    #[test]
    fn inline_text_and_span_share_a_line_when_width_allows() {
        // <p>Hello <span>red</span> world</p>, wide viewport.
        let p = element(
            1,
            block_style(),
            vec![
                text(2, "Hello ", 16.0, Color::BLACK),
                inline_span(
                    3,
                    16.0,
                    Color::rgb(255, 0, 0),
                    vec![text(4, "red", 16.0, Color::rgb(255, 0, 0))],
                ),
                text(5, " world", 16.0, Color::BLACK),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);

        assert_eq!(line_boxes(&tree).len(), 1, "all three runs fit on one line");
        let runs = text_runs(&tree);
        let texts: Vec<&str> = runs.iter().map(|r| run_text(r)).collect();
        assert_eq!(texts, vec!["Hello", "red", "world"]);
        // They share the same line top.
        assert!(runs.iter().all(|r| r.rect.y == runs[0].rect.y));
        // Runs are placed left to right in order.
        assert!(runs[0].rect.right() <= runs[1].rect.x);
        assert!(runs[1].rect.right() <= runs[2].rect.x);
    }

    #[test]
    fn span_color_affects_only_span_text() {
        let p = element(
            1,
            block_style(),
            vec![
                text(2, "Hello ", 16.0, Color::BLACK),
                inline_span(
                    3,
                    16.0,
                    Color::rgb(255, 0, 0),
                    vec![text(4, "red", 16.0, Color::rgb(255, 0, 0))],
                ),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let runs = text_runs(&tree);
        assert_eq!(runs[0].color, Color::BLACK);
        assert_eq!(runs[1].color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn nested_spans_inherit_and_override_color() {
        // outer blue span containing text then an inner red span.
        let inner = inline_span(
            4,
            16.0,
            Color::rgb(255, 0, 0),
            vec![text(5, "red", 16.0, Color::rgb(255, 0, 0))],
        );
        let outer = inline_span(
            3,
            16.0,
            Color::rgb(0, 0, 255),
            vec![text(6, "blue ", 16.0, Color::rgb(0, 0, 255)), inner],
        );
        let p = element(1, block_style(), vec![outer]);
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let runs = text_runs(&tree);
        assert_eq!(run_text(runs[0]), "blue");
        assert_eq!(runs[0].color, Color::rgb(0, 0, 255));
        assert_eq!(run_text(runs[1]), "red");
        assert_eq!(runs[1].color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn larger_inline_span_increases_line_height() {
        let p = element(
            1,
            block_style(),
            vec![
                text(2, "Normal ", 16.0, Color::BLACK),
                inline_span(
                    3,
                    32.0,
                    Color::BLACK,
                    vec![text(4, "large", 32.0, Color::BLACK)],
                ),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let line = line_boxes(&tree)[0];
        // Line height uses the max font (32 * 1.2 = 38.4 -> 38), not 16.
        assert_eq!(line.rect.height, (32.0 * 1.2_f32).round());
    }

    // --- word wrapping ------------------------------------------------------

    fn paragraph_of(words: usize) -> StyledNode {
        let sentence = vec!["word"; words].join(" ");
        let p = element(
            1,
            block_style(),
            vec![text(2, &sentence, 16.0, Color::BLACK)],
        );
        element(0, block_style(), vec![p])
    }

    #[test]
    fn narrow_viewport_produces_more_lines_than_wide() {
        let wide = layout(&paragraph_of(12), 800.0);
        let narrow = layout(&paragraph_of(12), 120.0);
        assert!(
            line_boxes(&narrow).len() > line_boxes(&wide).len(),
            "narrow={} wide={}",
            line_boxes(&narrow).len(),
            line_boxes(&wide).len()
        );
    }

    #[test]
    fn word_order_is_preserved_after_wrapping() {
        let root = element(
            0,
            block_style(),
            vec![element(
                1,
                block_style(),
                vec![text(2, "alpha beta gamma delta", 16.0, Color::BLACK)],
            )],
        );
        let tree = layout(&root, 80.0);
        let texts: Vec<&str> = text_runs(&tree).iter().map(|r| run_text(r)).collect();
        assert_eq!(texts, vec!["alpha", "beta", "gamma", "delta"]);
        assert!(
            line_boxes(&tree).len() > 1,
            "should wrap onto several lines"
        );
    }

    #[test]
    fn long_word_overflows_without_crashing() {
        let root = element(
            0,
            block_style(),
            vec![element(
                1,
                block_style(),
                vec![text(
                    2,
                    "supercalifragilisticexpialidocious",
                    16.0,
                    Color::BLACK,
                )],
            )],
        );
        let tree = layout(&root, 50.0);
        let runs = text_runs(&tree);
        assert_eq!(runs.len(), 1);
        assert!(
            runs[0].rect.width > 50.0,
            "the long word overflows the line"
        );
    }

    #[test]
    fn line_boxes_stack_vertically() {
        let tree = layout(&paragraph_of(20), 120.0);
        let lines = line_boxes(&tree);
        for pair in lines.windows(2) {
            assert!(pair[1].rect.y >= pair[0].rect.bottom());
        }
    }

    // --- block formatting ---------------------------------------------------

    #[test]
    fn block_children_stack_vertically() {
        let a = element(1, block_style(), vec![text(3, "a", 16.0, Color::BLACK)]);
        let b = element(2, block_style(), vec![text(4, "b", 16.0, Color::BLACK)]);
        let root = element(0, block_style(), vec![a, b]);
        let tree = layout(&root, 800.0);
        let box_a = find(&tree, 1).unwrap();
        let box_b = find(&tree, 2).unwrap();
        assert!(box_b.rect.y >= box_a.rect.bottom());
    }

    #[test]
    fn padding_and_border_offset_child_position() {
        let mut parent = block_style();
        parent.padding = StyleEdges {
            top: 10.0,
            left: 12.0,
            ..StyleEdges::default()
        };
        parent.border_width = 2.0;
        let child = element(2, block_style(), vec![text(3, "x", 16.0, Color::BLACK)]);
        let root = element(0, block_style(), vec![element(1, parent, vec![child])]);
        let tree = layout(&root, 800.0);
        let child_box = find(&tree, 2).unwrap();
        assert_eq!(child_box.rect.x, 14.0); // border 2 + padding-left 12
        assert_eq!(child_box.rect.y, 12.0); // border 2 + padding-top 10
    }

    #[test]
    fn margin_affects_vertical_position() {
        let mut style = block_style();
        style.margin = StyleEdges {
            top: 25.0,
            ..StyleEdges::default()
        };
        let root = element(0, block_style(), vec![element(1, style, Vec::new())]);
        let tree = layout(&root, 800.0);
        assert_eq!(find(&tree, 1).unwrap().rect.y, 25.0);
    }

    #[test]
    fn auto_height_expands_to_fit_children_and_explicit_height_overrides() {
        let child = element(2, block_style(), vec![text(3, "x", 16.0, Color::BLACK)]);
        let auto = element(1, block_style(), vec![child.clone()]);
        let root = element(0, block_style(), vec![auto]);
        let tree = layout(&root, 800.0);
        let auto_box = find(&tree, 1).unwrap();
        assert!(auto_box.rect.height > 0.0);

        let mut fixed_style = block_style();
        fixed_style.height = Some(500.0);
        let fixed = element(1, fixed_style, vec![child]);
        let root = element(0, block_style(), vec![fixed]);
        let tree = layout(&root, 800.0);
        assert_eq!(find(&tree, 1).unwrap().rect.height, 500.0);
    }

    #[test]
    fn explicit_width_changes_wrapping_width() {
        let mut narrow = block_style();
        narrow.width = Some(80.0);
        let p = element(
            1,
            narrow,
            vec![text(2, "alpha beta gamma delta", 16.0, Color::BLACK)],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0); // wide viewport, but the block is 80px.
        assert!(
            line_boxes(&tree).len() > 1,
            "explicit width should force wrapping"
        );
    }

    // --- mixed inline/block content ----------------------------------------

    #[test]
    fn mixed_inline_and_block_uses_anonymous_blocks_in_order() {
        // <div> Intro <p>Block</p> Outro </div>
        let div = element(
            1,
            block_style(),
            vec![
                text(2, "Intro", 16.0, Color::BLACK),
                element(3, block_style(), vec![text(4, "Block", 16.0, Color::BLACK)]),
                text(5, "Outro", 16.0, Color::BLACK),
            ],
        );
        let root = element(0, block_style(), vec![div]);
        let tree = layout(&root, 800.0);

        let div_box = find(&tree, 1).unwrap();
        assert_eq!(div_box.children.len(), 3);
        assert_eq!(div_box.children[0].kind, LayoutBoxKind::AnonymousBlock);
        assert_eq!(div_box.children[1].kind, LayoutBoxKind::Block);
        assert_eq!(div_box.children[2].kind, LayoutBoxKind::AnonymousBlock);
        // Vertical order: intro above block above outro.
        assert!(div_box.children[0].rect.y < div_box.children[1].rect.y);
        assert!(div_box.children[1].rect.y < div_box.children[2].rect.y);
        // The block does not overlap the inline content above it.
        assert!(div_box.children[1].rect.y >= div_box.children[0].rect.bottom());
    }

    // --- display:none -------------------------------------------------------

    #[test]
    fn display_none_block_produces_no_box() {
        let mut hidden = block_style();
        hidden.display = Display::None;
        let root = element(
            0,
            block_style(),
            vec![element(
                1,
                hidden,
                vec![text(2, "hidden", 16.0, Color::BLACK)],
            )],
        );
        let tree = layout(&root, 800.0);
        assert!(find(&tree, 1).is_none());
        assert!(find(&tree, 2).is_none());
    }

    #[test]
    fn display_none_span_does_not_affect_wrapping() {
        let mut hidden = ComputedStyle::initial();
        hidden.display = Display::None;
        let p = element(
            1,
            block_style(),
            vec![
                text(2, "Hello ", 16.0, Color::BLACK),
                StyledNode {
                    node_id: NodeId(3),
                    text: None,
                    style: hidden,
                    replaced: None,
                    control: None,
                    children: vec![text(4, "INVISIBLE", 16.0, Color::BLACK)],
                },
                text(5, "world", 16.0, Color::BLACK),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let texts: Vec<&str> = text_runs(&tree).iter().map(|r| run_text(r)).collect();
        assert_eq!(texts, vec!["Hello", "world"]);
    }

    // --- form controls --------------------------------------------------------

    use mocha_style::ControlBox;

    fn control_box(control_type: &str, width: f32, height: f32) -> ControlBox {
        ControlBox {
            control_type: control_type.to_string(),
            value: None,
            checked: None,
            disabled: false,
            width,
            height,
        }
    }

    /// An inline styled node carrying a resolved control box (what the shell
    /// attaches for `<input>`/`<button>`/`<textarea>`/`<select>`).
    fn control_node(node_id: usize, control: ControlBox) -> StyledNode {
        let mut style = ComputedStyle::initial();
        style.display = Display::Inline;
        StyledNode {
            node_id: NodeId(node_id),
            text: None,
            style,
            replaced: None,
            control: Some(control),
            children: Vec::new(),
        }
    }

    fn find_control(root: &LayoutBox, id: usize) -> Option<&LayoutBox> {
        all_boxes(root)
            .into_iter()
            .find(|b| b.node_id == Some(NodeId(id)) && matches!(b.kind, LayoutBoxKind::Control(_)))
    }

    #[test]
    fn inline_control_creates_a_control_box_with_its_resolved_size() {
        let p = element(
            1,
            block_style(),
            vec![control_node(2, control_box("text", 160.0, 24.0))],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let control = find_control(&tree, 2).expect("control box exists");
        assert_eq!(control.rect.width, 160.0);
        assert_eq!(control.rect.height, 24.0);
    }

    #[test]
    fn control_shares_a_line_with_text_and_raises_line_height() {
        // "Search" <input 160x24> on one line: same line box, control taller
        // than the 16px text (19px line), so the line is 24px high.
        let p = element(
            1,
            block_style(),
            vec![
                text(2, "Search ", 16.0, Color::BLACK),
                control_node(3, control_box("text", 160.0, 24.0)),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);

        let lines = line_boxes(&tree);
        assert_eq!(lines.len(), 1, "text and control share a line");
        assert_eq!(lines[0].rect.height, 24.0, "control raises line height");
        let run = text_runs(&tree)[0];
        let control = find_control(&tree, 3).unwrap();
        assert_eq!(run.rect.y, control.rect.y, "same line top");
        assert!(
            run.rect.right() <= control.rect.x,
            "control placed after text"
        );
    }

    #[test]
    fn controls_wrap_to_the_next_line_when_width_runs_out() {
        let p = element(
            1,
            block_style(),
            vec![
                control_node(2, control_box("text", 160.0, 24.0)),
                control_node(3, control_box("text", 160.0, 24.0)),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 200.0);
        assert_eq!(line_boxes(&tree).len(), 2, "second control wraps");
        let first = find_control(&tree, 2).unwrap();
        let second = find_control(&tree, 3).unwrap();
        assert!(second.rect.y >= first.rect.bottom());
    }

    #[test]
    fn checkbox_control_box_keeps_its_square_size() {
        let p = element(
            1,
            block_style(),
            vec![control_node(2, control_box("checkbox", 13.0, 13.0))],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let control = find_control(&tree, 2).unwrap();
        assert_eq!((control.rect.width, control.rect.height), (13.0, 13.0));
    }

    #[test]
    fn block_level_control_lays_out_like_a_replaced_block() {
        let mut style = block_style();
        style.margin.top = 10.0;
        let control = StyledNode {
            node_id: NodeId(1),
            text: None,
            style,
            replaced: None,
            control: Some(control_box("textarea", 200.0, 80.0)),
            children: Vec::new(),
        };
        let root = element(0, block_style(), vec![control]);
        let tree = layout(&root, 800.0);
        let control = find_control(&tree, 1).unwrap();
        assert_eq!(control.rect.y, 10.0);
        assert_eq!((control.rect.width, control.rect.height), (200.0, 80.0));
    }

    #[test]
    fn display_none_control_produces_no_box() {
        let mut hidden = ComputedStyle::initial();
        hidden.display = Display::None;
        let control = StyledNode {
            node_id: NodeId(2),
            text: None,
            style: hidden,
            replaced: None,
            control: Some(control_box("text", 160.0, 24.0)),
            children: Vec::new(),
        };
        let p = element(1, block_style(), vec![control]);
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        assert!(find_control(&tree, 2).is_none());
    }

    #[test]
    fn control_is_hit_testable() {
        let p = element(
            1,
            block_style(),
            vec![control_node(2, control_box("text", 160.0, 24.0))],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        assert_eq!(hit_test(&tree, 10.0, 10.0), Some(NodeId(2)));
    }

    #[test]
    fn debug_dump_includes_control_kind() {
        let p = element(
            1,
            block_style(),
            vec![control_node(2, control_box("checkbox", 13.0, 13.0))],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        assert!(format_layout_tree(&tree).contains("Control checkbox"));
    }

    // --- inter-atom whitespace (Milestone 11 / Part A1) ---------------------

    #[test]
    fn whitespace_text_between_controls_adds_a_separating_space() {
        // <input> " " <input>: a whitespace-only text node between two controls
        // must keep them apart on the line.
        let p = element(
            1,
            block_style(),
            vec![
                control_node(2, control_box("text", 160.0, 24.0)),
                text(3, " ", 16.0, Color::BLACK),
                control_node(4, control_box("text", 160.0, 24.0)),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let first = find_control(&tree, 2).unwrap();
        let second = find_control(&tree, 4).unwrap();
        assert!(
            second.rect.x > first.rect.right(),
            "controls separated by whitespace must not touch ({} !> {})",
            second.rect.x,
            first.rect.right()
        );
    }

    #[test]
    fn whitespace_between_control_and_text_adds_a_space() {
        // <input> " Agree": checkbox then label text, separated by a space.
        let p = element(
            1,
            block_style(),
            vec![
                control_node(2, control_box("checkbox", 13.0, 13.0)),
                text(3, " Agree", 16.0, Color::BLACK),
            ],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let control = find_control(&tree, 2).unwrap();
        let run = text_runs(&tree)[0];
        assert!(
            run.rect.x > control.rect.right(),
            "text starts after the gap"
        );
    }

    #[test]
    fn whitespace_only_inline_group_between_blocks_emits_no_box() {
        // <div><p/>" "<p/></div>: the indentation-only text node between block
        // siblings must not become an anonymous block (no visible box/height).
        let div = element(
            1,
            block_style(),
            vec![
                element(2, block_style(), vec![text(5, "a", 16.0, Color::BLACK)]),
                text(3, " ", 16.0, Color::BLACK),
                element(4, block_style(), vec![text(6, "b", 16.0, Color::BLACK)]),
            ],
        );
        let root = element(0, block_style(), vec![div]);
        let tree = layout(&root, 800.0);
        let div_box = find(&tree, 1).unwrap();
        assert_eq!(
            div_box.children.len(),
            2,
            "only the two block children; the whitespace group is dropped"
        );
        assert!(div_box
            .children
            .iter()
            .all(|c| c.kind == LayoutBoxKind::Block));
    }

    #[test]
    fn leading_whitespace_text_does_not_create_visible_leading_space() {
        // " Hello": a leading space at the start of a line is ignored.
        let p = element(
            1,
            block_style(),
            vec![text(2, " Hello", 16.0, Color::BLACK)],
        );
        let root = element(0, block_style(), vec![p]);
        let tree = layout(&root, 800.0);
        let run = text_runs(&tree)[0];
        // The first run sits at the content origin (x == 0 here), not pushed right.
        assert_eq!(run.rect.x, 0.0);
    }

    // --- debug dump ---------------------------------------------------------

    #[test]
    fn hit_test_returns_deepest_node_and_misses_outside() {
        // root -> p(block, id 1) -> text "abcde" (id 2)
        let p = element(1, block_style(), vec![text(2, "abcde", 16.0, Color::BLACK)]);
        let root = element(0, block_style(), vec![p]);
        let layout = layout(&root, 800.0);

        // Inside the text run -> the text node (deepest), not the paragraph.
        assert_eq!(hit_test(&layout, 5.0, 5.0), Some(NodeId(2)));
        // Far outside the document -> nothing.
        assert_eq!(hit_test(&layout, 5000.0, 5000.0), None);
    }

    #[test]
    fn hit_test_in_empty_block_returns_the_block() {
        // A block with an explicit size but no text: the point hits the block.
        let mut sized = block_style();
        sized.height = Some(50.0);
        let root = element(0, block_style(), vec![element(1, sized, Vec::new())]);
        let layout = layout(&root, 800.0);
        assert_eq!(hit_test(&layout, 10.0, 10.0), Some(NodeId(1)));
    }

    #[test]
    fn hit_test_skips_display_none() {
        let mut hidden = block_style();
        hidden.display = Display::None;
        hidden.height = Some(50.0);
        let root = element(0, block_style(), vec![element(1, hidden, Vec::new())]);
        let layout = layout(&root, 800.0);
        // The hidden node produced no box, so it cannot be hit (root may match).
        assert_ne!(hit_test(&layout, 10.0, 10.0), Some(NodeId(1)));
    }

    #[test]
    fn debug_dump_includes_expected_kinds() {
        let div = element(
            1,
            block_style(),
            vec![
                text(2, "Intro", 16.0, Color::BLACK),
                element(
                    3,
                    block_style(),
                    vec![text(4, "Hello world", 16.0, Color::BLACK)],
                ),
            ],
        );
        let root = element(0, block_style(), vec![div]);
        let tree = layout(&root, 800.0);
        let dump = format_layout_tree(&tree);
        assert!(dump.contains("Block"));
        assert!(dump.contains("AnonymousBlock"));
        assert!(dump.contains("LineBox"));
        assert!(dump.contains("TextRun \"Hello\""));
        assert!(dump.contains("node=#"));
    }
}
