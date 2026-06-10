# Crate Boundaries

Mocha is a Cargo workspace of small crates with strict, one-way dependencies.
Each crate owns one responsibility and depends only on crates "below" it. This
keeps the engine legible and prevents, for example, layout logic from leaking
into parsing.

```text
mocha_error
- shared error types only
- depends on: nothing

mocha_url
- URL parsing and normalization
- no networking
- depends on: mocha_error

mocha_dom
- DOM tree representation
- no HTML parsing
- depends on: mocha_error

mocha_html
- tokenization and tree building
- no layout
- depends on: mocha_error, mocha_dom

mocha_css
- CSS tokenizer, parser, stylesheet/value model, specificity
- no DOM access, no selector matching against a DOM
- depends on: mocha_error

mocha_style
- selector matching, cascade, inheritance, computed style
- owns the default (user-agent) styles
- no layout geometry
- depends on: mocha_error, mocha_dom, mocha_css

mocha_layout
- box-model layout consuming computed style
- no CSS parsing
- depends on: mocha_error, mocha_dom, mocha_style

mocha_paint
- display list generation (colors, borders)
- no window rendering, no CSS parsing
- depends on: mocha_error, mocha_layout

mocha_shell
- command-line executable (library + binary)
- wires the pipeline together
- no browser UI yet
- depends on: mocha_error, mocha_url, mocha_html, mocha_style,
  mocha_layout, mocha_paint
```

## Notes

- `mocha_error` is the only crate every other crate may depend on. Each crate
  constructs the `MochaError` variant matching its own responsibility, which
  keeps messages specific. There are deliberately no conversions between
  `MochaError` variants.
- `mocha_dom` knows nothing about HTML syntax; `mocha_html` builds DOM trees but
  never lays them out; `mocha_css` parses CSS but never touches a DOM;
  `mocha_style` matches and cascades but produces no geometry; `mocha_layout`
  produces geometry but never paints; `mocha_paint` produces a display list but
  never opens a window.
- Default (user-agent) styles live in `mocha_style`, **not** `mocha_layout`.
  Layout no longer hard-codes per-tag font sizes or margins.
- `Color` is defined in `mocha_css` and re-exported through `mocha_style` and
  `mocha_layout` so that `mocha_layout` and `mocha_paint` can name it without
  depending on `mocha_css` directly.
- `mocha_shell` is the only crate that performs I/O against the real filesystem
  and the terminal. It exposes `run_file` and `run_html` so the whole pipeline is
  testable without a process boundary.
- Future crates (`mocha_net`, `mocha_js`, `mocha_gpu`, `mocha_security`,
  `mocha_browser`, …) are intentionally **not** created yet. They are described
  in [milestones.md](milestones.md) as direction only.
