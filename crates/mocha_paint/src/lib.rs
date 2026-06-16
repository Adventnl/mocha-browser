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
    /// Fill a rounded rectangle (background with `border-radius`).
    DrawRoundedRect {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Width.
        width: f32,
        /// Height.
        height: f32,
        /// Corner radius.
        radius: f32,
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
    /// Draw a rounded box border (border with `border-radius`).
    DrawRoundedBorder {
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
        /// Corner radius.
        radius: f32,
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
    /// Draw a decoded image (a replaced element). The paint layer emits this
    /// command; `mocha_raster` (Milestone 11) resolves the pixels onto the
    /// desktop window surface.
    DrawImage {
        /// Index into the document's image store.
        image_id: usize,
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Draw width.
        width: f32,
        /// Draw height.
        height: f32,
    },
    /// Draw a form control. Like images, controls are **not** rasterized to a
    /// real widget or window — the command carries everything a future surface
    /// would need.
    DrawControl {
        /// The normalized control type (`"text"`, `"checkbox"`, `"button"`, …).
        control_type: String,
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Draw width.
        width: f32,
        /// Draw height.
        height: f32,
        /// The current value (text controls) or visible label (buttons), if any.
        value: Option<String>,
        /// The checked state for checkboxes/radios; `None` for other controls.
        checked: Option<bool>,
        /// Whether the control is disabled.
        disabled: bool,
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
            DisplayCommand::DrawRoundedRect {
                x,
                y,
                width,
                height,
                radius,
                color,
            } => format!(
                "DrawRoundedRect x={x} y={y} width={width} height={height} radius={radius} color={color}"
            ),
            DisplayCommand::DrawRoundedBorder {
                x,
                y,
                width,
                height,
                border_width,
                radius,
                color,
            } => format!(
                "DrawRoundedBorder x={x} y={y} width={width} height={height} border_width={border_width} radius={radius} color={color}"
            ),
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
            DisplayCommand::DrawImage {
                image_id,
                x,
                y,
                width,
                height,
            } => format!("DrawImage image_id={image_id} x={x} y={y} width={width} height={height}"),
            DisplayCommand::DrawControl {
                control_type,
                x,
                y,
                width,
                height,
                value,
                checked,
                disabled,
            } => {
                let mut line = format!(
                    "DrawControl type={control_type} x={x} y={y} width={width} height={height}"
                );
                if let Some(value) = value {
                    line.push_str(&format!(" value={value:?}"));
                }
                if let Some(checked) = checked {
                    line.push_str(&format!(" checked={checked}"));
                }
                line.push_str(&format!(" disabled={disabled}"));
                line
            }
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

/// Emit a background fill, rounded when the box has a `border-radius`.
fn emit_background(commands: &mut Vec<DisplayCommand>, b: &LayoutBox) {
    if b.background_color.a == 0 {
        return;
    }
    let r = b.rect;
    if b.border_radius > 0.0 {
        commands.push(DisplayCommand::DrawRoundedRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
            radius: b.border_radius,
            color: b.background_color,
        });
    } else {
        commands.push(DisplayCommand::DrawRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
            color: b.background_color,
        });
    }
}

/// Emit a border, rounded when the box has a `border-radius`.
fn emit_border(commands: &mut Vec<DisplayCommand>, b: &LayoutBox) {
    if b.border_width <= 0.0 {
        return;
    }
    let r = b.rect;
    if b.border_radius > 0.0 {
        commands.push(DisplayCommand::DrawRoundedBorder {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
            border_width: b.border_width,
            radius: b.border_radius,
            color: b.border_color,
        });
    } else {
        commands.push(DisplayCommand::DrawBorder {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
            border_width: b.border_width,
            color: b.border_color,
        });
    }
}

fn paint_box(layout_box: &LayoutBox, commands: &mut Vec<DisplayCommand>) {
    let rect = layout_box.rect;
    match &layout_box.kind {
        // Box-generating boxes paint their background, then border, before their
        // children (so text draws on top). Anonymous blocks and line boxes carry
        // no styling, so they emit nothing themselves and just recurse.
        LayoutBoxKind::Block | LayoutBoxKind::Inline | LayoutBoxKind::AnonymousBlock => {
            emit_background(commands, layout_box);
            emit_border(commands, layout_box);
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
        LayoutBoxKind::Image(image_id) => {
            // A replaced element draws its (optional) background/border, then the
            // image fills the box.
            emit_background(commands, layout_box);
            emit_border(commands, layout_box);
            commands.push(DisplayCommand::DrawImage {
                image_id: *image_id,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            });
        }
        LayoutBoxKind::Control(control) => {
            // A form control draws like a replaced element: optional
            // background/border, then the control fills the box.
            emit_background(commands, layout_box);
            commands.push(DisplayCommand::DrawControl {
                control_type: control.control_type.clone(),
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                value: control.value.clone(),
                checked: control.checked,
                disabled: control.disabled,
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
            border_radius: 0.0,
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
            border_radius: 0.0,
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
            border_radius: 0.0,
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

    fn control_box(
        control_type: &str,
        value: Option<&str>,
        checked: Option<bool>,
        disabled: bool,
    ) -> LayoutBox {
        LayoutBox {
            node_id: Some(NodeId(1)),
            kind: LayoutBoxKind::Control(mocha_layout::ControlBox {
                control_type: control_type.to_string(),
                value: value.map(str::to_string),
                checked,
                disabled,
                width: 3.0,
                height: 4.0,
            }),
            rect: rect(),
            font_size: 0.0,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            border_width: 0.0,
            border_color: Color::BLACK,
            border_radius: 0.0,
            children: Vec::new(),
        }
    }

    #[test]
    fn text_input_emits_draw_control_with_value() {
        let commands =
            build_display_list(&control_box("text", Some("hello"), None, false)).unwrap();
        assert_eq!(
            commands,
            vec![DisplayCommand::DrawControl {
                control_type: "text".to_string(),
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
                value: Some("hello".to_string()),
                checked: None,
                disabled: false,
            }]
        );
    }

    #[test]
    fn checkbox_emits_draw_control_with_checked_state() {
        let commands =
            build_display_list(&control_box("checkbox", None, Some(true), false)).unwrap();
        assert!(matches!(
            &commands[0],
            DisplayCommand::DrawControl {
                control_type,
                checked: Some(true),
                ..
            } if control_type == "checkbox"
        ));
    }

    #[test]
    fn disabled_state_is_included_in_draw_control() {
        let commands = build_display_list(&control_box("button", Some("Go"), None, true)).unwrap();
        assert!(matches!(
            &commands[0],
            DisplayCommand::DrawControl { disabled: true, .. }
        ));
    }

    #[test]
    fn control_debug_line_is_readable() {
        let checkbox = control_box("checkbox", None, Some(true), false);
        let commands = build_display_list(&checkbox).unwrap();
        assert_eq!(
            commands[0].to_debug_line(),
            "DrawControl type=checkbox x=1 y=2 width=3 height=4 checked=true disabled=false"
        );

        let text = control_box("text", Some("hi"), None, true);
        let commands = build_display_list(&text).unwrap();
        assert_eq!(
            commands[0].to_debug_line(),
            "DrawControl type=text x=1 y=2 width=3 height=4 value=\"hi\" disabled=true"
        );
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
