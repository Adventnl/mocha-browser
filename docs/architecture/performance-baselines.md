# Performance Baselines

Milestone 20 added `mocha_perf`, a tiny tool that times the core CPU phases of
rendering one local document. It is a **baseline**, not a benchmark suite:
timings vary run to run and are **never** asserted in CI. The only CI check is
that the command runs.

## Running

```bash
cargo run -p mocha_perf -- examples/layout/article.html
```

Example output:

```text
Mocha Perf Report
parse_html_ms=0.562
js_ms=0.000
style_ms=0.305
layout_ms=0.212
paint_ms=0.020
raster_ms=12.842
total_ms=0.980
nodes=33
layout_boxes=153
display_commands=130
mem_estimate_bytes=36352
```

## What is measured

- `parse_html_ms` — `mocha_html::parse_html`.
- `js_ms` — running inline `<script>`s through `mocha_js_dom` (0 when there are
  none); mirrors the engine, which runs scripts before style/layout.
- `style_ms` — `mocha_style::build_style_tree` (inline stylesheets only).
- `layout_ms` — `mocha_layout::build_layout_tree`.
- `paint_ms` — `mocha_paint::build_display_list`.
- `raster_ms` — `mocha_raster::rasterize` onto a surface (debug font, no images).
- `total_ms` — an end-to-end render through the real `mocha_engine` in-memory
  pipeline.
- `nodes` / `layout_boxes` / `display_commands` — exact counts.
- `mem_estimate_bytes` — a deliberately rough estimate from those counts, not a
  real allocation measurement.

## Scope and caveats

- **Local files only.** No network is involved, so timings reflect CPU work
  rather than connection variance.
- The phase breakdown does **not** load subresources (external CSS, images); only
  inline `<style>`/`<script>` participate. The headline `total_ms` does use the
  full in-memory pipeline.
- Debug builds (the default) are much slower than release; `raster_ms` in
  particular dominates in debug. Use `--release` for representative numbers.
- These numbers are a baseline to notice large regressions, not a target. M20
  does not optimize the engine.

The library has unit tests asserting that every report field is populated and
formatted; the binary is exercised by the release-readiness / final-verification
commands.
