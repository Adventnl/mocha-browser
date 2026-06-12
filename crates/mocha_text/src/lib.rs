//! Shared real-font text metrics and glyph rasterization for the page pipeline.
//!
//! Both `mocha_layout` (line breaking / box sizing) and `mocha_raster` (drawing)
//! need to agree on how wide text is and what each glyph looks like. This crate
//! is the single source of truth: it lazily loads a system sans font (via
//! `fontdue`, the same approved rasterizer the chrome uses) and exposes
//! measurement + glyph queries.
//!
//! **Determinism.** Until [`init_system_font`] (or [`init_from_bytes`]) succeeds,
//! the engine is *inactive* and every function returns the historical fixed
//! estimate (`chars * size * 0.6` advance, `size * 1.2` line height). This keeps
//! layout output byte-identical to the pre-font pipeline in tests and on systems
//! with no fonts; the desktop shell calls [`init_system_font`] once at startup to
//! switch the whole page to real, proportional, anti-aliased text.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// A rasterized glyph: coverage bitmap plus placement relative to the pen.
///
/// To draw, with the text baseline at `baseline` and the pen at `pen_x`:
/// `glyph_x = round(pen_x + left)`, `glyph_y = round(baseline - top)`, then blend
/// `bitmap[row*width + col]` as the alpha coverage of `(glyph_x+col, glyph_y+row)`.
#[derive(Debug)]
pub struct Glyph {
    pub advance: f32,
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
    pub bitmap: Vec<u8>,
}

/// Candidate system sans fonts, regular then a matching bold, tried in order.
const REGULAR_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\segoeui.ttf",
    r"C:\Windows\Fonts\arial.ttf",
    r"C:\Windows\Fonts\calibri.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/Library/Fonts/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
];
const BOLD_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\segoeuib.ttf",
    r"C:\Windows\Fonts\arialbd.ttf",
    r"C:\Windows\Fonts\calibrib.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
];

struct Faces {
    regular: fontdue::Font,
    bold: Option<fontdue::Font>,
}

static FACES: OnceLock<Option<Faces>> = OnceLock::new();

fn faces() -> Option<&'static Faces> {
    FACES.get().and_then(|o| o.as_ref())
}

/// Initialize the font faces once from `build`; returns whether a font is active
/// afterwards. Subsequent calls are no-ops (the first result wins).
fn init_faces(build: impl FnOnce() -> Option<Faces>) -> bool {
    FACES.get_or_init(build);
    is_active()
}

/// Glyph cache key: character, `size*10` rounded, and bold flag.
type GlyphKey = (char, u32, bool);
type GlyphCache = HashMap<GlyphKey, Option<Arc<Glyph>>>;

fn cache() -> &'static Mutex<GlyphCache> {
    static CACHE: OnceLock<Mutex<GlyphCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Load a system sans font (regular + bold). Returns whether a font is now
/// active. Idempotent: only the first successful call takes effect.
pub fn init_system_font() -> bool {
    init_faces(|| {
        let regular = load_first(REGULAR_CANDIDATES)?;
        let bold = load_first(BOLD_CANDIDATES);
        Some(Faces { regular, bold })
    })
}

/// Activate from in-memory font bytes (used by tests / headless tooling).
pub fn init_from_bytes(regular: &[u8], bold: Option<&[u8]>) -> bool {
    let regular = regular.to_vec();
    let bold = bold.map(|b| b.to_vec());
    init_faces(move || {
        let regular = fontdue::Font::from_bytes(regular.as_slice(), settings()).ok()?;
        let bold = bold
            .as_ref()
            .and_then(|b| fontdue::Font::from_bytes(b.as_slice(), settings()).ok());
        Some(Faces { regular, bold })
    })
}

/// Whether a real font is active (otherwise all queries use the estimate).
pub fn is_active() -> bool {
    faces().is_some()
}

