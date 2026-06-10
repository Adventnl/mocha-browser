//! A deliberately tiny box-model layout engine for Mocha Browser.
//!
//! Layout consumes a [`StyledNode`] tree (computed style lives in `mocha_style`;
//! this crate does **no CSS parsing**). It is still vertical-stacking only: every
//! box that produces output is placed on its own line, top to bottom. There is a
//! simple box model — margin, border, and padding offset position and size — but
//! no real inline formatting, no line wrapping, no floats, and no positioning.
//!
//! Text dimensions are estimated, not measured:
//! - width  = `char_count * font_size * 0.6`
//! - height = `font_size * 1.2`

pub use mocha_style::Color;
pub use mocha_style::NodeId;

use mocha_error::{MochaError, MochaResult};
use mocha_style::{Display, StyledNode};

/// Default viewport width used by the shell.
pub const DEFAULT_VIEWPORT_WIDTH: f32 = 800.0;
/// Default viewport height. Vertical content may exceed this in Milestone 2.
pub const DEFAULT_VIEWPORT_HEIGHT: f32 = 600.0;

/// The available drawing area. Only `width` influences layout today.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutViewport {
    /// Viewport width in pixels.
    pub width: f32,
    /// Viewport height in pixels (currently informational only).
    pub height: f32,
}

impl Default for LayoutViewport {
    fn default() -> Self {
        LayoutViewport {
            width: DEFAULT_VIEWPORT_WIDTH,
            height: DEFAULT_VIEWPORT_HEIGHT,
        }
    }
}

/// An axis-aligned rectangle in pixels with the origin at the top-left. For
/// boxes this is the border box (content + padding + border).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// What a [`LayoutBox`] represents.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutBoxKind {
    /// A block-level box.
    Block,
    /// An inline-level box (still stacked vertically in this milestone).
    Inline,
    /// A text box carrying its rendered string.
    Text(String),
}

/// A node in the layout tree with computed geometry and the style fields paint
/// needs (so `mocha_paint` does not depend on `mocha_css`/`mocha_style`).
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    /// The DOM node this box was generated from.
    pub node_id: NodeId,
    /// The kind of box.
    pub kind: LayoutBoxKind,
    /// The border-box geometry.
    pub rect: Rect,
    /// Font size in pixels.
    pub font_size: f32,
    /// Text/foreground color.
    pub color: Color,
    /// Background color (`a == 0` means transparent / no fill).
    pub background_color: Color,
    /// Border width in pixels (0 means no border).
    pub border_width: f32,
    /// Border color.
    pub border_color: Color,
    /// Child boxes in document order.
    pub children: Vec<LayoutBox>,
}

/// Build a layout tree from a styled tree for the given `viewport`.
///
/// `display: none` nodes (and their subtrees) produce no boxes.
pub fn build_layout_tree(
    styled_root: &StyledNode,
    viewport: LayoutViewport,
) -> MochaResult<LayoutBox> {
    match layout_node(styled_root, 0.0, 0.0, viewport.width) {
        Some((layout_box, _)) => Ok(layout_box),
        None => Err(MochaError::Layout(
            "the document root has display:none and produced no layout box".to_string(),
        )),
    }
}

/// Lay out one styled node at top-left `(x, y)` within `available_width` (the
/// containing block's content width).
///
/// Returns the box and the vertical space it consumes in its parent's content
/// box (its full margin-box height), or `None` for `display: none`.
fn layout_node(
    styled: &StyledNode,
    x: f32,
    y: f32,
    available_width: f32,
) -> Option<(LayoutBox, f32)> {
    if let Some(text) = &styled.text {
        return Some(layout_text(styled, text, x, y));
    }

    let style = &styled.style;
    if style.display == Display::None {
        return None;
    }

    let margin = style.margin;
    let padding = style.padding;
    let border = style.border_width;

    let content_x = x + margin.left + border + padding.left;
    let content_y = y + margin.top + border + padding.top;
    let content_width = style.width.unwrap_or_else(|| {
        (available_width - margin.left - margin.right - 2.0 * border - padding.left - padding.right)
            .max(0.0)
    });

    let mut cursor_y = content_y;
    let mut children = Vec::new();
    for child in &styled.children {
        if let Some((child_box, advance)) = layout_node(child, content_x, cursor_y, content_width) {
            cursor_y += advance;
            children.push(child_box);
        }
    }

    let content_height = style.height.unwrap_or((cursor_y - content_y).max(0.0));
    let border_box_width = content_width + padding.left + padding.right + 2.0 * border;
    let border_box_height = content_height + padding.top + padding.bottom + 2.0 * border;

    let kind = match style.display {
        Display::Inline => LayoutBoxKind::Inline,
        // Block and (unreachable) None both fall here as Block.
        _ => LayoutBoxKind::Block,
    };

    let layout_box = LayoutBox {
        node_id: styled.node_id,
        kind,
        rect: Rect {
            x: x + margin.left,
            y: y + margin.top,
            width: border_box_width,
            height: border_box_height,
        },
        font_size: style.font_size,
        color: style.color,
        background_color: style.background_color,
        border_width: border,
        border_color: style.border_color,
        children,
    };
    let advance = margin.top + border_box_height + margin.bottom;
    Some((layout_box, advance))
}

