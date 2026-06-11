//! A small, pure-Rust software rasterizer for Mocha Browser (Milestone 11).
//!
//! It turns a [`DisplayCommand`] list plus the document's decoded images into an
//! RGBA pixel [`Surface`]. There is **no** GPU, no compositor, no anti-aliasing,
//! and text is drawn with a crude built-in debug font (see [`font`]). It applies
//! a vertical scroll offset and clips every write to the surface bounds.
//!
//! This crate knows nothing about windowing, layout, the DOM, or networking — it
//! consumes the engine's output and writes pixels.

mod font;

use mocha_image::RasterImage;
use mocha_layout::Color;
use mocha_paint::DisplayCommand;

pub use font::{GLYPH_ADVANCE, GLYPH_HEIGHT, GLYPH_WIDTH};

/// The page background (and the colour the surface is cleared to).
pub const BACKGROUND: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// An RGBA pixel surface. Pixels are stored as `0x00RRGGBB` (opaque; the top byte
/// is unused so the buffer can be handed straight to a 32-bit window backend).
pub struct Surface {
    width: u32,
    height: u32,
    buffer: Vec<u32>,
}

impl Surface {
    /// A new surface of `width`×`height`, cleared to [`BACKGROUND`].
    pub fn new(width: u32, height: u32) -> Surface {
        let mut surface = Surface {
            width,
            height,
            buffer: vec![0; (width as usize) * (height as usize)],
        };
        surface.clear(BACKGROUND);
        surface
    }

    /// Surface width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Surface height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The raw `0x00RRGGBB` pixel buffer, row-major (for a window backend).
    pub fn buffer(&self) -> &[u32] {
        &self.buffer
    }

    /// Fill the whole surface with `color`.
    pub fn clear(&mut self, color: Color) {
        let packed = pack(color);
        for pixel in &mut self.buffer {
            *pixel = packed;
        }
    }

    /// The packed `0x00RRGGBB` value at `(x, y)`, or `None` if out of bounds
    /// (used by tests).
    pub fn pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x < self.width && y < self.height {
            Some(self.buffer[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    /// Blend `color` over the pixel at `(x, y)` using its alpha. Out-of-bounds
    /// coordinates are ignored (clipping).
    fn blend(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height || color.a == 0 {
            return;
        }
        let index = (y as u32 * self.width + x as u32) as usize;
        let dst = self.buffer[index];
        self.buffer[index] = if color.a == 255 {
            pack(color)
        } else {
            blend_over(dst, color)
        };
    }

    /// Fill the rectangle (clipped) at integer device coordinates.
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if color.a == 0 {
            return;
        }
        for dy in 0..h.max(0) {
            for dx in 0..w.max(0) {
                self.blend(x + dx, y + dy, color);
            }
        }
    }

    /// Stroke a rectangle outline of `thickness` device pixels.
    fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, thickness: i32, color: Color) {
        let t = thickness.clamp(0, h.max(0)).min(w.max(0));
        if t == 0 || color.a == 0 {
            return;
        }
        self.fill_rect(x, y, w, t, color); // top
        self.fill_rect(x, y + h - t, w, t, color); // bottom
        self.fill_rect(x, y, t, h, color); // left
        self.fill_rect(x + w - t, y, t, h, color); // right
    }

    /// Draw a single debug-font glyph at device `(x, y)` (top-left), scaled by
    /// `scale` device pixels per dot.
    fn draw_glyph(&mut self, c: char, x: i32, y: i32, scale: i32, color: Color) {
        let rows = font::glyph(c);
        for (row_index, row) in rows.iter().enumerate() {
            for col in 0..GLYPH_WIDTH {
                // bit 4 is the leftmost column.
                let bit = 1u8 << (GLYPH_WIDTH - 1 - col);
                if row & bit != 0 {
                    let px = x + col as i32 * scale;
                    let py = y + row_index as i32 * scale;
                    self.fill_rect(px, py, scale, scale, color);
                }
            }
        }
    }

    /// Draw a run of debug text at device `(x, y)`, returning the device width
    /// consumed.
    fn draw_text(&mut self, text: &str, x: i32, y: i32, scale: i32, color: Color) -> i32 {
        let mut cursor = x;
        for c in text.chars() {
            self.draw_glyph(c, cursor, y, scale, color);
            cursor += GLYPH_ADVANCE as i32 * scale;
        }
        cursor - x
    }

    /// Draw a filled rectangle at device coordinates (public for chrome rendering).
    pub fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        self.fill_rect(x, y, w, h, color);
    }

