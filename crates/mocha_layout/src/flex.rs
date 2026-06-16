//! A single-line flex formatting context (`display: flex`).
//!
//! This implements the common, high-impact subset of CSS flexbox used by modern
//! layouts: `flex-direction` row/column (+ reverse), `justify-content`
//! (start/end/center/space-between/around/evenly), `align-items`
//! (stretch/start/end/center), `gap`, and `flex-grow`. There is no wrapping
//! (`flex-wrap`), no `flex-shrink`, and no baseline alignment — flex items are
//! sized by their content (or explicit `width`/`height`), grown to fill free
//! space when `flex-grow > 0`, and positioned along the two axes.

use mocha_style::{AlignItems, Display, FlexDirection, JustifyContent, StyledNode};

use crate::block::layout_block;
use crate::box_tree::LayoutBox;

/// A very large width used to measure a row item's max-content size.
const UNBOUNDED: f32 = 1.0e6;

/// Lay out the children of a flex container into its content box at
/// `(content_x, content_y)` with main-axis extent `content_width` (for a row) /
/// the container width (for a column). Returns the item boxes plus the content
/// height the container needs.
pub(crate) fn layout_flex(
    styled: &StyledNode,
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<LayoutBox>, f32) {
    let style = &styled.style;
    let row = matches!(
        style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );
    let reverse = matches!(
        style.flex_direction,
        FlexDirection::RowReverse | FlexDirection::ColumnReverse
    );
    let gap = style.gap.max(0.0);

    // Collect items in order (skip `display: none` and whitespace-only text).
    let mut items: Vec<&StyledNode> = styled
        .children
        .iter()
        .filter(|c| c.style.display != Display::None && !is_blank_text(c))
        .collect();
    if reverse {
        items.reverse();
    }
    if items.is_empty() {
        return (Vec::new(), style.height.unwrap_or(0.0));
    }

    // Lay each item out at the origin and record its main/cross border-box size.
    let mut boxes: Vec<LayoutBox> = Vec::with_capacity(items.len());
    let mut mains: Vec<f32> = Vec::with_capacity(items.len());
    let mut crosses: Vec<f32> = Vec::with_capacity(items.len());
    for item in &items {
        let mut b = if row {
            // Row items are shrink-to-fit: measure at an unbounded width.
            let mut measured = layout_block(item, 0.0, 0.0, UNBOUNDED);
            let natural = content_extent(&measured).min(content_width.max(0.0));
            set_width(&mut measured, natural);
            measured
        } else {
            // Column items fill the cross axis (container width) naturally.
            layout_block(item, 0.0, 0.0, content_width)
        };
        normalize_origin(&mut b);
        let (main, cross) = if row {
            (b.rect.width, b.rect.height)
        } else {
            (b.rect.height, b.rect.width)
        };
        mains.push(main);
        crosses.push(cross);
        boxes.push(b);
    }

    let n = boxes.len();
    let main_container = if row {
        content_width
    } else {
        style.height.unwrap_or(0.0)
    };
    let total_gap = gap * (n.saturating_sub(1)) as f32;
    let total_main: f32 = mains.iter().sum::<f32>() + total_gap;

    // Distribute free space to items with flex-grow.
    let free = (main_container - total_main).max(0.0);
    let total_grow: f32 = items.iter().map(|i| i.style.flex_grow.max(0.0)).sum();
    if free > 0.0 && total_grow > 0.0 && row {
        for (i, item) in items.iter().enumerate() {
            let grow = item.style.flex_grow.max(0.0);
            if grow > 0.0 {
                let add = free * grow / total_grow;
                mains[i] += add;
                set_width(&mut boxes[i], mains[i]);
            }
        }
    }
    let used_main: f32 = mains.iter().sum::<f32>() + total_gap;

    // Cross size of the line: the tallest/widest item, or a fixed container size.
    let fixed_cross = if row {
        style.height
    } else {
        Some(content_width)
    };
    let line_cross = fixed_cross.unwrap_or_else(|| crosses.iter().cloned().fold(0.0, f32::max));

    // Main-axis distribution from justify-content. A column with no fixed height
    // packs its items from the start.
    let main_box = if row {
        content_width
    } else {
        main_container.max(used_main)
    };
    let (mut cursor, spacing) = justify(
        style.justify_content,
        main_box.max(used_main),
        used_main,
        n,
        gap,
    );

    for i in 0..n {
        // Cross-axis position, and stretch the item to fill the line if asked.
        let cross_pos = align(style.align_items, line_cross, crosses[i]);
        if matches!(style.align_items, AlignItems::Stretch) {
            if row {
                set_height(&mut boxes[i], line_cross);
            } else {
                set_width(&mut boxes[i], line_cross);
            }
        }

        let (dx, dy) = if row {
            (content_x + cursor, content_y + cross_pos)
        } else {
            (content_x + cross_pos, content_y + cursor)
        };
        translate(&mut boxes[i], dx, dy);
        cursor += mains[i] + spacing;
    }

    let content_height = if row {
        style.height.unwrap_or(line_cross)
    } else {
        style.height.unwrap_or(used_main)
    };
    (boxes, content_height)
}

