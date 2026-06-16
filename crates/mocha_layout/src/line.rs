//! Line-box construction and wrapping for an inline formatting context.
//!
//! Inline content is flattened into a stream of [`InlineItem`]s — words and
//! replaced-element (image) atoms — which are placed left to right and wrapped at
//! item boundaries. Text is measured with the same estimate used elsewhere
//! (`chars * font * 0.6` per word, one such character for an inter-word space),
//! not real font metrics. A single item wider than the line is placed alone and
//! allowed to overflow (no hyphenation, no character wrapping). A line's height is
//! the tallest item on it, so an inline image taller than the text raises the
//! line height. Baseline / `vertical-align` is not modelled: items are top-aligned.

use mocha_style::{Color, ControlBox, NodeId, TextAlign};

use crate::box_tree::{LayoutBox, LayoutBoxKind};
use crate::geometry::Rect;

/// One word to place, with the style it should be painted in. `space_before`
/// records whether a single space separated it from the previous item in source.
#[derive(Debug, Clone)]
pub(crate) struct Word {
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub space_before: bool,
    /// The source text node, so hit testing can map a click to the DOM.
    pub node_id: NodeId,
}

/// One inline replaced-element (image) atom to place on a line.
#[derive(Debug, Clone)]
pub(crate) struct ImageAtom {
    pub image_id: usize,
    pub width: f32,
    pub height: f32,
    pub space_before: bool,
    /// The source `<img>` node.
    pub node_id: NodeId,
}

/// One inline form-control atom to place on a line.
#[derive(Debug, Clone)]
pub(crate) struct ControlAtom {
    pub control: ControlBox,
    pub space_before: bool,
    /// The source control element.
    pub node_id: NodeId,
}

/// An item in an inline formatting context.
#[derive(Debug, Clone)]
pub(crate) enum InlineItem {
    Word(Word),
    Image(ImageAtom),
    Control(ControlAtom),
}

impl InlineItem {
    fn space_before(&self) -> bool {
        match self {
            InlineItem::Word(w) => w.space_before,
            InlineItem::Image(i) => i.space_before,
            InlineItem::Control(c) => c.space_before,
        }
    }

    /// The width the item occupies on the line.
    fn width(&self) -> f32 {
        match self {
            InlineItem::Word(w) => word_width(&w.text, w.font_size),
            InlineItem::Image(i) => i.width,
            InlineItem::Control(c) => c.control.width,
        }
    }

    /// The width of the inter-item space that precedes this item.
    fn space_width(&self) -> f32 {
        match self {
            InlineItem::Word(w) => space_width(w.font_size),
            // Images and controls carry no font; approximate the surrounding
            // space with the base font size.
            InlineItem::Image(_) | InlineItem::Control(_) => space_width(16.0),
        }
    }

    /// The vertical space the item needs (its contribution to line height).
    fn height(&self) -> f32 {
        match self {
            InlineItem::Word(w) => line_height(w.font_size),
            InlineItem::Image(i) => i.height,
            InlineItem::Control(c) => c.control.height,
        }
    }

    /// The word font size, if this is a word (used for the line box's own font).
    fn word_font(&self) -> Option<f32> {
        match self {
            InlineItem::Word(w) => Some(w.font_size),
            InlineItem::Image(_) | InlineItem::Control(_) => None,
        }
    }

