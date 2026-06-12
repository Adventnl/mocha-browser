//! Visual / raster regression checks (Milestone 20).
//!
//! Each case in `tests/visual/manifest.toml` is rendered through `mocha_engine`
//! and rasterized by `mocha_raster` into a fixed-size RGBA surface using the
//! built-in **debug font** (never OS fonts), then reduced to a checksum. The
//! checksum is compared against `tests/visual/expected/<name>.txt`.
//!
//! Checksums are deterministic because (a) layout is pure f32 arithmetic that is
//! identical across platforms (the same exact geometry is asserted by the
//! cross-platform integration tests), and (b) the rasterizer and debug font are
//! pure integer code with no OS dependency. Regenerate expected checksums with:
//!
//! ```bash
//! MOCHA_BLESS=1 cargo test -p mocha_compat --test visual_regression
//! ```

use std::path::PathBuf;

use mocha_engine::{render_url, RenderOptions};
use mocha_raster::{rasterize, Surface};

fn visual_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/visual"))
}

struct Case {
    name: String,
    path: String,
    width: u32,
    height: u32,
}

/// Parse the tiny `[[case]]` manifest (name/path strings, width/height numbers).
fn load_cases() -> Vec<Case> {
    let text = std::fs::read_to_string(visual_root().join("manifest.toml")).unwrap();
    let mut cases: Vec<Case> = Vec::new();
    let mut name = None;
    let mut path = None;
    let mut width = None;
    let mut height = None;
    let flush = |name: &mut Option<String>,
                 path: &mut Option<String>,
                 width: &mut Option<u32>,
                 height: &mut Option<u32>,
                 cases: &mut Vec<Case>| {
        if let (Some(n), Some(p), Some(w), Some(h)) =
            (name.take(), path.take(), width.take(), height.take())
        {
            cases.push(Case {
                name: n,
                path: p,
                width: w,
                height: h,
            });
        }
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[case]]" {
            flush(&mut name, &mut path, &mut width, &mut height, &mut cases);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        match key {
            "name" => name = Some(value.to_string()),
            "path" => path = Some(value.to_string()),
            "width" => width = Some(value.parse().unwrap()),
            "height" => height = Some(value.parse().unwrap()),
            _ => {}
        }
    }
    flush(&mut name, &mut path, &mut width, &mut height, &mut cases);
    assert!(!cases.is_empty(), "visual manifest has no cases");
    cases
}

/// Render and rasterize one case; return `(checksum, painted_pixels)`.
fn rasterize_case(case: &Case) -> (u64, usize) {
    let target = visual_root().join("cases").join(&case.path);
    let options = RenderOptions {
        viewport_width: case.width as f32,
        ..RenderOptions::default()
    };
    let page = render_url(&target.to_string_lossy(), &options)
        .unwrap_or_else(|e| panic!("visual case {} failed to render: {e}", case.name));
    let mut surface = Surface::new(case.width, case.height);
    rasterize(&mut surface, &page.display_list, &page.images, 0.0);
    let buffer = surface.buffer();
    let background = buffer.first().copied().unwrap_or(0);
    let painted = buffer.iter().filter(|&&px| px != background).count();
    (checksum(buffer), painted)
}

/// FNV-1a 64-bit over the little-endian RGBA buffer.
fn checksum(buffer: &[u32]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for pixel in buffer {
        for byte in pixel.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[test]
fn raster_checksums_are_stable() {
    let bless = std::env::var_os("MOCHA_BLESS").is_some();
    let expected_dir = visual_root().join("expected");
    if bless {
        std::fs::create_dir_all(&expected_dir).unwrap();
    }

    for case in load_cases() {
        let (sum, painted) = rasterize_case(&case);
        // Every case must actually paint something (catches an all-blank render).
        assert!(
            painted > 0,
            "visual case {} produced a blank surface",
            case.name
        );

        let expected_path = expected_dir.join(format!("{}.txt", case.name));
        if bless {
            std::fs::write(&expected_path, format!("{sum:016x}\n")).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
            panic!(
                "missing expected checksum for {} (run: MOCHA_BLESS=1 cargo test -p mocha_compat --test visual_regression)",
                case.name
            )
        });
        assert_eq!(
            format!("{sum:016x}"),
            expected.trim(),
            "raster checksum changed for visual case {} \
             (re-bless with MOCHA_BLESS=1 if this change is intended)",
            case.name
        );
    }
}

#[test]
fn scroll_offset_changes_the_checksum() {
    // A scrolled render of a tall page differs from the unscrolled one: proves the
    // checksum is sensitive to content position, not a constant.
    let case = Case {
        name: "article".to_string(),
        path: "article.html".to_string(),
        width: 800,
        height: 400,
    };
    let target = visual_root().join("cases").join(&case.path);
    let options = RenderOptions {
        viewport_width: case.width as f32,
        ..RenderOptions::default()
    };
    let page = render_url(&target.to_string_lossy(), &options).unwrap();

    let mut top = Surface::new(case.width, case.height);
    rasterize(&mut top, &page.display_list, &page.images, 0.0);
    let mut scrolled = Surface::new(case.width, case.height);
    rasterize(&mut scrolled, &page.display_list, &page.images, 200.0);

    assert_ne!(
        checksum(top.buffer()),
        checksum(scrolled.buffer()),
        "scrolling should change the rasterized output"
    );
}
