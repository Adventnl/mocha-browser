//! Sans-serif text for the browser chrome and native views.
//!
//! [`Fonts`] rasterizes anti-aliased glyphs with `fontdue` using a sans font
//! loaded **from the operating system at runtime** (Segoe UI on Windows, with
//! Arial/DejaVu/Liberation fallbacks; no font files ship in the repository).
//! When no system font can be found, every API degrades to the built-in debug
//! pixel font so headless tests and minimal systems keep working.
//!
//! Scope: this is UI text only — horizontal left-to-right runs with kerning,
//! no shaping, no bidi, no complex scripts, no font fallback per glyph. Web
//! page content still renders through the engine's debug font (real page text
//! quality is a separate engine milestone).

use std::collections::HashMap;
use std::fmt;

use mocha_layout::Color;
use mocha_raster::{Surface, GLYPH_ADVANCE, GLYPH_HEIGHT};

/// Candidate system font files, tried in order.
const FONT_CANDIDATES: &[&str] = &[
    // Windows (resolved against %SystemRoot%\Fonts as well).
    r"C:\Windows\Fonts\segoeui.ttf",
    r"C:\Windows\Fonts\arial.ttf",
    r"C:\Windows\Fonts\calibri.ttf",
    r"C:\Windows\Fonts\tahoma.ttf",
    // Linux.
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    // macOS.
    "/Library/Fonts/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
];

/// A loaded sans font plus a per-(glyph, size) rasterization cache.
struct SansFont {
    font: fontdue::Font,
    cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
}

impl SansFont {
    fn from_bytes(bytes: &[u8]) -> Option<SansFont> {
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()?;
        Some(SansFont {
            font,
            cache: HashMap::new(),
        })
    }

    fn glyph(&mut self, c: char, size: f32) -> &(fontdue::Metrics, Vec<u8>) {
        let key = (c, (size * 10.0) as u32);
        let font = &self.font;
        self.cache
            .entry(key)
            .or_insert_with(|| font.rasterize(c, size))
    }
}

/// Chrome text drawing/measuring with automatic fallback to the debug font.
pub struct Fonts {
    sans: Option<SansFont>,
}

impl fmt::Debug for Fonts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fonts")
            .field("sans", &self.sans.is_some())
            .finish()
    }
}

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts::load()
    }
}

impl Fonts {
    /// Load the first available system sans font (or none — the debug-font
    /// fallback keeps everything working).
    pub fn load() -> Fonts {
        let system_root = std::env::var_os("SystemRoot");
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Some(root) = &system_root {
            for name in ["segoeui.ttf", "arial.ttf", "calibri.ttf", "tahoma.ttf"] {
                candidates.push(std::path::Path::new(root).join("Fonts").join(name));
            }
        }
        candidates.extend(FONT_CANDIDATES.iter().map(std::path::PathBuf::from));

        for path in candidates {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Some(sans) = SansFont::from_bytes(&bytes) {
                    return Fonts { sans: Some(sans) };
                }
            }
        }
        Fonts { sans: None }
    }

    /// A `Fonts` with no sans font (debug-font fallback), for tests.
    pub fn fallback_only() -> Fonts {
        Fonts { sans: None }
    }

    /// Whether a real (anti-aliased) sans font is available.
    pub fn has_sans(&self) -> bool {
        self.sans.is_some()
    }

    /// Draw one line of text with its top-left at `(x, y)` and a nominal line
    /// size of `size` pixels. Returns the advance width actually consumed.
    pub fn draw(
        &mut self,
        surface: &mut Surface,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
    ) -> f32 {
        match &mut self.sans {
            Some(sans) => {
                let ascent = sans
                    .font
                    .horizontal_line_metrics(size)
                    .map(|m| m.ascent)
                    .unwrap_or(size * 0.8);
                let baseline = y + ascent;
                let mut pen = x;
                let mut previous: Option<char> = None;
                for c in text.chars() {
                    if let Some(p) = previous {
                        if let Some(kern) = sans.font.horizontal_kern(p, c, size) {
                            pen += kern;
                        }
                    }
                    let (metrics, bitmap) = sans.glyph(c, size);
                    let metrics = *metrics;
                    let glyph_x = (pen + metrics.xmin as f32).round() as i32;
                    let glyph_y =
                        (baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;
                    for row in 0..metrics.height {
                        for col in 0..metrics.width {
                            let coverage = bitmap[row * metrics.width + col];
                            surface.blend_pixel(
                                glyph_x + col as i32,
                                glyph_y + row as i32,
                                color,
                                coverage,
                            );
                        }
                    }
                    pen += metrics.advance_width;
                    previous = Some(c);
                }
                pen - x
            }
            None => {
                let scale = fallback_scale(size);
                surface.draw_text_at(text, x.round() as i32, y.round() as i32, scale, color) as f32
            }
        }
    }

    /// The advance width of `text` at `size` (without drawing).
    pub fn measure(&mut self, text: &str, size: f32) -> f32 {
        match &mut self.sans {
            Some(sans) => {
                let mut width = 0.0;
                let mut previous: Option<char> = None;
                for c in text.chars() {
                    if let Some(p) = previous {
                        if let Some(kern) = sans.font.horizontal_kern(p, c, size) {
                            width += kern;
                        }
                    }
                    width += sans.glyph(c, size).0.advance_width;
                    previous = Some(c);
                }
                width
            }
            None => {
                let scale = fallback_scale(size);
                (text.chars().count() as i32 * GLYPH_ADVANCE as i32 * scale) as f32
            }
        }
    }

    /// The recommended line height for `size`.
    pub fn line_height(&self, size: f32) -> f32 {
        match &self.sans {
            Some(sans) => sans
                .font
                .horizontal_line_metrics(size)
                .map(|m| m.new_line_size)
                .unwrap_or(size * 1.25),
            None => (GLYPH_HEIGHT as i32 * fallback_scale(size) + 2) as f32,
        }
    }

    /// Truncate `text` with an ellipsis so it fits in `max_width` at `size`.
    /// Returns the text unchanged when it already fits.
    pub fn ellipsize(&mut self, text: &str, size: f32, max_width: f32) -> String {
        if self.measure(text, size) <= max_width {
            return text.to_string();
        }
        let ellipsis = if self.has_sans() { "…" } else { "..." };
        let mut kept: Vec<char> = text.chars().collect();
        while !kept.is_empty() {
            kept.pop();
            let candidate: String = kept.iter().collect::<String>() + ellipsis;
            if self.measure(&candidate, size) <= max_width {
                return candidate;
            }
        }
        ellipsis.to_string()
    }
}

