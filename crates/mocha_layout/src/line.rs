//! Line-box construction and word wrapping for an inline formatting context.
//!
//! Text is measured with the same estimate used elsewhere — `chars * font * 0.6`
//! per word and one such character for an inter-word space — not real font
//! metrics. Words wrap at word boundaries; a single word wider than the line is
//! placed alone and allowed to overflow (no hyphenation, no character wrapping).

use mocha_style::Color;

use crate::box_tree::{LayoutBox, LayoutBoxKind};
use crate::geometry::Rect;

/// One word to place, with the style it should be painted in. `space_before`
/// records whether a single space separated it from the previous word in source.
#[derive(Debug, Clone)]
pub(crate) struct Word {
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub space_before: bool,
}

fn word_width(text: &str, font_size: f32) -> f32 {
    (text.chars().count() as f32 * font_size * 0.6).round()
}

fn space_width(font_size: f32) -> f32 {
    (font_size * 0.6).round()
}

fn line_height(font_size: f32) -> f32 {
    (font_size * 1.2).round()
}

/// Lay `words` out into stacked line boxes within `available_width`, starting at
/// `(content_x, content_y)`. Returns the line boxes and the total height used.
pub(crate) fn layout_words(
    words: &[Word],
    content_x: f32,
    content_y: f32,
    available_width: f32,
) -> (Vec<LayoutBox>, f32) {
    let mut lines: Vec<LayoutBox> = Vec::new();
    let mut runs: Vec<LayoutBox> = Vec::new();
    let mut cursor_x = 0.0;
    let mut max_font = 0.0_f32;
    let mut line_top = content_y;

    for word in words {
        let w = word_width(&word.text, word.font_size);
        let mut space = if word.space_before && !runs.is_empty() {
            space_width(word.font_size)
        } else {
            0.0
        };

        // Wrap when the next word would overflow the current (non-empty) line.
        if !runs.is_empty() && cursor_x + space + w > available_width {
            let height = line_height(max_font);
            lines.push(finish_line(
                content_x,
                line_top,
                available_width,
                height,
                max_font,
                &mut runs,
            ));
            line_top += height;
            cursor_x = 0.0;
            max_font = 0.0;
            space = 0.0; // a wrapped line never starts with a leading space
        }

        cursor_x += space;
        runs.push(LayoutBox {
            node_id: None,
            kind: LayoutBoxKind::TextRun(word.text.clone()),
            rect: Rect {
                x: content_x + cursor_x,
                y: line_top,
                width: w,
                height: line_height(word.font_size),
            },
            font_size: word.font_size,
            color: word.color,
            background_color: Color::TRANSPARENT,
            border_width: 0.0,
            border_color: word.color,
            children: Vec::new(),
        });
        cursor_x += w;
        max_font = max_font.max(word.font_size);
    }

    if !runs.is_empty() {
        let height = line_height(max_font);
        lines.push(finish_line(
            content_x,
            line_top,
            available_width,
            height,
            max_font,
            &mut runs,
        ));
        line_top += height;
    }

    (lines, line_top - content_y)
}

fn finish_line(
    content_x: f32,
    line_top: f32,
    available_width: f32,
    height: f32,
    max_font: f32,
    runs: &mut Vec<LayoutBox>,
) -> LayoutBox {
    let mut line = LayoutBox::anonymous(
        LayoutBoxKind::LineBox,
        Rect {
            x: content_x,
            y: line_top,
            width: available_width,
            height,
        },
        std::mem::take(runs),
    );
    line.font_size = max_font;
    line
}
