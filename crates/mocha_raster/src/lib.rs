//! A small, pure-Rust software rasterizer for Mocha Browser (Milestone 11).
//!
//! It turns a [`DisplayCommand`] list plus the document's decoded images into an
//! RGBA pixel [`Surface`]. There is **no** GPU and no compositor; page text is
//! drawn with a crude built-in debug font (see [`font`]). It applies a vertical
//! scroll offset and clips every write to the surface bounds.
//!
//! For the desktop browser chrome, [`Surface`] also offers anti-aliased
//! geometry helpers (rounded rectangles, pills, lines) and coverage blending
//! ([`Surface::blend_pixel`]) so the shell can draw smooth UI shapes and
//! glyphs. Page content rendering itself remains un-anti-aliased.
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
    pub fn draw_rect_outline(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        thickness: i32,
        color: Color,
    ) {
        self.stroke_rect(x, y, w, h, thickness, color);
    }

    /// Draw text at device coordinates (public for chrome rendering).
    pub fn draw_text_at(&mut self, text: &str, x: i32, y: i32, scale: i32, color: Color) -> i32 {
        self.draw_text(text, x, y, scale, color)
    }

    /// Blend `color` at `(x, y)` scaled by `coverage` (0 = none, 255 = the
    /// colour's own alpha). Out-of-bounds coordinates are ignored. Public so
    /// the desktop shell can draw anti-aliased glyphs and shapes.
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: Color, coverage: u8) {
        if coverage == 0 {
            return;
        }
        let alpha = ((color.a as u32 * coverage as u32) / 255) as u8;
        self.blend(
            x,
            y,
            Color {
                r: color.r,
                g: color.g,
                b: color.b,
                a: alpha,
            },
        );
    }

    /// Fill a rectangle with anti-aliased rounded corners. `radii` is
    /// `[top-left, top-right, bottom-right, bottom-left]` in device pixels;
    /// each is clamped to half the rectangle's smaller side.
    fn fill_rounded(&mut self, x: f32, y: f32, w: f32, h: f32, radii: [f32; 4], color: Color) {
        if w <= 0.0 || h <= 0.0 || color.a == 0 {
            return;
        }
        let max_radius = (w.min(h)) / 2.0;
        let radii = radii.map(|r| r.clamp(0.0, max_radius));
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x + w).ceil() as i32;
        let y1 = (y + h).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let coverage =
                    rounded_rect_coverage(px as f32 + 0.5, py as f32 + 0.5, x, y, w, h, radii);
                if coverage > 0.0 {
                    self.blend_pixel(px, py, color, (coverage * 255.0) as u8);
                }
            }
        }
    }

    /// Fill an anti-aliased rounded rectangle (uniform corner radius).
    pub fn draw_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        self.fill_rounded(x, y, w, h, [radius; 4], color);
    }

    /// Fill a rectangle whose **top** corners are rounded (browser tabs).
    pub fn draw_rounded_rect_top(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    ) {
        self.fill_rounded(x, y, w, h, [radius, radius, 0.0, 0.0], color);
    }

    /// Stroke an anti-aliased rounded-rectangle outline of `thickness` pixels
    /// (drawn inward from the rectangle edge).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rounded_rect_outline(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        thickness: f32,
        color: Color,
    ) {
        if w <= 0.0 || h <= 0.0 || thickness <= 0.0 || color.a == 0 {
            return;
        }
        let max_radius = (w.min(h)) / 2.0;
        let radius = radius.clamp(0.0, max_radius);
        let t = thickness.min(max_radius.max(1.0));
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x + w).ceil() as i32;
        let y1 = (y + h).ceil() as i32;
        for py in y0..y1 {
            for px in x0..x1 {
                let cx = px as f32 + 0.5;
                let cy = py as f32 + 0.5;
                let outer = rounded_rect_coverage(cx, cy, x, y, w, h, [radius; 4]);
                let inner = rounded_rect_coverage(
                    cx,
                    cy,
                    x + t,
                    y + t,
                    w - 2.0 * t,
                    h - 2.0 * t,
                    [(radius - t).max(0.0); 4],
                );
                let coverage = (outer - inner).max(0.0);
                if coverage > 0.0 {
                    self.blend_pixel(px, py, color, (coverage * 255.0) as u8);
                }
            }
        }
    }

    /// Fill an anti-aliased pill (a rounded rect whose radius is half its
    /// height — the classic browser address-bar shape).
    pub fn draw_pill(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.fill_rounded(x, y, w, h, [h / 2.0; 4], color);
    }

    /// Stroke an anti-aliased pill outline of `thickness` pixels.
    pub fn draw_pill_outline(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        thickness: f32,
        color: Color,
    ) {
        self.draw_rounded_rect_outline(x, y, w, h, h / 2.0, thickness, color);
    }

    /// Draw an anti-aliased line segment of `thickness` device pixels between
    /// `(x0, y0)` and `(x1, y1)` (used for the chrome's vector icons).
    pub fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: Color) {
        if thickness <= 0.0 || color.a == 0 {
            return;
        }
        let half = thickness / 2.0;
        let min_x = (x0.min(x1) - half).floor() as i32;
        let max_x = (x0.max(x1) + half).ceil() as i32;
        let min_y = (y0.min(y1) - half).floor() as i32;
        let max_y = (y0.max(y1) + half).ceil() as i32;
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let distance =
                    point_segment_distance(px as f32 + 0.5, py as f32 + 0.5, x0, y0, x1, y1);
                let coverage = (half + 0.5 - distance).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    self.blend_pixel(px, py, color, (coverage * 255.0) as u8);
                }
            }
        }
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
    rasterize_at(surface, display_list, images, scroll_y, 0);
}

