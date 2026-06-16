//! Inline formatting context: flatten inline-level styled nodes into a stream of
//! words and image atoms, and hand them to [`crate::line`] for line-box building.
//!
//! Because the styled tree already carries each text node's computed `color` and
//! `font-size` (inherited from its inline ancestors), flattening gives correct
//! per-run styling without any inheritance logic here. Nested inline elements are
//! walked transparently; an inline `<img>` becomes a replaced atom;
//! `display: none` nodes contribute nothing.

use mocha_style::{Display, StyledNode, TextAlign};

use crate::box_tree::LayoutBox;
use crate::line::{layout_items, ControlAtom, ImageAtom, InlineItem, Word};

/// Lay an inline run (the inline-level children of one container) out into line
/// boxes at `(content_x, content_y)` within `available_width`.
pub(crate) fn layout_inline(
    inline_children: &[&StyledNode],
    content_x: f32,
    content_y: f32,
    available_width: f32,
    align: TextAlign,
) -> (Vec<LayoutBox>, f32) {
    let mut items = Vec::new();
    let mut pending_space = false;
    for child in inline_children {
        collect_items(child, &mut items, &mut pending_space);
    }
    layout_items(&items, content_x, content_y, available_width, align)
}

/// Walk an inline subtree in document order, appending one [`InlineItem`] per
/// whitespace-separated token (or per replaced element). `pending_space` carries a
/// trailing space across node boundaries so the space between `Hello ` and a
/// following `<span>`/`<img>` is kept.
fn collect_items(node: &StyledNode, items: &mut Vec<InlineItem>, pending_space: &mut bool) {
    if node.style.display == Display::None {
        return;
    }

    // A replaced element (image) is a single inline atom.
    if let Some(replaced) = &node.replaced {
        items.push(InlineItem::Image(ImageAtom {
            image_id: replaced.image_id,
            width: replaced.width,
            height: replaced.height,
            space_before: *pending_space,
            node_id: node.node_id,
        }));
        *pending_space = false;
        return;
    }

    // A form control is a single inline atom too; its children (e.g. a
    // button's label or a select's options) never lay out as separate boxes —
    // paint renders the control from its `ControlBox` data.
    if let Some(control) = &node.control {
        items.push(InlineItem::Control(ControlAtom {
            control: control.clone(),
            space_before: *pending_space,
            node_id: node.node_id,
        }));
        *pending_space = false;
        return;
    }

    if let Some(text) = &node.text {
        let leading = text.starts_with(char::is_whitespace);
        let trailing = text.ends_with(char::is_whitespace);
        let tokens: Vec<&str> = text.split_whitespace().collect();
        if tokens.is_empty() {
            // A whitespace-only run still implies a separating space.
            *pending_space = *pending_space || leading;
            return;
        }
        for (index, token) in tokens.iter().enumerate() {
            let space_before = if index == 0 {
                *pending_space || leading
            } else {
                true
            };
            items.push(InlineItem::Word(Word {
                text: (*token).to_string(),
                font_size: node.style.font_size,
                color: node.style.color,
                space_before,
                node_id: node.node_id,
            }));
        }
        *pending_space = trailing;
    } else {
        for child in &node.children {
            collect_items(child, items, pending_space);
        }
    }
}
