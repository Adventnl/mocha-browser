# Limitations

Mocha Browser is at **Milestone 23** (multi-tab desktop shell with a SQLite
profile, minimal cookies + origin-aware web storage, a security policy
foundation, a multi-process prototype, a sandbox prototype, a headless
DevTools snapshot foundation, a compatibility test harness, crash corpus,
visual regression, and performance baseline; real proportional page font metrics
in M22; and forgiving, fail-open HTML parsing that lets real content pages render
in M23). It is an experimental engine with a minimal desktop frontend, not a
usable browser. This document is deliberately explicit about what does not exist
so the project never overclaims.

The supported subset is enumerated precisely in
[compatibility-level-1.md](compatibility-level-1.md) and held in place by the
`mocha_compat` harness (see [compatibility-testing.md](compatibility-testing.md)).
That harness tests Mocha's own small subset; it is **not** web-platform-tests and
says nothing about modern-web compatibility.

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
- **HTTPS / TLS (Milestone 21)** — `https://` loads over rustls with
  certificates verified against the embedded Mozilla root store (TLS is never
  hand-rolled). Limits: no certificate-error interstitial or override, no
  revocation checking (CRL/OCSP), no HSTS, no client certificates, no OS trust
  store, and TLS state is not surfaced in the UI (no padlock).
- **Real HTML5 parsing algorithm** — the tokenizer and tree builder are a small
  hand-written grammar, not the spec's state machine: no insertion modes and no
  foster parenting. Since Milestone 23 they are **forgiving** — any tag name is
  accepted, malformed/mismatched/unclosed markup recovers, a few implied end tags
  are applied, and HTML character references are decoded — but this is still a
  pragmatic subset, not spec-compliant parsing.
- **Incremental invalidation** — scripts mutate the DOM, but style/layout/paint
  re-run **once** over the final DOM (coarse invalidation); there is no
  incremental relayout.
- **Forms beyond the Milestone 10 basics** — basic controls parse, carry
  state, lay out, and model GET submission (see
  [forms-and-controls.md](forms-and-controls.md)). Terminal mode prints
  `DrawControl`; desktop mode debug-rasterizes controls through
  `mocha_raster`/`mocha_desktop`, but they are not native widgets. Desktop mode
  has crude click routing and simple text entry/backspace where implemented, but
  there is still no mature focus, caret, selection, IME, accessibility, validation
  UI, POST bodies / `multipart/form-data`, file/date/color/range/number inputs,
  `:checked`/`:disabled`/`:focus` pseudo-classes, `<optgroup>` / `<fieldset>` /
  multiple-select / `form` attribute, label activation, or autofill.
- **Fonts** — no font loading or shaping; text size is estimated, not measured.
- **Canvas / accessibility** — not parsed or rendered.
- **Security sandbox** — no process sandbox, no permission UI, and no web APIs
  consuming permission state yet. A **minimal origin model** exists (Milestone 15,
  `mocha_origin`) for scoping cookies/web storage,
  and M16 adds explicit policy objects in `mocha_security` (same-origin checks,
  scheme/file decisions, mixed-content awareness, CSP, permissions, and
  capabilities). M18 adds a capability-restricted renderer path, but this is
  **not** a complete security boundary: no OS sandbox, site isolation, TLS, CORS,
  Fetch, or broad runtime enforcement.
- **Multi-process architecture** — M17 has a renderer child-process prototype
  with typed IPC, crash detection, and respawn tests. M18 adds an optional
  capability-restricted prepared-document path. The normal desktop/shell path is
  still not multiprocess by default; there is no site isolation, network/GPU
  process split, or OS sandboxing.
- **DevTools** — M19 adds `mocha_devtools` snapshots and a
  `mocha_shell --devtools-snapshot` command for deterministic headless
  inspection. It is not Chrome DevTools or CDP: no remote debugging socket,
  breakpoints, JavaScript stepping, live DOM/style editing, heap snapshots,
  profiler, network waterfall, or interactive UI panels. Network logs currently
  include top-level document metadata; subresource logging hooks are future work.
- **Profile / sessions** — the desktop shell has tabs (M13) and a SQLite profile
  (M14): history, bookmarks, settings, download metadata, and persistent session
  snapshots, plus a private in-memory profile. M15 adds cookie persistence and a
  persistent origin-keyed `localStorage` table. But it is **not** a full or
  secure profile: no encryption, no sync, no passwords, no favicons, and the
  interactive shell does not yet surface history/bookmarks UI or auto-restore
  sessions. The default page-loading path is not yet cookie-backed, JS
  `localStorage` is not yet wired to the persistent store, and `sessionStorage`
  is still per-runtime unless an embedder wires tab-owned storage. No tab
  drag/reorder, pinned tabs, tab groups, or crash recovery. See
  [tabs-and-session.md](tabs-and-session.md),
  [profile-storage.md](profile-storage.md), and
  [cookies-and-web-storage.md](cookies-and-web-storage.md).
