//! Inline formatting context: flatten inline-level styled nodes into words and
//! hand them to [`crate::line`] for line-box construction.
//!
//! Because the styled tree already carries each text node's computed `color` and
//! `font-size` (inherited from its inline ancestors), flattening to words gives
//! correct per-run styling without any inheritance logic here. Nested inline
//! elements are walked transparently; `display: none` nodes contribute nothing.

use mocha_style::{Display, StyledNode};

use crate::box_tree::LayoutBox;
use crate::line::{layout_words, Word};

/// Lay an inline run (the inline-level children of one container) out into line
/// boxes at `(content_x, content_y)` within `available_width`.
pub(crate) fn layout_inline(
    inline_children: &[&StyledNode],
    content_x: f32,
    content_y: f32,
    available_width: f32,
) -> (Vec<LayoutBox>, f32) {
    let mut words = Vec::new();
    let mut pending_space = false;
    for child in inline_children {
        collect_words(child, &mut words, &mut pending_space);
    }
    layout_words(&words, content_x, content_y, available_width)
}

/// Walk an inline subtree in document order, appending one [`Word`] per
/// whitespace-separated token. `pending_space` carries a trailing space across
/// node boundaries so the space between `Hello ` and a following `<span>` is kept.
fn collect_words(node: &StyledNode, words: &mut Vec<Word>, pending_space: &mut bool) {
    if node.style.display == Display::None {
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
            words.push(Word {
                text: (*token).to_string(),
                font_size: node.style.font_size,
                color: node.style.color,
                space_before,
            });
        }
        *pending_space = trailing;
    } else {
        for child in &node.children {
            collect_words(child, words, pending_space);
        }
    }
}