    fn to_box(&self, x: f32, y: f32, width: f32, height: f32) -> LayoutBox {
        match self {
            InlineItem::Word(w) => LayoutBox {
                node_id: Some(w.node_id),
                kind: LayoutBoxKind::TextRun(w.text.clone()),
                rect: Rect {
                    x,
                    y,
                    width,
                    height,
                },
                font_size: w.font_size,
                color: w.color,
                background_color: Color::TRANSPARENT,
                border_width: 0.0,
                border_color: w.color,
                border_radius: 0.0,
                children: Vec::new(),
            },
            InlineItem::Image(i) => LayoutBox {
                node_id: Some(i.node_id),
                kind: LayoutBoxKind::Image(i.image_id),
                rect: Rect {
                    x,
                    y,
                    width,
                    height,
                },
                font_size: 0.0,
                color: Color::BLACK,
                background_color: Color::TRANSPARENT,
                border_width: 0.0,
                border_color: Color::BLACK,
                border_radius: 0.0,
                children: Vec::new(),
            },
            InlineItem::Control(c) => LayoutBox {
                node_id: Some(c.node_id),
                kind: LayoutBoxKind::Control(c.control.clone()),
                rect: Rect {
                    x,
                    y,
                    width,
                    height,
                },
                font_size: 0.0,
                color: Color::BLACK,
                background_color: Color::TRANSPARENT,
                border_width: 0.0,
                border_color: Color::BLACK,
                border_radius: 0.0,
                children: Vec::new(),
            },
        }
    }
}

fn word_width(text: &str, font_size: f32) -> f32 {
    mocha_text::measure(text, font_size, false)
}

fn space_width(font_size: f32) -> f32 {
    mocha_text::space_advance(font_size, false)
}

fn line_height(font_size: f32) -> f32 {
    mocha_text::line_height(font_size)
}

/// Lay `items` out into stacked line boxes within `available_width`, starting at
/// `(content_x, content_y)`. Returns the line boxes and the total height used.
pub(crate) fn layout_items(
    items: &[InlineItem],
    content_x: f32,
    content_y: f32,
    available_width: f32,
    align: TextAlign,
) -> (Vec<LayoutBox>, f32) {
    let mut lines: Vec<LayoutBox> = Vec::new();
    let mut runs: Vec<LayoutBox> = Vec::new();
    let mut cursor_x = 0.0;
    let mut line_height_px = 0.0_f32; // tallest item on the current line
    let mut line_font = 0.0_f32; // tallest word font on the current line
    let mut line_top = content_y;

    for item in items {
        let w = item.width();
        let mut space = if item.space_before() && !runs.is_empty() {
            item.space_width()
        } else {
            0.0
        };

        // Wrap when the next item would overflow the current (non-empty) line.
        if !runs.is_empty() && cursor_x + space + w > available_width {
            lines.push(finish_line(
                content_x,
                line_top,
                available_width,
                line_height_px,
                line_font,
                cursor_x,
                align,
                &mut runs,
            ));
            line_top += line_height_px;
            cursor_x = 0.0;
            line_height_px = 0.0;
            line_font = 0.0;
            space = 0.0; // a wrapped line never starts with a leading space
        }

        cursor_x += space;
        let height = item.height();
        runs.push(item.to_box(content_x + cursor_x, line_top, w, height));
        cursor_x += w;
        line_height_px = line_height_px.max(height);
        if let Some(font) = item.word_font() {
            line_font = line_font.max(font);
        }
    }

    if !runs.is_empty() {
        lines.push(finish_line(
            content_x,
            line_top,
            available_width,
            line_height_px,
            line_font,
            cursor_x,
            align,
            &mut runs,
        ));
        line_top += line_height_px;
    }

    (lines, line_top - content_y)
}

#[allow(clippy::too_many_arguments)]
fn finish_line(
    content_x: f32,
    line_top: f32,
    available_width: f32,
    height: f32,
    font: f32,
    used_width: f32,
    align: TextAlign,
    runs: &mut Vec<LayoutBox>,
) -> LayoutBox {
    // Horizontal alignment: shift every run by the line's free space.
    let factor = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => 0.5,
        TextAlign::Right => 1.0,
    };
    if factor > 0.0 {
        let shift = ((available_width - used_width) * factor).max(0.0);
        if shift > 0.0 {
            shift_runs(runs, shift);
        }
    }
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
    line.font_size = font;
    line
}

/// Shift every run on a line right by `shift` px (for center/right alignment).
fn shift_runs(runs: &mut [LayoutBox], shift: f32) {
    for run in runs.iter_mut() {
        run.rect.x += shift;
    }
}
