//! Vector toolbar/tab icons, drawn with anti-aliased lines.
//!
//! No image assets and no third-party icon fonts: every icon is a handful of
//! line segments sized relative to the button rectangle, so they stay crisp at
//! any chrome scale and inherit their colour from the theme.

use mocha_layout::Color;
use mocha_raster::Surface;

use crate::chrome::Rect;

/// Stroke width relative to the icon size.
const STROKE_FRACTION: f32 = 0.09;

/// The icon's center and half-extent within `rect` (icons fill ~42% of the
/// button so they read as glyphs, not as boxes).
fn geometry(rect: Rect) -> (f32, f32, f32, f32) {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let size = rect.width.min(rect.height) * 0.42;
    let stroke = (rect.width.min(rect.height) * STROKE_FRACTION).max(1.4);
    (cx, cy, size, stroke)
}

/// Left arrow: shaft plus chevron head.
pub fn draw_back_icon(surface: &mut Surface, rect: Rect, color: Color) {
    let (cx, cy, s, t) = geometry(rect);
    surface.draw_line(cx - s, cy, cx + s, cy, t, color);
    surface.draw_line(cx - s, cy, cx - s * 0.1, cy - s * 0.85, t, color);
    surface.draw_line(cx - s, cy, cx - s * 0.1, cy + s * 0.85, t, color);
}

/// Right arrow: shaft plus chevron head.
pub fn draw_forward_icon(surface: &mut Surface, rect: Rect, color: Color) {
    let (cx, cy, s, t) = geometry(rect);
    surface.draw_line(cx - s, cy, cx + s, cy, t, color);
    surface.draw_line(cx + s, cy, cx + s * 0.1, cy - s * 0.85, t, color);
    surface.draw_line(cx + s, cy, cx + s * 0.1, cy + s * 0.85, t, color);
}

/// Circular-arrow reload: an arc with a gap, ending in an arrowhead.
pub fn draw_reload_icon(surface: &mut Surface, rect: Rect, color: Color) {
    let (cx, cy, s, t) = geometry(rect);
    let radius = s * 0.9;
    // Arc from 30° to 300° (leaving a gap at the upper right for the head).
    let start = 30.0_f32.to_radians();
    let end = 300.0_f32.to_radians();
    let segments = 24;
    let mut previous = (cx + radius * start.cos(), cy - radius * start.sin());
    for i in 1..=segments {
        let angle = start + (end - start) * (i as f32 / segments as f32);
        let next = (cx + radius * angle.cos(), cy - radius * angle.sin());
        surface.draw_line(previous.0, previous.1, next.0, next.1, t, color);
        previous = next;
    }
    // Arrowhead at the arc start, pointing along the (clockwise) tangent.
    let head_x = cx + radius * start.cos();
    let head_y = cy - radius * start.sin();
    surface.draw_line(head_x, head_y, head_x + s * 0.55, head_y - s * 0.2, t, color);
    surface.draw_line(head_x, head_y, head_x - s * 0.05, head_y - s * 0.65, t, color);
}

/// A small house: roof above a body with a flat base.
pub fn draw_home_icon(surface: &mut Surface, rect: Rect, color: Color) {
    let (cx, cy, s, t) = geometry(rect);
    let roof_y = cy - s;
    let eave_y = cy - s * 0.1;
    let base_y = cy + s;
    let half = s * 0.95;
    let body = s * 0.7;
    // Roof.
    surface.draw_line(cx - half, eave_y, cx, roof_y, t, color);
    surface.draw_line(cx, roof_y, cx + half, eave_y, t, color);
    // Body (walls + floor).
    surface.draw_line(cx - body, eave_y, cx - body, base_y, t, color);
    surface.draw_line(cx + body, eave_y, cx + body, base_y, t, color);
    surface.draw_line(cx - body, base_y, cx + body, base_y, t, color);
}

/// A plus sign (new tab).
pub fn draw_plus_icon(surface: &mut Surface, rect: Rect, color: Color) {
    let (cx, cy, s, t) = geometry(rect);
    surface.draw_line(cx - s * 0.8, cy, cx + s * 0.8, cy, t, color);
    surface.draw_line(cx, cy - s * 0.8, cx, cy + s * 0.8, t, color);
}

/// An × (close tab).
pub fn draw_close_icon(surface: &mut Surface, rect: Rect, color: Color) {
    let (cx, cy, s, t) = geometry(rect);
    let r = s * 0.7;
    surface.draw_line(cx - r, cy - r, cx + r, cy + r, t, color);
    surface.draw_line(cx - r, cy + r, cx + r, cy - r, t, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink() -> Color {
        Color {
            r: 30,
            g: 30,
            b: 30,
            a: 255,
        }
    }

    fn rect() -> Rect {
        Rect::new(0.0, 0.0, 32.0, 32.0)
    }

    fn painted(draw: impl Fn(&mut Surface, Rect, Color)) -> Vec<u32> {
        let mut surface = Surface::new(32, 32);
        draw(&mut surface, rect(), ink());
        surface.buffer().to_vec()
    }

    fn non_background(buffer: &[u32]) -> usize {
        buffer.iter().filter(|&&p| p != 0x00ff_ffff).count()
    }

    #[test]
    fn every_icon_changes_pixels() {
        for draw in [
            draw_back_icon,
            draw_forward_icon,
            draw_reload_icon,
            draw_home_icon,
            draw_plus_icon,
            draw_close_icon,
        ] {
            assert!(non_background(&painted(draw)) > 10);
        }
    }

    #[test]
    fn icons_are_distinct_shapes() {
        assert_ne!(painted(draw_back_icon), painted(draw_forward_icon));
        assert_ne!(painted(draw_plus_icon), painted(draw_close_icon));
        assert_ne!(painted(draw_reload_icon), painted(draw_home_icon));
    }

    #[test]
    fn icon_color_is_applied() {
        // A disabled (lighter) colour paints different pixel values.
        let mut strong = Surface::new(32, 32);
        draw_back_icon(&mut strong, rect(), ink());
        let mut faded = Surface::new(32, 32);
        draw_back_icon(
            &mut faded,
            rect(),
            Color {
                r: 200,
                g: 200,
                b: 205,
                a: 255,
            },
        );
        assert_ne!(strong.buffer(), faded.buffer());
    }
}