    /// Draw a stroked rectangle at device coordinates (public for chrome rendering).
    pub fn draw_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, thickness: i32, color: Color) {
        self.stroke_rect(x, y, w, h, thickness, color);
    }

    /// Draw text at device coordinates (public for chrome rendering).
    pub fn draw_text_at(&mut self, text: &str, x: i32, y: i32, scale: i32, color: Color) -> i32 {
        self.draw_text(text, x, y, scale, color)
    }

    /// Nearest-neighbour blit of an RGBA image into the device rect `(x, y, w, h)`.
    fn draw_image(&mut self, image: &RasterImage, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 || image.width == 0 || image.height == 0 {
            return;
        }
        for dy in 0..h {
            let src_y = (dy as u32 * image.height / h as u32).min(image.height - 1);
            for dx in 0..w {
                let src_x = (dx as u32 * image.width / w as u32).min(image.width - 1);
                let i = ((src_y * image.width + src_x) * 4) as usize;
                let color = Color {
                    r: image.rgba[i],
                    g: image.rgba[i + 1],
                    b: image.rgba[i + 2],
                    a: image.rgba[i + 3],
                };
                self.blend(x + dx, y + dy, color);
            }
        }
    }
}

/// Rasterize `display_list` onto `surface`, scrolled down by `scroll_y` device
/// pixels. The surface is cleared to [`BACKGROUND`] first. `images` is indexed by
/// the `image_id` carried in `DrawImage` commands; a missing id is skipped.
pub fn rasterize(
    surface: &mut Surface,
    display_list: &[DisplayCommand],
    images: &[RasterImage],
    scroll_y: f32,
) {
    surface.clear(BACKGROUND);
    let offset = scroll_y.round() as i32;
    // Text/control labels are scaled so a 16px font reads roughly right with the
    // 7-dot-tall debug glyphs.
    for command in display_list {
        match command {
            DisplayCommand::DrawRect {
                x,
                y,
                width,
                height,
                color,
            } => {
                surface.fill_rect(px(*x), px(*y) - offset, px(*width), px(*height), *color);
            }
            DisplayCommand::DrawBorder {
                x,
                y,
                width,
                height,
                border_width,
                color,
            } => {
                surface.stroke_rect(
                    px(*x),
                    px(*y) - offset,
                    px(*width),
                    px(*height),
                    px(*border_width).max(1),
                    *color,
                );
            }
            DisplayCommand::DrawText {
                text,
                x,
                y,
                font_size,
                color,
            } => {
                surface.draw_text(
                    text,
                    px(*x),
                    px(*y) - offset,
                    text_scale(*font_size),
                    *color,
                );
            }
            DisplayCommand::DrawImage {
                image_id,
                x,
                y,
                width,
                height,
            } => {
                if let Some(image) = images.get(*image_id) {
                    surface.draw_image(image, px(*x), px(*y) - offset, px(*width), px(*height));
                }
            }
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
                draw_control(
                    surface,
                    control_type,
                    px(*x),
                    px(*y) - offset,
                    px(*width),
                    px(*height),
                    value.as_deref(),
                    *checked,
                    *disabled,
                );
            }
        }
    }
}

