# Visual Regression (not implemented yet)

**Visual regression image comparison is not implemented yet.** Mocha has no
surface renderer — it emits a textual display list, not pixels.

The HTML files in `cases/` are **future render targets**: once Mocha grows a real
surface renderer, these will be rendered to images and compared against approved
baselines. There are intentionally **no PNGs** here, and nothing in this
directory is wired into `cargo test`.

Until then, layout is verified by:

- geometry / structure tests in `crates/mocha_layout`,
- display-list tests in `crates/mocha_paint`,
- end-to-end tests in `tests/integration/rendering_pipeline.rs`, and
- the layout dump: `cargo run -p mocha_shell -- --dump-layout <file>`.

## Cases

- `cases/article.html` — headings, paragraphs, nested block, long wrapping text.
- `cases/inline-wrap.html` — inline spans sharing lines and wrapping in a narrow column.
- `cases/box-model.html` — margin, padding, border, background, fixed sizes, nesting.
