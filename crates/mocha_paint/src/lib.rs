//! Display-list generation for Mocha Browser.
//!
//! This crate walks a [`LayoutBox`] tree and emits a flat list of
//! [`DisplayCommand`]s carrying color information from computed style.
//! **Nothing is painted to a real surface** — there is no graphics library and
//! no window. The commands are a debug representation the shell prints to the
//! terminal. A future milestone will feed an equivalent list to a real
//! compositor.

use mocha_error::MochaResult;
use mocha_layout::{Color, LayoutBox, LayoutBoxKind};

/// A single drawing instruction in a display list.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayCommand {
    /// Fill a rectangle with a solid color (used for backgrounds).
    DrawRect {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Width.
        width: f32,
        /// Height.
        height: f32,
        /// Fill color.
        color: Color,
    },
    /// Draw a box border of the given width and color.
    DrawBorder {
        /// Left edge of the border box.
        x: f32,
        /// Top edge of the border box.
        y: f32,
        /// Border-box width.
        width: f32,
        /// Border-box height.
        height: f32,
        /// Border thickness.
        border_width: f32,
        /// Border color.
        color: Color,
    },
    /// Draw a run of text.
    DrawText {
        /// The text to draw.
        text: String,
        /// Left edge of the text.
        x: f32,
        /// Top edge (baseline handling is out of scope for this milestone).
        y: f32,
        /// Font size in pixels.
        font_size: f32,
        /// Text color.
        color: Color,
    },
}

impl DisplayCommand {
    /// Render this command as a single readable line for terminal output.
    pub fn to_debug_line(&self) -> String {
        match self {
            DisplayCommand::DrawRect {
                x,
                y,
                width,
                height,
                color,
            } => format!("DrawRect x={x} y={y} width={width} height={height} color={color}"),
            DisplayCommand::DrawBorder {
                x,
                y,
                width,
                height,
                border_width,
                color,
            } => format!(
                "DrawBorder x={x} y={y} width={width} height={height} border_width={border_width} color={color}"
            ),
            DisplayCommand::DrawText {
                text,
                x,
                y,
                font_size,
                color,
            } => format!("DrawText {text:?} x={x} y={y} font_size={font_size} color={color}"),
        }
    }
}

/// Build a display list from a layout tree.
///
/// For each box, in depth-first order:
/// - a [`DisplayCommand::DrawRect`] is emitted for a non-transparent background,
/// - a [`DisplayCommand::DrawBorder`] is emitted when `border_width > 0`, and
/// - a [`DisplayCommand::DrawText`] is emitted for text boxes.
///
/// A box's own commands are emitted before its children's, giving a stable,
/// document-order sequence.
pub fn build_display_list(layout_root: &LayoutBox) -> MochaResult<Vec<DisplayCommand>> {
    let mut commands = Vec::new();
    paint_box(layout_root, &mut commands);
    Ok(commands)
}

fn paint_box(layout_box: &LayoutBox, commands: &mut Vec<DisplayCommand>) {
    let rect = layout_box.rect;
    match &layout_box.kind {
        // Box-generating boxes paint their background, then border, before their
        // children (so text draws on top). Anonymous blocks and line boxes carry
        // no styling, so they emit nothing themselves and just recurse.
        LayoutBoxKind::Block | LayoutBoxKind::Inline | LayoutBoxKind::AnonymousBlock => {
            // Transparent backgrounds emit no rectangle.
            if layout_box.background_color.a != 0 {
                commands.push(DisplayCommand::DrawRect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                    color: layout_box.background_color,
                });
            }
            if layout_box.border_width > 0.0 {
                commands.push(DisplayCommand::DrawBorder {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                    border_width: layout_box.border_width,
                    color: layout_box.border_color,
                });
            }
        }
        // Line boxes are pure structure: they never paint directly.
        LayoutBoxKind::LineBox => {}
        LayoutBoxKind::TextRun(text) => {
            commands.push(DisplayCommand::DrawText {
                text: text.clone(),
                x: rect.x,
                y: rect.y,
                font_size: layout_box.font_size,
                color: layout_box.color,
            });
        }
    }

    for child in &layout_box.children {
        paint_box(child, commands);
    }
}

