//! Block formatting: lay block-level children out vertically, and wrap runs of
//! inline-level children in anonymous block boxes when they sit among blocks.
//!
//! Margin collapse is **not** implemented: each block advances its parent by its
//! full margin-box height. Floats and positioning are not implemented.

use mocha_style::{Display, EdgeSizes as StyleEdges, StyledNode, TextAlign};

use crate::box_tree::{LayoutBox, LayoutBoxKind};
use crate::geometry::{EdgeSizes, Rect};
use crate::inline;

/// Lay out a block-level node whose margin box starts at `(x, y)`, inside a
/// containing block of content width `available_width`.
pub(crate) fn layout_block(styled: &StyledNode, x: f32, y: f32, available_width: f32) -> LayoutBox {
    let style = &styled.style;
    let margin = edges(&style.margin);
    let padding = edges(&style.padding);
    let border = style.border_width;

    // A block-level replaced element (`<img style="display:block">`): its content
    // box is the resolved image size; it has no flow children.
    if let Some(replaced) = &styled.replaced {
        let border_box_width = replaced.width + padding.horizontal() + 2.0 * border;
        let border_box_height = replaced.height + padding.vertical() + 2.0 * border;
        return LayoutBox {
            node_id: Some(styled.node_id),
            kind: LayoutBoxKind::Image(replaced.image_id),
            rect: Rect {
                x: x + margin.left,
                y: y + margin.top,
                width: border_box_width,
                height: border_box_height,
            },
            font_size: 0.0,
            color: style.color,
            background_color: style.background_color,
            border_width: border,
            border_color: style.border_color,
            children: Vec::new(),
        };
    }

    // A block-level form control (`display: block` via CSS): like a replaced
    // element, its content box is the resolved control size and it has no flow
    // children.
    if let Some(control) = &styled.control {
        let border_box_width = control.width + padding.horizontal() + 2.0 * border;
        let border_box_height = control.height + padding.vertical() + 2.0 * border;
        return LayoutBox {
            node_id: Some(styled.node_id),
            kind: LayoutBoxKind::Control(control.clone()),
            rect: Rect {
                x: x + margin.left,
                y: y + margin.top,
                width: border_box_width,
                height: border_box_height,
            },
            font_size: 0.0,
            color: style.color,
            background_color: style.background_color,
            border_width: border,
            border_color: style.border_color,
            children: Vec::new(),
        };
    }

    let mut content_width = style.width.unwrap_or_else(|| {
        (available_width - margin.horizontal() - padding.horizontal() - 2.0 * border).max(0.0)
    });
    // `max-width` caps the content width.
    if let Some(max) = style.max_width {
        content_width = content_width.min(max.max(0.0));
    }

    let border_box_width = content_width + padding.horizontal() + 2.0 * border;
    // `margin: 0 auto` (or `margin-left/right: auto`) centers a block whose box
    // is narrower than the containing block.
    let left_margin = if style.center_horizontally && border_box_width < available_width {
        (available_width - border_box_width) / 2.0
    } else {
        margin.left
    };
    let content_x = x + left_margin + border + padding.left;
    let content_y = y + margin.top + border + padding.top;

    // A flex container lays its children out in a flex formatting context;
    // every other block uses normal block/inline flow.
    let (children, children_height) = if style.display == Display::Flex {
        crate::flex::layout_flex(styled, content_x, content_y, content_width)
    } else {
        layout_children(styled, content_x, content_y, content_width)
    };
    let content_height = style.height.unwrap_or(children_height);

    let border_box_height = content_height + padding.vertical() + 2.0 * border;

    LayoutBox {
        node_id: Some(styled.node_id),
        kind: LayoutBoxKind::Block,
        rect: Rect {
            x: x + left_margin,
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
    }
}

/// Lay out the children of a container into its content box and return the child
/// boxes plus the total content height they consume.
///
/// If the container has any block-level children, contiguous inline-level
/// children are wrapped in anonymous block boxes. If it has only inline-level
/// children, it establishes an inline formatting context directly (line boxes
/// become its children).
fn layout_children(
    parent: &StyledNode,
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<LayoutBox>, f32) {
    let visible: Vec<&StyledNode> = parent
        .children
        .iter()
        .filter(|child| child.style.display != Display::None)
        .collect();

    let has_block = visible.iter().any(|child| is_block_level(child));
    if !has_block {
        return inline::layout_inline(
            &visible,
            content_x,
            content_y,
            content_width,
            parent.style.text_align,
        );
    }

    let mut boxes = Vec::new();
    let mut cursor_y = content_y;
    let mut inline_group: Vec<&StyledNode> = Vec::new();

    for child in visible {
        if is_block_level(child) {
            flush_inline_group(
                &mut inline_group,
                content_x,
                &mut cursor_y,
                content_width,
                parent.style.text_align,
                &mut boxes,
            );
            let block_box = layout_block(child, content_x, cursor_y, content_width);
            cursor_y += margin_box_height(&block_box, &child.style.margin);
            boxes.push(block_box);
        } else {
            inline_group.push(child);
        }
    }
    flush_inline_group(
        &mut inline_group,
        content_x,
        &mut cursor_y,
        content_width,
        parent.style.text_align,
        &mut boxes,
    );

    (boxes, cursor_y - content_y)
}

/// Lay out any accumulated inline children as one anonymous block box.
///
/// A group that produces no line boxes — e.g. only inter-tag whitespace between
/// block siblings — emits nothing, so indentation whitespace adds no visible box
/// or height.
fn flush_inline_group(
    inline_group: &mut Vec<&StyledNode>,
    content_x: f32,
    cursor_y: &mut f32,
    content_width: f32,
    align: TextAlign,
    boxes: &mut Vec<LayoutBox>,
) {
    if inline_group.is_empty() {
        return;
    }
    let (lines, height) =
        inline::layout_inline(inline_group, content_x, *cursor_y, content_width, align);
    if lines.is_empty() {
        inline_group.clear();
        return;
    }
    let anon = LayoutBox::anonymous(
        LayoutBoxKind::AnonymousBlock,
        Rect {
            x: content_x,
            y: *cursor_y,
            width: content_width,
            height,
        },
        lines,
    );
    *cursor_y += height;
    boxes.push(anon);
    inline_group.clear();
}

fn margin_box_height(layout_box: &LayoutBox, margin: &StyleEdges) -> f32 {
    margin.top + layout_box.rect.height + margin.bottom
}

fn is_block_level(node: &StyledNode) -> bool {
    node.text.is_none() && matches!(node.style.display, Display::Block | Display::Flex)
}

fn edges(style_edges: &StyleEdges) -> EdgeSizes {
    EdgeSizes {
        top: style_edges.top,
        right: style_edges.right,
        bottom: style_edges.bottom,
        left: style_edges.left,
    }
}