/// Debug-font dots-per-pixel scale approximating a `size`-pixel line.
fn fallback_scale(size: f32) -> i32 {
    ((size / GLYPH_HEIGHT as f32).round() as i32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Color {
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    fn non_background(surface: &Surface) -> usize {
        surface
            .buffer()
            .iter()
            .filter(|&&p| p != 0x00ff_ffff)
            .count()
    }

    #[test]
    fn drawing_text_changes_pixels_with_or_without_system_font() {
        for mut fonts in [Fonts::load(), Fonts::fallback_only()] {
            let mut surface = Surface::new(200, 40);
            let width = fonts.draw(&mut surface, "Mocha", 4.0, 4.0, 16.0, red());
            assert!(width > 0.0);
            assert!(non_background(&surface) > 0);
        }
    }

    #[test]
    fn measure_returns_nonzero_and_grows_with_text() {
        for mut fonts in [Fonts::load(), Fonts::fallback_only()] {
            let short = fonts.measure("hi", 14.0);
            let long = fonts.measure("hello, mocha browser", 14.0);
            assert!(short > 0.0);
            assert!(long > short);
        }
    }

    #[test]
    fn measure_matches_draw_advance() {
        let mut fonts = Fonts::load();
        let mut surface = Surface::new(400, 40);
        let drawn = fonts.draw(&mut surface, "Mocha Browser", 2.0, 2.0, 14.0, red());
        let measured = fonts.measure("Mocha Browser", 14.0);
        assert!((drawn - measured).abs() < 0.5);
    }

    #[test]
    fn line_height_is_positive_and_scales() {
        for fonts in [Fonts::load(), Fonts::fallback_only()] {
            assert!(fonts.line_height(13.0) > 0.0);
            assert!(fonts.line_height(36.0) > fonts.line_height(13.0));
        }
    }

    #[test]
    fn ellipsize_truncates_long_text_to_fit() {
        for mut fonts in [Fonts::load(), Fonts::fallback_only()] {
            let long = "a very long tab title that cannot possibly fit";
            let budget = 80.0;
            let fitted = fonts.ellipsize(long, 13.0, budget);
            assert!(fitted.len() < long.len());
            assert!(fonts.measure(&fitted, 13.0) <= budget);
            // Short text passes through unchanged.
            assert_eq!(fonts.ellipsize("ok", 13.0, budget), "ok");
        }
    }
}