/// Like [`rasterize`], but the document is drawn starting `top_offset` device
/// pixels down the surface so it sits in its viewport region below the browser
/// chrome. The surface is cleared first; the caller is expected to paint opaque
/// chrome over the `[0, top_offset)` band afterwards (content scrolled above the
/// viewport top is drawn there and then covered).
pub fn rasterize_at(
    surface: &mut Surface,
    display_list: &[DisplayCommand],
    images: &[RasterImage],
    scroll_y: f32,
    top_offset: i32,
) {
    surface.clear(BACKGROUND);
    let offset = scroll_y.round() as i32 - top_offset;
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

/// Coverage (0.0..=1.0) of the point `(cx, cy)` inside the rounded rectangle
/// `(x, y, w, h)` with per-corner radii, using a signed-distance estimate for
/// a one-pixel anti-aliased edge.
fn rounded_rect_coverage(cx: f32, cy: f32, x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> f32 {
    if w <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    // Pick the radius of the corner quadrant this point falls in
    // (radii = [top-left, top-right, bottom-right, bottom-left]).
    let right = cx >= x + w / 2.0;
    let bottom = cy >= y + h / 2.0;
    let radius = match (right, bottom) {
        (false, false) => radii[0],
        (true, false) => radii[1],
        (true, true) => radii[2],
        (false, true) => radii[3],
    };
    // The canonical rounded-box signed distance: negative inside, zero on the
    // edge, positive outside. One pixel of smoothing gives the anti-aliasing.
    let qx = (cx - (x + w / 2.0)).abs() - (w / 2.0 - radius);
    let qy = (cy - (y + h / 2.0)).abs() - (h / 2.0 - radius);
    let outside = (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt();
    let inside = qx.max(qy).min(0.0);
    let distance = outside + inside - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Distance from point `(px, py)` to the segment `(x0, y0)`–`(x1, y1)`.
fn point_segment_distance(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let vx = x1 - x0;
    let vy = y1 - y0;
    let length_squared = vx * vx + vy * vy;
    let t = if length_squared == 0.0 {
        0.0
    } else {
        (((px - x0) * vx + (py - y0) * vy) / length_squared).clamp(0.0, 1.0)
    };
    let nearest_x = x0 + t * vx;
    let nearest_y = y0 + t * vy;
    let dx = px - nearest_x;
    let dy = py - nearest_y;
    (dx * dx + dy * dy).sqrt()
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

    // --- rounded/anti-aliased chrome drawing ---------------------------------

    #[test]
    fn rounded_rect_draws_non_empty_pixels() {
        let mut surface = Surface::new(40, 40);
        surface.draw_rounded_rect(4.0, 4.0, 32.0, 24.0, 8.0, red());
        assert!(count_non_background(&surface) > 0);
        // The center is fully covered.
        assert_eq!(surface.pixel(20, 16), Some(pack(red())));
    }

    #[test]
    fn rounded_rect_corners_differ_from_square_corners() {
        let mut rounded = Surface::new(40, 40);
        rounded.draw_rounded_rect(0.0, 0.0, 32.0, 32.0, 10.0, red());
        let mut square = Surface::new(40, 40);
        square.draw_rect(0, 0, 32, 32, red());
        // The square's corner pixel is solid; the rounded one stays background.
        assert_eq!(square.pixel(0, 0), Some(pack(red())));
        assert_eq!(rounded.pixel(0, 0), Some(pack(BACKGROUND)));
        // Straight edges still painted on both.
        assert_eq!(rounded.pixel(16, 0), Some(pack(red())));
    }

    #[test]
    fn rounded_rect_top_keeps_square_bottom_corners() {
        let mut surface = Surface::new(40, 40);
        surface.draw_rounded_rect_top(0.0, 0.0, 32.0, 32.0, 10.0, red());
        assert_eq!(
            surface.pixel(0, 0),
            Some(pack(BACKGROUND)),
            "top corner rounded"
        );
        assert_eq!(
            surface.pixel(0, 31),
            Some(pack(red())),
            "bottom corner square"
        );
    }

    #[test]
    fn rounded_outline_draws_border_not_interior() {
        let mut surface = Surface::new(40, 40);
        surface.draw_rounded_rect_outline(2.0, 2.0, 30.0, 30.0, 8.0, 2.0, red());
        // Top edge midpoint is stroked; the interior stays background.
        assert_eq!(surface.pixel(17, 2), Some(pack(red())));
        assert_eq!(surface.pixel(17, 17), Some(pack(BACKGROUND)));
        // The far corner pixel outside the radius stays background.
        assert_eq!(surface.pixel(2, 2), Some(pack(BACKGROUND)));
    }

    #[test]
    fn pill_rounds_to_half_height() {
        let mut surface = Surface::new(60, 20);
        surface.draw_pill(0.0, 0.0, 56.0, 16.0, red());
        // Center solid, extreme corners empty (radius = 8).
        assert_eq!(surface.pixel(28, 8), Some(pack(red())));
        assert_eq!(surface.pixel(0, 0), Some(pack(BACKGROUND)));
        assert_eq!(surface.pixel(55, 15), Some(pack(BACKGROUND)));
    }

    #[test]
    fn line_draws_along_its_path_with_thickness() {
        let mut surface = Surface::new(30, 30);
        surface.draw_line(4.0, 15.0, 26.0, 15.0, 3.0, red());
        assert_eq!(surface.pixel(15, 14), Some(pack(red())), "on the line");
        assert_eq!(
            surface.pixel(15, 4),
            Some(pack(BACKGROUND)),
            "far from line"
        );
        // Diagonals draw too (anti-aliased, so just require non-background).
        let mut diagonal = Surface::new(30, 30);
        diagonal.draw_line(4.0, 4.0, 26.0, 26.0, 2.0, red());
        assert!(diagonal.pixel(15, 15) != Some(pack(BACKGROUND)));
    }

    #[test]
    fn blend_pixel_coverage_mixes_with_background() {
        let mut surface = Surface::new(4, 4);
        surface.blend_pixel(1, 1, red(), 128);
        let pixel = surface.pixel(1, 1).unwrap();
        assert_ne!(pixel, pack(red()), "half coverage is not solid");
        assert_ne!(pixel, pack(BACKGROUND), "half coverage is not background");
        // Out of bounds is clipped, zero coverage is a no-op.
        surface.blend_pixel(-1, 99, red(), 255);
        surface.blend_pixel(2, 2, red(), 0);
        assert_eq!(surface.pixel(2, 2), Some(pack(BACKGROUND)));
    }
}
