//! Human-readable layout-tree dump for debugging and tests.

use std::fmt::Write;

use crate::box_tree::{LayoutBox, LayoutBoxKind};

/// Format a layout tree as an indented, human-readable string.
///
/// Each line shows the box kind, the DOM node id (when present), the rect, and —
/// for text runs — the text.
pub fn format_layout_tree(root: &LayoutBox) -> String {
    let mut output = String::new();
    write_box(root, 0, &mut output);
    output
}

fn write_box(layout_box: &LayoutBox, depth: usize, output: &mut String) {
    for _ in 0..depth {
        output.push_str("  ");
    }

    let rect = layout_box.rect;
    let kind = match &layout_box.kind {
        LayoutBoxKind::Block => "Block".to_string(),
        LayoutBoxKind::Inline => "Inline".to_string(),
        LayoutBoxKind::AnonymousBlock => "AnonymousBlock".to_string(),
        LayoutBoxKind::LineBox => "LineBox".to_string(),
        LayoutBoxKind::TextRun(text) => format!("TextRun {text:?}"),
        LayoutBoxKind::Image(image_id) => format!("Image #{image_id}"),
    };

    let _ = write!(output, "{kind}");
    if let Some(node_id) = layout_box.node_id {
        let _ = write!(output, " node=#{}", node_id.0);
    }
    let _ = writeln!(
        output,
        " rect=({},{} {}x{})",
        rect.x, rect.y, rect.width, rect.height
    );

    for child in &layout_box.children {
        write_box(child, depth + 1, output);
    }
}