/// `justify-content`: returns the starting offset and the per-gap spacing.
fn justify(
    justify: JustifyContent,
    container_main: f32,
    used_main: f32,
    count: usize,
    gap: f32,
) -> (f32, f32) {
    let free = (container_main - used_main).max(0.0);
    let n = count as f32;
    match justify {
        JustifyContent::Start => (0.0, gap),
        JustifyContent::End => (free, gap),
        JustifyContent::Center => (free / 2.0, gap),
        JustifyContent::SpaceBetween if count > 1 => (0.0, gap + free / (n - 1.0)),
        JustifyContent::SpaceBetween => (free / 2.0, gap),
        JustifyContent::SpaceAround => {
            let unit = free / n;
            (unit / 2.0, gap + unit)
        }
        JustifyContent::SpaceEvenly => {
            let unit = free / (n + 1.0);
            (unit, gap + unit)
        }
    }
}

/// `align-items`: the cross-axis offset for an item of size `item_cross`.
fn align(align: AlignItems, line_cross: f32, item_cross: f32) -> f32 {
    match align {
        AlignItems::Start | AlignItems::Stretch => 0.0,
        AlignItems::End => (line_cross - item_cross).max(0.0),
        AlignItems::Center => ((line_cross - item_cross) / 2.0).max(0.0),
    }
}

/// Whether a node is a whitespace-only text node (collapsed between flex items).
fn is_blank_text(node: &StyledNode) -> bool {
    node.text.as_deref().is_some_and(|t| t.trim().is_empty())
}

/// Move a box (and its whole subtree) by `(dx, dy)`.
fn translate(b: &mut LayoutBox, dx: f32, dy: f32) {
    b.rect.x += dx;
    b.rect.y += dy;
    for child in &mut b.children {
        translate(child, dx, dy);
    }
}

/// Shift a box so its top-left sits at the origin (removing baked-in margins).
fn normalize_origin(b: &mut LayoutBox) {
    let (dx, dy) = (-b.rect.x, -b.rect.y);
    translate(b, dx, dy);
}

/// Set a box's border-box width without re-laying its content (left-aligned).
fn set_width(b: &mut LayoutBox, width: f32) {
    b.rect.width = width.max(0.0);
}

/// Set a box's border-box height without re-laying its content.
fn set_height(b: &mut LayoutBox, height: f32) {
    b.rect.height = height.max(0.0);
}

/// The max-content width of a laid-out box: the rightmost leaf edge relative to
/// the box's left, so an auto-width item measured at an unbounded width reports
/// its natural content width rather than the measuring width.
fn content_extent(b: &LayoutBox) -> f32 {
    let mut max_right = b.rect.x;
    collect_leaf_right(b, &mut max_right);
    (max_right - b.rect.x).max(0.0)
}

fn collect_leaf_right(b: &LayoutBox, max_right: &mut f32) {
    if b.children.is_empty() {
        *max_right = max_right.max(b.rect.x + b.rect.width);
    } else {
        for child in &b.children {
            collect_leaf_right(child, max_right);
        }
    }
}