/// Format a whole display list as newline-separated debug lines.
pub fn format_display_list(commands: &[DisplayCommand]) -> String {
    commands
        .iter()
        .map(DisplayCommand::to_debug_line)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_layout::{LayoutBoxKind, NodeId, Rect};

    fn rect() -> Rect {
        Rect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        }
    }

    fn text_box(text: &str, color: Color) -> LayoutBox {
        LayoutBox {
            node_id: None,
            kind: LayoutBoxKind::TextRun(text.to_string()),
            rect: rect(),
            font_size: 16.0,
            color,
            background_color: Color::TRANSPARENT,
            border_width: 0.0,
            border_color: Color::BLACK,
            children: Vec::new(),
        }
    }

    fn block_box(
        background_color: Color,
        border_width: f32,
        children: Vec<LayoutBox>,
    ) -> LayoutBox {
        LayoutBox {
            node_id: Some(NodeId(0)),
            kind: LayoutBoxKind::Block,
            rect: rect(),
            font_size: 16.0,
            color: Color::BLACK,
            background_color,
            border_width,
            border_color: Color::rgb(0x6b, 0x3f, 0x2a),
            children,
        }
    }

    fn line_box(children: Vec<LayoutBox>) -> LayoutBox {
        LayoutBox {
            node_id: None,
            kind: LayoutBoxKind::LineBox,
            rect: rect(),
            font_size: 16.0,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            border_width: 0.0,
            border_color: Color::BLACK,
            children,
        }
    }

    #[test]
    fn text_color_appears_in_draw_text() {
        let commands = build_display_list(&text_box("Hi", Color::rgb(0x22, 0x22, 0x22))).unwrap();
        assert_eq!(
            commands,
            vec![DisplayCommand::DrawText {
                text: "Hi".to_string(),
                x: 1.0,
                y: 2.0,
                font_size: 16.0,
                color: Color::rgb(0x22, 0x22, 0x22),
            }]
        );
    }

    #[test]
    fn background_color_creates_draw_rect() {
        let commands =
            build_display_list(&block_box(Color::rgb(0xee, 0xee, 0xee), 0.0, Vec::new())).unwrap();
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands[0],
            DisplayCommand::DrawRect {
                color: Color { r: 0xee, .. },
                ..
            }
        ));
    }

    #[test]
    fn transparent_background_creates_no_draw_rect() {
        let commands = build_display_list(&block_box(Color::TRANSPARENT, 0.0, Vec::new())).unwrap();
        assert!(commands.is_empty());
    }

    #[test]
    fn border_creates_draw_border() {
        let commands = build_display_list(&block_box(Color::TRANSPARENT, 2.0, Vec::new())).unwrap();
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands[0],
            DisplayCommand::DrawBorder {
                border_width: 2.0,
                ..
            }
        ));
    }

    #[test]
    fn nested_layout_produces_commands_in_stable_order() {
        let tree = block_box(
            Color::rgb(1, 2, 3),
            0.0,
            vec![
                text_box("first", Color::BLACK),
                text_box("second", Color::BLACK),
            ],
        );
        let commands = build_display_list(&tree).unwrap();
        assert_eq!(commands.len(), 3);
        assert!(matches!(commands[0], DisplayCommand::DrawRect { .. }));
        assert!(matches!(&commands[1], DisplayCommand::DrawText { text, .. } if text == "first"));
        assert!(matches!(&commands[2], DisplayCommand::DrawText { text, .. } if text == "second"));
    }

    #[test]
    fn line_box_emits_no_command_but_its_runs_do() {
        let tree = block_box(
            Color::TRANSPARENT,
            0.0,
            vec![line_box(vec![text_box("hi", Color::BLACK)])],
        );
        let commands = build_display_list(&tree).unwrap();
        // No DrawRect for the transparent block or the line box; just the text.
        assert_eq!(commands.len(), 1);
        assert!(matches!(&commands[0], DisplayCommand::DrawText { text, .. } if text == "hi"));
    }

    #[test]
    fn empty_layout_does_not_panic() {
        let commands = build_display_list(&block_box(Color::TRANSPARENT, 0.0, Vec::new())).unwrap();
        assert!(commands.is_empty());
    }

    #[test]
    fn debug_line_is_readable() {
        let command = DisplayCommand::DrawText {
            text: "Hi".to_string(),
            x: 8.0,
            y: 8.0,
            font_size: 32.0,
            color: Color::rgb(0x22, 0x22, 0x22),
        };
        assert_eq!(
            command.to_debug_line(),
            "DrawText \"Hi\" x=8 y=8 font_size=32 color=#222222"
        );
    }
}
