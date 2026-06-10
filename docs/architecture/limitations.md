# Limitations

Mocha Browser is at **Milestone 9**. It is an engine laboratory, not a usable
browser. This document is deliberately explicit about what does not exist so the
project never overclaims.

## Not supported

- **Browser-grade JavaScript** — `mocha_js` is a small custom subset (not
  ECMAScript). Inline `<script>` now runs against the DOM through the
  `mocha_js_dom` bindings, but the DOM surface is tiny and there is no security
  model (see the JavaScript section below and
  [dom-bindings.md](dom-bindings.md)).
- **External scripts and CSS `url(...)`** — `<script src>`, CSS `url(...)`
  resources, web fonts, and a `<base>` element are unsupported. External
  `<link rel="stylesheet">` CSS and `<img>` images *are* loaded (Milestones 8–9).
- **Image rasterization** — `<img>` is decoded (PNG/JPEG) and laid out, and a
  `DrawImage` command is emitted, but **no pixels are drawn** (there is no
  graphics surface). `srcset`/`<picture>`, SVG, and animation are unsupported.
- **HTTPS / TLS** — `https://` returns `UnsupportedFeature` (TLS is never
  hand-rolled and no TLS library is bundled). This includes HTTPS subresources.
- **Real HTML5 parsing algorithm** — the tokenizer and tree builder accept a
  tiny hand-written grammar and a fixed tag set; there is no spec-compliant
  tokenization, no insertion modes, and no error recovery.
- **Incremental invalidation** — scripts mutate the DOM, but style/layout/paint
  re-run **once** over the final DOM (coarse invalidation); there is no
  incremental relayout.
- **Forms** — no form elements, controls, or submission.
- **Fonts** — no font loading or shaping; text size is estimated, not measured.
- **Canvas / accessibility** — not parsed or rendered.
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

## JavaScript limitations (Milestones 6–7)

See [javascript-interpreter.md](javascript-interpreter.md) and
[dom-bindings.md](dom-bindings.md) for detail.

- **Not ECMAScript-compliant** and a small subset only.
- **DOM bindings are a tiny hand-picked surface** (`window`/`document`, query,
  mutate, `addEventListener`, `setTimeout`); inline `<script>` runs but external
  `<script src>` does not. No live `NodeList`, MutationObserver, real event
  loop/microtasks, or security model. Timers are a deterministic queue (no real
  clock). Invalidation is coarse.
- No promises, `async`/`await`, modules, classes, `new`, prototype chains, full
  `this` semantics, regex, `Date`, `JSON`, template literals, arrow functions,
  destructuring, spread, generators, ternary `?:`, `switch`, `break`/`continue`,
  or exceptions/`try`-`catch`.
- `==`/`!=` behave like `===`/`!==` (so `null == undefined` is **false** here);
  coercion is a small documented subset.
- No garbage collector beyond Rust ownership + `Rc`; an execution **step limit**
  (100,000) aborts runaway loops.

## Event limitations (Milestone 5)

See [events.md](events.md) for detail.

- **No JavaScript** — event listeners are Rust callbacks; there is no scripting.
- **No real window/OS input or event loop** — events are dispatched
  programmatically (e.g. in tests or via `--hit-test`).
- **No pointer, touch, wheel, focus, input, composition, or drag/drop events**;
  only `click`/`mousedown`/`mouseup`/`mousemove`/`keydown`/`keyup` data exist.
- **No `passive` listeners** and no accessibility event model.
- **Link navigation is a default-action result only**, not an interactive UI.
- **Hit testing ignores z-index, transforms, scrolling, clipping, and
  `pointer-events`**; it returns the deepest box containing the point.
- `<a>` supports only `href` (inline, blue); `target`/`download`/`rel`/`ping`
  are parsed but have no behavior.

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
  schemes are rejected. Dot-segments (`.`/`..`) in relative references *are*
  normalized for URL/POSIX paths (used for subresource resolution); Windows file
  paths are resolved by the OS at access time.

## Layout limitations (Milestone 3)

Block and inline formatting are real but small:

- **Text measurement is estimated**, not from real font metrics: word width is
  `char_count * font_size * 0.6` and line height is `max_font_size * 1.2`.
- **No real fonts** — no font loading, shaping, kerning, or per-glyph metrics.
- **No hyphenation or character wrapping**; a single word wider than the line is
  placed alone and **overflows** the content box.
- **No margin collapse** — adjacent vertical margins add rather than collapse.
- **No `text-align`, `white-space` modes, `line-height` property, vertical-align,
  or baseline alignment** — text runs and inline images are top-aligned within a
  line box (a line's height is its tallest item).
- **Inline-level elements are flattened into text runs**: no inline boxes are
  produced, and **inline backgrounds/borders are deferred** (inline text color
  and font size are honored).
- **Replaced elements (`<img>`)** lay out with resolved CSS/attribute/intrinsic
  dimensions (inline by default, or block) but have no `object-fit`, no
  backgrounds/borders, and no baseline alignment. See
  [images-and-replaced-elements.md](images-and-replaced-elements.md).
- **No floats, positioning, flexbox, grid, tables, or overflow/scrolling.**

## Supported HTML tags

Only these element names are accepted. Any other tag is an `UnsupportedFeature`
error (it is **not** silently skipped):

```text
html  body  h1  h2  p  div  span  a  style  script  link  img
```

`style` and `script` are raw-text elements; `link` and `img` are void elements.
Plus doctype declarations and comments. (`<link>` participates only as
`rel="stylesheet"`; `<script src>` is parsed but not executed.)

## Honesty rule

Unsupported behaviour fails with a clear `MochaError` (commonly
`UnsupportedFeature`, `NotImplemented`, `Parse`, `Layout`, or `InvalidUrl`).
Mocha never returns a fake or placeholder result that pretends a feature works.