/// The advance width of `text` at `size` px in the given weight.
pub fn measure(text: &str, size: f32, bold: bool) -> f32 {
    match face(bold) {
        Some(font) => text
            .chars()
            .map(|c| font.metrics(c, size).advance_width)
            .sum::<f32>()
            .round(),
        None => (text.chars().count() as f32 * size * 0.6).round(),
    }
}

/// The width of an inter-word space at `size`.
pub fn space_advance(size: f32, bold: bool) -> f32 {
    match face(bold) {
        Some(font) => font.metrics(' ', size).advance_width.round().max(1.0),
        None => (size * 0.6).round(),
    }
}

/// The recommended line height for `size`.
pub fn line_height(size: f32) -> f32 {
    match faces() {
        Some(f) => f
            .regular
            .horizontal_line_metrics(size)
            .map(|m| m.new_line_size.round())
            .unwrap_or((size * 1.2).round()),
        None => (size * 1.2).round(),
    }
}

/// The ascent (baseline offset from the top of the line) for `size`.
pub fn ascent(size: f32) -> f32 {
    match faces() {
        Some(f) => f
            .regular
            .horizontal_line_metrics(size)
            .map(|m| m.ascent)
            .unwrap_or(size * 0.8),
        None => size * 0.8,
    }
}

/// A rasterized, cached glyph for `c` at `size` (None when inactive or empty).
pub fn glyph(c: char, size: f32, bold: bool) -> Option<Arc<Glyph>> {
    let font = face(bold)?;
    let key = (c, (size * 10.0).round() as u32, bold);
    let mut cache = cache().lock().ok()?;
    cache
        .entry(key)
        .or_insert_with(|| {
            let (m, bitmap) = font.rasterize(c, size);
            if m.width == 0 || m.height == 0 {
                return Some(Arc::new(Glyph {
                    advance: m.advance_width,
                    left: 0,
                    top: 0,
                    width: 0,
                    height: 0,
                    bitmap: Vec::new(),
                }));
            }
            Some(Arc::new(Glyph {
                advance: m.advance_width,
                left: m.xmin,
                top: m.height as i32 + m.ymin,
                width: m.width,
                height: m.height,
                bitmap,
            }))
        })
        .clone()
}

fn face(bold: bool) -> Option<&'static fontdue::Font> {
    let f = faces()?;
    if bold {
        Some(f.bold.as_ref().unwrap_or(&f.regular))
    } else {
        Some(&f.regular)
    }
}

fn settings() -> fontdue::FontSettings {
    fontdue::FontSettings::default()
}

fn load_first(paths: &[&str]) -> Option<fontdue::Font> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(root) = std::env::var_os("SystemRoot") {
        for name in ["segoeui.ttf", "segoeuib.ttf", "arial.ttf", "arialbd.ttf"] {
            candidates.push(std::path::Path::new(&root).join("Fonts").join(name));
        }
    }
    candidates.extend(paths.iter().map(std::path::PathBuf::from));
    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(font) = fontdue::Font::from_bytes(bytes, settings()) {
                return Some(font);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // These run with no font loaded (the crate never calls `init_*` in tests),
    // so they pin the deterministic estimate that keeps layout output stable.
    #[test]
    fn inactive_by_default() {
        assert!(!is_active());
        assert!(glyph('a', 16.0, false).is_none());
    }

    #[test]
    fn estimate_matches_historical_formula() {
        for &size in &[12.0_f32, 16.0, 24.0, 32.0] {
            assert_eq!(measure("hello", size, false), (5.0 * size * 0.6).round());
            assert_eq!(measure("", size, false), 0.0);
            assert_eq!(space_advance(size, false), (size * 0.6).round());
            assert_eq!(line_height(size), (size * 1.2).round());
            assert_eq!(ascent(size), size * 0.8);
        }
    }

    #[test]
    fn measure_grows_with_length() {
        assert!(measure("aaaa", 16.0, false) > measure("aa", 16.0, false));
    }
}