- **Modern web compatibility** — since Milestone 23, Mocha can render real
  *content* pages (their text, headings, lists, links, and images) because the
  pipeline now **fails open**: a stylesheet, script, or image it cannot process is
  skipped (and reported as a diagnostic) instead of aborting the whole page. It
  still cannot run app-style sites that build their UI with modern JavaScript, and
  most modern CSS is not yet *applied* (the offending stylesheet is currently
  skipped wholesale — finer-grained CSS recovery is a later milestone).

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

Not supported — since Milestone 23, encountering any of these makes the cascade
**skip that whole stylesheet** (a diagnostic is recorded) and render with what
remains, rather than aborting the page; finer-grained per-declaration recovery is
a later milestone:

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

- **JavaScript event handlers are minimal** — JS can register listeners through
  `addEventListener`, and Mocha dispatches its own simplified event objects, but
  there is no browser event loop, no real pointer/wheel/touch/focus model, and
  only the small supported event surface is wired.
- **Window input (M12)** — `mocha_desktop` routes clicks and keyboard to the
  page/address bar via the layout's hit-test and browser state (see
  [desktop-shell.md](desktop-shell.md)). **But:** no text editing/caret, no
  focus/selection, no IME, no pointer/wheel/touch/drag events, no accessibility.
- **No `passive` listeners** and no accessibility event model.
- **Hit testing ignores z-index, transforms, scrolling, clipping, and
  `pointer-events`**; it returns the deepest box containing the point.
- `<a>` supports only `href` (inline, blue); `target`/`download`/`rel`/`ping`
  are parsed but have no behavior.

## Networking limitations (Milestones 4 and 21)

See [networking-and-navigation.md](networking-and-navigation.md) for detail.

- **`http://` and `https://`** — a hand-written blocking HTTP/1.1 `GET` over
  TCP (TLS via rustls for https), with chunked-transfer decoding and gzip
  content decoding (the from-scratch `mocha_gzip` crate). No HTTP/2 or HTTP/3,
  no keep-alive, and no `br`/`zstd`/`deflate` encodings (clear errors).
- **Cookies are minimal** (Milestone 15): a jar with `Set-Cookie`/`Cookie`
  HTTP integration (`CookieProvider`) and profile persistence, but **not** full
  RFC 6265bis — no public-suffix list, no third-party/partitioned policy, no real
  `SameSite` enforcement, and `Secure` cookies need HTTPS. **No authentication,
  credentials, or proxy support.** See
  [cookies-and-web-storage.md](cookies-and-web-storage.md).
- **The in-memory cache is not an HTTP cache** — no `Cache-Control`, validation,
  or expiration; only `200` responses are stored, for the process lifetime.
- **Only HTML documents render**; other content types return a clear error.
- **UTF-8 only** — no charset detection or decoding; invalid UTF-8 is rejected.
- A **minimal origin model** exists (`mocha_origin`) for storage/cookie scoping,
  and M16 adds policy objects for same-origin checks, mixed-content handling, and
  CSP. Broad enforcement across all network/render paths is still incomplete.
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

- **Input is crude** — desktop mode routes clicks to controls and supports simple
  text entry/backspace for text-editable controls where implemented, but there is
  no mature focus model, caret, selection, IME, accessibility, or rich editing.
  Terminal/headless mode only prints display-list/control state.
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
- **`DrawControl` is not a native widget** — terminal mode prints it as a
  display-list command; desktop mode rasterizes/debug-draws it through
  `mocha_raster`/`mocha_desktop`.
- **`form.submit()` never navigates** — it records a request the embedder may
  inspect; the shell only notes it on stderr.

## HTML tags

Since Milestone 23 **any** element name is accepted; unknown tags become ordinary
elements (block-level by default). The user-agent stylesheet gives sensible
defaults to the common content tags — headings (`h1`–`h6`, bold), inline text
semantics (`em`/`strong`/`code`/`sub`/`sup`/…), lists (`ul`/`ol`/`li`, with
bullet/number markers), sectioning elements, `blockquote`, `pre` — and treats
head metadata (`head`/`meta`/`title`/`link`/`style`/`script`/`noscript`) as
non-rendered. `style`, `script`, and `textarea` are raw-text elements; the full
HTML void set (`area base br col embed hr img input link meta param source track
wbr`) is recognized. Plus doctype declarations and comments. (`<link>`
participates only as `rel="stylesheet"`; `<script src>` is parsed but not
executed — it is skipped with a diagnostic.)

## Honesty rule

Mocha never returns a fake or placeholder result that pretends a feature works.
Since Milestone 23 the *granularity* of failure is per-feature, not per-page:
when a stylesheet, script, or image cannot be processed it is **skipped** and the
reason is recorded as a render diagnostic (surfaced in the desktop "N features not
supported" badge, the terminal shell's stderr, and the DevTools snapshot) while
the rest of the page renders. This matches how the HTML/CSS specs define
forward-compatible parsing (unknown constructs are dropped, not fatal). Genuinely
fatal conditions still fail with a clear `MochaError` (commonly `Network` for a
transport failure, `UnsupportedFeature` for a non-HTML content type, `Io`, or
`InvalidUrl`).
