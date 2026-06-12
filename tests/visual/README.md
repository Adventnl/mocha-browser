# Visual Regression (raster checksums)

Milestone 20 added a lightweight **raster checksum** visual regression check.
Each case is rendered through `mocha_engine`, rasterized by `mocha_raster` into a
fixed-size RGBA surface using the **built-in debug font** (never OS fonts), and
reduced to a checksum that is compared against an approved value in `expected/`.

This is intentionally small: no PNG diffing, no perceptual comparison. The
checksum is deterministic because layout is pure f32 arithmetic (the same exact
geometry is asserted cross-platform by the integration tests) and the rasterizer
+ debug font are pure integer code with no OS dependency.

## Layout

- `manifest.toml` — the cases (`name`, `path`, `width`, `height`).
- `cases/` — the HTML inputs (and `pixel.png`, a tiny image asset).
- `expected/<name>.txt` — the approved FNV-1a checksum for each case.
- `visual_regression.rs` — the test target (compiled as part of `mocha_compat`).

## Running

```bash
# verify (part of the normal gate via `cargo test --all`)
cargo test -p mocha_compat --test visual_regression

# regenerate expected checksums after an intended rendering change, then review
MOCHA_BLESS=1 cargo test -p mocha_compat --test visual_regression
```

The test also asserts every case paints something (catches an all-blank render)
and that scrolling changes the checksum (catches a constant/no-op checksum). If a
checksum changes unexpectedly, a rendering regression is the likely cause —
investigate before re-blessing.

## Cases

- `article.html` — headings, paragraphs, nested block, long wrapping text.
- `inline-wrap.html` — inline spans sharing lines and wrapping in a narrow column.
- `box-model.html` — margin, padding, border, background, fixed sizes, nesting.
- `image.html` — an inline `<img>` (PNG) between paragraphs.
- `form-controls.html` — a label, text input, checkbox, radio, and button.