/// Draw a crude representation of a form control. Not a real widget: a bordered
/// box, plus a check mark / label as appropriate.
#[allow(clippy::too_many_arguments)]
fn draw_control(
    surface: &mut Surface,
    control_type: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    value: Option<&str>,
    checked: Option<bool>,
    disabled: bool,
) {
    let face = if disabled {
        Color {
            r: 224,
            g: 224,
            b: 224,
            a: 255,
        }
    } else {
        Color {
            r: 245,
            g: 245,
            b: 245,
            a: 255,
        }
    };
    let border = Color {
        r: 90,
        g: 90,
        b: 90,
        a: 255,
    };
    let ink = if disabled {
        Color {
            r: 150,
            g: 150,
            b: 150,
            a: 255,
        }
    } else {
        Color {
            r: 20,
            g: 20,
            b: 20,
            a: 255,
        }
    };

    match control_type {
        "checkbox" | "radio" => {
            surface.fill_rect(x, y, w, h, face);
            surface.stroke_rect(x, y, w, h, 1, border);
            if checked == Some(true) {
                // A filled inner square marks the checked state.
                surface.fill_rect(x + 3, y + 3, (w - 6).max(1), (h - 6).max(1), ink);
            }
        }
        _ => {
            // text / password / select / button: bordered box + label/value.
            surface.fill_rect(x, y, w, h, face);
            surface.stroke_rect(x, y, w, h, 1, border);
            if let Some(text) = value {
                let masked = if control_type == "password" {
                    "*".repeat(text.chars().count())
                } else {
                    text.to_string()
                };
                surface.draw_text(&masked, x + 3, y + 3, 1, ink);
            }
        }
    }
}

/// Round a CSS px float to a device pixel.
fn px(value: f32) -> i32 {
    value.round() as i32
}

/// Device pixels per debug-font dot for a given CSS font size. The debug glyph is
/// 7 dots tall; this keeps it within the line without real metrics.
fn text_scale(font_size: f32) -> i32 {
    ((font_size / GLYPH_HEIGHT as f32).round() as i32).max(1)
}

/// Pack an opaque-ish colour to `0x00RRGGBB` (alpha dropped; the surface is
/// opaque).
fn pack(color: Color) -> u32 {
    ((color.r as u32) << 16) | ((color.g as u32) << 8) | (color.b as u32)
}