fn layout_text(styled: &StyledNode, text: &str, x: f32, y: f32) -> (LayoutBox, f32) {
    let font_size = styled.style.font_size;
    let width = (text.chars().count() as f32 * font_size * 0.6).round();
    let height = (font_size * 1.2).round();
    let layout_box = LayoutBox {
        node_id: styled.node_id,
        kind: LayoutBoxKind::Text(text.to_string()),
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        font_size,
        color: styled.style.color,
        background_color: Color::TRANSPARENT,
        border_width: 0.0,
        border_color: styled.style.color,
        children: Vec::new(),
    };
    (layout_box, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_style::{ComputedStyle, Display, EdgeSizes, StyledNode};

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
            children,
        }
    }

    fn text(node_id: usize, content: &str, font_size: f32) -> StyledNode {
        let mut style = ComputedStyle::initial();
        style.display = Display::Inline;
        style.font_size = font_size;
        StyledNode {
            node_id: NodeId(node_id),
            text: Some(content.to_string()),
            style,
            children: Vec::new(),
        }
    }

    fn find(root: &LayoutBox, id: usize) -> Option<&LayoutBox> {
        if root.node_id == NodeId(id) {
            return Some(root);
        }
        root.children.iter().find_map(|child| find(child, id))
    }

    #[test]
    fn computed_font_size_affects_text_layout() {
        let big = text(1, "ab", 40.0);
        let root = element(0, block_style(), vec![big]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        let text_box = find(&layout, 1).unwrap();
        // width = 2 chars * 40 * 0.6 = 48, height = round(40 * 1.2) = 48.
        assert_eq!(text_box.rect.width, 48.0);
        assert_eq!(text_box.rect.height, 48.0);
    }

    #[test]
    fn width_property_overrides_block_width() {
        let mut style = block_style();
        style.width = Some(123.0);
        let root = element(0, block_style(), vec![element(1, style, Vec::new())]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        let inner = find(&layout, 1).unwrap();
        assert_eq!(inner.rect.width, 123.0);
    }

    #[test]
    fn height_property_overrides_content_height() {
        let mut style = block_style();
        style.height = Some(200.0);
        let root = element(0, block_style(), vec![element(1, style, Vec::new())]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        let inner = find(&layout, 1).unwrap();
        assert_eq!(inner.rect.height, 200.0);
    }

    #[test]
    fn margin_affects_y_position() {
        let mut style = block_style();
        style.margin = EdgeSizes {
            top: 25.0,
            ..EdgeSizes::default()
        };
        let root = element(0, block_style(), vec![element(1, style, Vec::new())]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        let inner = find(&layout, 1).unwrap();
        assert_eq!(inner.rect.y, 25.0);
    }

    #[test]
    fn padding_and_border_affect_child_position() {
        let mut parent = block_style();
        parent.padding = EdgeSizes {
            top: 10.0,
            left: 12.0,
            ..EdgeSizes::default()
        };
        parent.border_width = 2.0;
        let child = text(2, "x", 16.0);
        let root = element(0, block_style(), vec![element(1, parent, vec![child])]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        let text_box = find(&layout, 2).unwrap();
        // child x = border(2) + padding.left(12); y = border(2) + padding.top(10).
        assert_eq!(text_box.rect.x, 14.0);
        assert_eq!(text_box.rect.y, 12.0);
    }

    #[test]
    fn display_none_skips_layout() {
        let mut hidden = block_style();
        hidden.display = Display::None;
        let root = element(
            0,
            block_style(),
            vec![element(1, hidden, vec![text(2, "hidden", 16.0)])],
        );
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        assert!(find(&layout, 1).is_none());
        assert!(find(&layout, 2).is_none());
        assert!(layout.children.is_empty());
    }

    #[test]
    fn inline_box_is_currently_stacked_not_inline_formatted() {
        // MILESTONE 3 NOTE: inline layout is intentionally fake today — an inline
        // element produces its own stacked box rather than participating in a line
        // box. This test pins the current behavior so the Milestone 3 rewrite is a
        // deliberate, visible change.
        let mut inline = ComputedStyle::initial();
        inline.display = Display::Inline;
        let a = element(1, inline.clone(), vec![text(3, "a", 16.0)]);
        let b = element(2, inline, vec![text(4, "b", 16.0)]);
        let root = element(0, block_style(), vec![a, b]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();

        let box_a = find(&layout, 1).unwrap();
        let box_b = find(&layout, 2).unwrap();
        assert_eq!(box_a.kind, LayoutBoxKind::Inline);
        // Two inline siblings stack on separate lines (b below a), which real
        // inline formatting would not do.
        assert!(box_b.rect.y >= box_a.rect.y + box_a.rect.height);
    }

    #[test]
    fn text_width_is_estimated_from_char_count() {
        // MILESTONE 3 NOTE: width is char_count * font_size * 0.6, not measured
        // from a font. This will change when real text measurement lands.
        let root = element(0, block_style(), vec![text(1, "abcde", 16.0)]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        let text_box = find(&layout, 1).unwrap();
        assert_eq!(text_box.rect.width, (5.0 * 16.0 * 0.6_f32).round());
    }

    #[test]
    fn blocks_stack_vertically() {
        let first = element(1, block_style(), vec![text(3, "first", 16.0)]);
        let second = element(2, block_style(), vec![text(4, "second", 16.0)]);
        let root = element(0, block_style(), vec![first, second]);
        let layout = build_layout_tree(&root, LayoutViewport::default()).unwrap();
        let a = find(&layout, 1).unwrap();
        let b = find(&layout, 2).unwrap();
        assert!(b.rect.y >= a.rect.y + a.rect.height);
    }
}
