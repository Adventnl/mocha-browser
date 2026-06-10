# Limitations

Mocha Browser is at **Milestone 2**. It is an engine laboratory, not a usable
browser. This document is deliberately explicit about what does not exist so the
project never overclaims.

## Not supported

- **JavaScript** — no engine, no execution, no bindings.
- **External / linked CSS** — `<link rel="stylesheet">` is rejected with a clear
  `UnsupportedFeature` error; external CSS is never fetched or parsed.
- **Networking** — no `file` fetching beyond a direct local read; no HTTP,
  HTTPS, sockets, cookies, or caching. Non-`file` URLs return
  `UnsupportedFeature`.
- **Real HTML5 parsing algorithm** — the tokenizer and tree builder accept a
  tiny hand-written grammar and a fixed tag set; there is no spec-compliant
  tokenization, no insertion modes, and no error recovery.
- **Dynamic DOM mutation** — the tree is built once and never changes.
- **Forms** — no form elements, controls, or submission.
- **Images** — no `<img>`, decoding, or raster pipeline.
- **Fonts** — no font loading or shaping; text size is estimated, not measured.
- **Canvas / SVG / accessibility** — not parsed or rendered.
- **Security sandbox** — no process sandbox, origin model, or permissions.
- **Multi-process architecture** — single process only.
- **Modern web compatibility** — Mocha cannot browse real websites.

## CSS support (Milestone 2)

Supported:

- **Sources:** `<style>` blocks and inline `style` attributes.
- **Selectors:** type, class, id, universal, and descendant combinator; simple
  selector lists (`h1, h2`); compound selectors (`div.note#x`).
- **Cascade:** UA defaults → author rules → inline; higher specificity wins,
  ties broken by source order (later wins); inline beats stylesheet rules.
- **Inheritance:** `color`, `font-size`, `font-weight`.
- **Properties:** `display`, `color`, `background-color`, `font-size`,
  `font-weight`, `width`, `height`, `margin`/`margin-*`, `padding`/`padding-*`,
  `border-width`, `border-color`.
- **Values:** `px` lengths (and unitless `0`); named colors (`black`, `white`,
  `red`, `green`, `blue`, `transparent`); `#rgb` and `#rrggbb` hex;
  keywords for `display` (`block`/`inline`/`none`) and `font-weight`
  (`normal`/`bold`).

Not supported (returns a clear error):

- `!important`, origin layers, user styles, animations, transitions.
- Media queries, pseudo-classes, pseudo-elements, attribute selectors.
- The `>`, `+`, and `~` combinators.
- `em`, `rem`, `%`, `vh`, `vw`; `calc()`, `var()`, custom properties.
- `rgb()`/`rgba()`/`hsl()`, `currentColor`, system colors.
- `position`, `float`, `line-height`, `overflow`, `text-align`, shorthands other
  than `margin`/`padding`, flexbox, and grid.

## Intentionally ignored, by design (documented)

- **Whitespace-only text nodes** are dropped during tokenization, and the
  whitespace inside retained *HTML* text is collapsed to single spaces. This is a
  simplification, not standards behaviour.
- **`<style>` raw-text handling is minimal**: the body is captured verbatim
  (whitespace preserved) until the literal `</style>`. This is **not** the
  HTML5 raw-text/RCDATA algorithm — there is no handling of escaped or nested
  edge cases beyond finding the closing tag. A missing `</style>` is an error.
- **Comment and doctype nodes** are stored in the DOM but produce no styled or
  layout box, and therefore no display command.
- **`<style>` element contents** never produce `DrawText`: `style` has a UA
  default of `display: none`, so layout skips its subtree.
- **`height` of the viewport** does not constrain layout; vertical content may
  exceed it. There is no scrolling or overflow handling.
- **Inline boxes** (e.g. `<span>`) are still laid out on their own line; there is
  no real inline formatting yet.

## Supported HTML tags

Only these element names are accepted. Any other tag is an `UnsupportedFeature`
error (it is **not** silently skipped); `<link>` and `<script>` are rejected
explicitly:

```text
html  body  h1  h2  p  div  span  style
```

Plus doctype declarations and comments.

## Honesty rule

Unsupported behaviour fails with a clear `MochaError` (commonly
`UnsupportedFeature`, `NotImplemented`, `Parse`, `Layout`, or `InvalidUrl`).
Mocha never returns a fake or placeholder result that pretends a feature works.