/// Alpha-blend `src` (with its own alpha) over a packed `0x00RRGGBB` destination.
fn blend_over(dst: u32, src: Color) -> u32 {
    let a = src.a as u32;
    let inv = 255 - a;
    let dr = (dst >> 16) & 0xff;
    let dg = (dst >> 8) & 0xff;
    let db = dst & 0xff;
    let r = (src.r as u32 * a + dr * inv) / 255;
    let g = (src.g as u32 * a + dg * inv) / 255;
    let b = (src.b as u32 * a + db * inv) / 255;
    (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32, color: Color) -> DisplayCommand {
        DisplayCommand::DrawRect {
            x,
            y,
            width: w,
            height: h,
            color,
        }
    }

    fn red() -> Color {
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    fn count_non_background(surface: &Surface) -> usize {
        let bg = pack(BACKGROUND);
        surface.buffer().iter().filter(|&&p| p != bg).count()
    }

    #[test]
    fn new_surface_is_cleared_to_white() {
        let surface = Surface::new(4, 4);
        assert_eq!(surface.pixel(0, 0), Some(pack(BACKGROUND)));
        assert_eq!(count_non_background(&surface), 0);
    }

    #[test]
    fn draw_rect_fills_expected_pixels() {
        let mut surface = Surface::new(10, 10);
        rasterize(&mut surface, &[rect(2.0, 3.0, 4.0, 5.0, red())], &[], 0.0);
        assert_eq!(surface.pixel(2, 3), Some(pack(red())));
        assert_eq!(surface.pixel(5, 7), Some(pack(red())));
        // Just outside the rect stays background.
        assert_eq!(surface.pixel(6, 3), Some(pack(BACKGROUND)));
        assert_eq!(count_non_background(&surface), 4 * 5);
    }

    #[test]
    fn draw_border_only_strokes_the_edge() {
        let mut surface = Surface::new(10, 10);
        let cmd = DisplayCommand::DrawBorder {
            x: 1.0,
            y: 1.0,
            width: 6.0,
            height: 6.0,
            border_width: 1.0,
            color: red(),
        };
        rasterize(&mut surface, &[cmd], &[], 0.0);
        assert_eq!(surface.pixel(1, 1), Some(pack(red())), "corner stroked");
        assert_eq!(surface.pixel(6, 6), Some(pack(red())), "far corner stroked");
        assert_eq!(
            surface.pixel(3, 3),
            Some(pack(BACKGROUND)),
            "interior not filled"
        );
    }

    #[test]
    fn draw_text_changes_pixels() {
        let mut surface = Surface::new(80, 20);
        let cmd = DisplayCommand::DrawText {
            text: "Hi".to_string(),
            x: 1.0,
            y: 1.0,
            font_size: 16.0,
            color: red(),
        };
        rasterize(&mut surface, &[cmd], &[], 0.0);
        assert!(count_non_background(&surface) > 0, "text drew some pixels");
    }

    #[test]
    fn draw_image_blits_pixels() {
        let image = RasterImage {
            width: 2,
            height: 2,
            rgba: vec![
                10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255,
            ],
        };
        let mut surface = Surface::new(8, 8);
        let cmd = DisplayCommand::DrawImage {
            image_id: 0,
            x: 1.0,
            y: 1.0,
            width: 4.0,
            height: 4.0,
        };
        rasterize(&mut surface, &[cmd], std::slice::from_ref(&image), 0.0);
        assert_eq!(
            surface.pixel(2, 2),
            Some(pack(Color {
                r: 10,
                g: 20,
                b: 30,
                a: 255
            }))
        );
        assert_eq!(count_non_background(&surface), 4 * 4);
    }

    #[test]
    fn missing_image_id_is_skipped() {
        let mut surface = Surface::new(8, 8);
        let cmd = DisplayCommand::DrawImage {
            image_id: 7,
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
        };
        rasterize(&mut surface, &[cmd], &[], 0.0);
        assert_eq!(count_non_background(&surface), 0);
    }

    fn checkbox(checked: bool) -> DisplayCommand {
        DisplayCommand::DrawControl {
            control_type: "checkbox".to_string(),
            x: 0.0,
            y: 0.0,
            width: 13.0,
            height: 13.0,
            value: None,
            checked: Some(checked),
            disabled: false,
        }
    }

    #[test]
    fn draw_control_checkbox_marks_checked_state() {
        let mut a = Surface::new(20, 20);
        rasterize(&mut a, &[checkbox(true)], &[], 0.0);
        let mut b = Surface::new(20, 20);
        rasterize(&mut b, &[checkbox(false)], &[], 0.0);
        // The check mark recolors the inner cell, so the center pixel differs.
        assert_ne!(
            a.pixel(6, 6),
            b.pixel(6, 6),
            "checked box has a mark the empty one lacks"
        );
        // Both still draw the box (non-empty).
        assert!(count_non_background(&a) > 0);
        assert!(count_non_background(&b) > 0);
    }

    #[test]
    fn scroll_offset_shifts_rendering_up() {
        // A rect at y=10, scrolled by 10, should land at device y=0.
        let mut surface = Surface::new(10, 10);
        rasterize(&mut surface, &[rect(0.0, 10.0, 4.0, 4.0, red())], &[], 10.0);
        assert_eq!(surface.pixel(0, 0), Some(pack(red())));
        assert_eq!(surface.pixel(0, 5), Some(pack(BACKGROUND)));
    }

    #[test]
    fn drawing_is_clipped_to_the_surface() {
        // A rect partly off the right/bottom edge must not panic or wrap.
        let mut surface = Surface::new(8, 8);
        rasterize(&mut surface, &[rect(6.0, 6.0, 10.0, 10.0, red())], &[], 0.0);
        assert_eq!(surface.pixel(7, 7), Some(pack(red())));
        // Only the in-bounds 2x2 corner is painted.
        assert_eq!(count_non_background(&surface), 4);
    }

    #[test]
    fn negative_scroll_region_is_clipped() {
        // A rect above the viewport (scrolled past) writes nothing.
        let mut surface = Surface::new(8, 8);
        rasterize(&mut surface, &[rect(0.0, 0.0, 4.0, 4.0, red())], &[], 100.0);
        assert_eq!(count_non_background(&surface), 0);
    }
}
