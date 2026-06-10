# Limitations

Mocha Browser is at **Milestone 4**. It is an engine laboratory, not a usable
browser. This document is deliberately explicit about what does not exist so the
project never overclaims.

## Not supported

- **JavaScript** — no engine, no execution, no bindings.
- **External / linked CSS and other subresources** — `<link rel="stylesheet">`
  is rejected with a clear `UnsupportedFeature` error; images, scripts, and fonts
  are not loaded. Milestone 4 loads the top-level document only.
- **HTTPS / TLS** — `https://` returns `UnsupportedFeature` (TLS is never
  hand-rolled and no TLS library is bundled).
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

## Networking limitations (Milestone 4)

See [networking-and-navigation.md](networking-and-navigation.md) for detail.

- **`http://` only** — a hand-written blocking HTTP/1.1 `GET` over TCP; no
  HTTPS/TLS, HTTP/2, or HTTP/3; no keep-alive, chunked-transfer decoding, or
  compression.
- **No cookies, authentication, credentials, or proxy support.**
- **The in-memory cache is not an HTTP cache** — no `Cache-Control`, validation,
  or expiration; only `200` responses are stored, for the process lifetime.
- **Only HTML documents render**; other content types return a clear error.
- **UTF-8 only** — no charset detection or decoding; invalid UTF-8 is rejected.
- **No origin model**, same-origin checks, mixed-content handling, or CSP.
- Redirects are followed (limit 10); redirects to `file://` and unsupported
  schemes are rejected. Dot-segments in relative redirects are not normalized.

## Layout limitations (Milestone 3)

Block and inline formatting are real but small:

- **Text measurement is estimated**, not from real font metrics: word width is
  `char_count * font_size * 0.6` and line height is `max_font_size * 1.2`.
- **No real fonts** — no font loading, shaping, kerning, or per-glyph metrics.
- **No hyphenation or character wrapping**; a single word wider than the line is
  placed alone and **overflows** the content box.
- **No margin collapse** — adjacent vertical margins add rather than collapse.
- **No `text-align`, `white-space` modes, `line-height` property, vertical-align,
  or baseline alignment** — text runs are top-aligned within a line box.
- **Inline-level elements are flattened into text runs**: no inline boxes are
  produced, and **inline backgrounds/borders are deferred** (inline text color
  and font size are honored).
- **No floats, positioning, flexbox, grid, tables, or overflow/scrolling.**

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
