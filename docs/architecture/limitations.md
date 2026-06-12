# Limitations

Mocha Browser is at **Milestone 13** (multi-tab desktop shell with an in-memory
session model). It is an experimental engine with a minimal desktop frontend, not
a usable browser. This document is deliberately explicit about what does not exist
so the project never overclaims.

## Not supported

- **Browser-grade JavaScript** — `mocha_js` is a small custom subset (not
  ECMAScript). Inline `<script>` now runs against the DOM through the
  `mocha_js_dom` bindings, but the DOM surface is tiny and there is no security
  model (see the JavaScript section below and
  [dom-bindings.md](dom-bindings.md)).
- **External scripts and CSS `url(...)`** — `<script src>`, CSS `url(...)`
  resources, web fonts, and a `<base>` element are unsupported. External
  `<link rel="stylesheet">` CSS and `<img>` images *are* loaded (Milestones 8–9).
- **Image rendering** — `<img>` is decoded (PNG/JPEG) and laid out, and a
  `DrawImage` command is emitted. In desktop mode (M12), images are rasterized via
  `mocha_raster`; in terminal mode, the display list is printed as text. No
  nearest-neighbor, antialiasing, or scaling quality control. `srcset`/`<picture>`,
  SVG, and animation are unsupported.
- **HTTPS / TLS** — `https://` returns `UnsupportedFeature` (TLS is never
  hand-rolled and no TLS library is bundled). This includes HTTPS subresources.
- **Real HTML5 parsing algorithm** — the tokenizer and tree builder accept a
  tiny hand-written grammar and a fixed tag set; there is no spec-compliant
  tokenization, no insertion modes, and no error recovery.
- **Incremental invalidation** — scripts mutate the DOM, but style/layout/paint
  re-run **once** over the final DOM (coarse invalidation); there is no
  incremental relayout.
- **Forms beyond the Milestone 10 basics** — basic controls parse, carry
  state, lay out, and model GET submission (see
  [forms-and-controls.md](forms-and-controls.md)). In M12, controls render as
  `DrawControl` commands (display-list only, no native widgets). **There is no
  keyboard text editing, focus, caret, selection, or IME**, no validation, no
  POST bodies / `multipart/form-data`, no file/date/color/range/number inputs,
  no `:checked`/`:disabled`/`:focus` pseudo-classes, no `<optgroup>` / `<fieldset>`
  / multiple-select / `form` attribute, no label activation, and no autofill.
- **Fonts** — no font loading or shaping; text size is estimated, not measured.
- **Canvas / accessibility** — not parsed or rendered.
- **Security sandbox** — no process sandbox, origin model, or permissions.
- **Multi-process architecture** — single process only.
- **Persistent profile / sessions** — the desktop shell has tabs (M13) and an
  **in-memory** session snapshot/restore, but **nothing is written to disk**: no
  profile directory, no persistent session restore, no bookmarks/history/downloads
  database, no settings store, no cookies (those begin at Milestones 14–15). There
  is no tab drag/reorder, pinned tabs, tab groups, private browsing, or crash
  recovery. See [tabs-and-session.md](tabs-and-session.md).
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
- **`<style>`/`<script>`/`<textarea>` raw-text handling is minimal**: the body
  is captured verbatim (whitespace preserved) until the literal close tag. This
  is **not** the HTML5 raw-text/RCDATA algorithm — there is no handling of
  escaped or nested edge cases beyond finding the closing tag, and no character
  references are decoded. A missing close tag is an error.
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

## Event and input limitations (Milestone 5, M12 extended)

See [events.md](events.md) for detail.

- **No JavaScript event handlers** — the DOM event system exists for internal
  dispatch (link clicks, form actions, etc.) and test harnesses, but there is no
  way to attach JavaScript listeners at runtime (the JS bindings offer a stub
  `addEventListener` that does not fire handlers).
- **Window input (M12)** — `mocha_desktop` routes clicks and keyboard to the
  page/address bar via the layout's hit-test and browser state (see
  [desktop-shell.md](desktop-shell.md)). **But:** no text editing/caret, no
  focus/selection, no IME, no pointer/wheel/touch/drag events, no accessibility.
- **No `passive` listeners** and no accessibility event model.
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

## Form limitations (Milestone 10)

See [forms-and-controls.md](forms-and-controls.md) for detail.

- **No real user input** — no typing, focus, caret, text selection, or IME;
  control state changes come from JavaScript and programmatic event dispatch.
- **Unsupported `input`/`button` types fail clearly** (`date`, `file`,
  `color`, `range`, `number`, …) — never a silent fallback to text.
- **GET only** — `method="post"` (and any other method) is
  `UnsupportedFeature`; there are no request bodies and no
  `multipart/form-data`.
- **No validation** — `required`/`pattern`/`min`/`max` have no behaviour and
  no validation UI exists.
- **No `:checked`/`:disabled`/`:focus` pseudo-classes** — control state does
  not affect selector matching.
- **Single-select only** — no `multiple`, no `<optgroup>`; a select with no
  `selected` option defaults to its **first** option.
- **Label clicks do not activate controls**; `<fieldset>`/`<legend>` and the
  `form` attribute are unsupported.
- **`DrawControl` is not a widget** — controls are display-list commands;
  nothing is rasterized and no native controls exist.
- **`form.submit()` never navigates** — it records a request the embedder may
  inspect; the shell only notes it on stderr.

## Supported HTML tags

Only these element names are accepted. Any other tag is an `UnsupportedFeature`
error (it is **not** silently skipped):

```text
html  body  h1  h2  p  div  span  a  style  script  link  img
form  input  button  label  textarea  select  option
```

`style`, `script`, and `textarea` are raw-text elements; `link`, `img`, and
`input` are void elements. Plus doctype declarations and comments. (`<link>`
participates only as `rel="stylesheet"`; `<script src>` is parsed but not
executed.)

## Honesty rule

Unsupported behaviour fails with a clear `MochaError` (commonly
`UnsupportedFeature`, `NotImplemented`, `Parse`, `Layout`, or `InvalidUrl`).
Mocha never returns a fake or placeholder result that pretends a feature works.
