# Rendering Pipeline

As of Milestone 13 the pipeline is unchanged per document; the desktop shell now
runs one pipeline per **tab** (the active tab drives the viewport). The flow is:

**Terminal path (mocha_shell):**
```text
input URL/path → mocha_url → mocha_net/mocha_nav (load: file/http, redirects,
  content-type, cache) → HTML/CSS/JS/subresources/images/forms
  → DOM/style/layout → display list → terminal text output
```

**Desktop path (mocha_desktop):**
```text
input URL/path → [same core pipeline] → display list + image resources
  → mocha_raster (rasterize to pixel buffer)
  → mocha_desktop::window (draw buffer + browser chrome)
  → desktop window
```

The core document loading and rendering (Milestones 1–10) is orchestrated by
`mocha_engine` and produces an identical display list regardless of output path:

Inline scripts run **once** before style/layout (coarse invalidation), then
subresources are collected from the final DOM and style/layout/paint run once.
Form-control state initializes from attributes before scripts run and is shared
with the JS bindings, so script changes to `value`/`checked`/`disabled` reach
the final display list.

Each stage is described below with its input, output, owning crate, current
limitations, and intended future expansion.

## 0. Input → bytes (load + navigation)

- **Input:** a URL or local path string.
- **Output:** a `ResourceResponse` (final URL, status, headers, content type,
  body), then a UTF-8 `&str` body for HTML.
- **Owning crates:** `mocha_url` (parse), `mocha_net` (`DefaultLoader`:
  file/http, redirects, content-type, in-memory cache), `mocha_nav`
  (history), orchestrated by `mocha_shell`.
- **Current limitations:** `file://`/`http://` only — **no HTTPS/TLS**; only HTML
  renders; UTF-8 only; no cookies/auth/proxy; the cache is not an HTTP cache. See
  [networking-and-navigation.md](networking-and-navigation.md).
- **Future expansion:** HTTPS via a vetted TLS library, subresource loading,
  charset decoding, real cache semantics.

## 1. Bytes → tokens (HTML tokenizer)

- **Input:** the decoded document body as a `&str`.
- **Output:** `Vec<HtmlToken>` (doctype, start/end tags, text, comments).
- **Owning crate:** `mocha_html` (`tokenizer.rs`).
- **Current limitations:** recognises a tiny hand-written grammar. Internal
  whitespace in text is collapsed to single spaces, a single leading/trailing
  space is preserved on non-empty runs (so spaces around inline `<span>`s
  survive), and whitespace-only runs are dropped. Malformed input is a `Parse`
  error. `<style>`, `<script>`, and `<textarea>` use a **minimal raw-text
  mode**: the body is captured verbatim until the literal close tag (so CSS/JS
  with `<`/`>` and textarea whitespace survive), but this is not the full HTML
  raw-text/RCDATA algorithm. A missing close tag is a `Parse` error.
- **Future expansion:** the HTML5 tokenization state machine and proper raw-text
  modes.

## 2. Tokens → DOM (HTML tree builder)

- **Input:** `Vec<HtmlToken>`.
- **Output:** a `mocha_dom::Document`.
- **Owning crate:** `mocha_html` (`lib.rs`).
- **Current limitations:** a simple explicit stack. Only the supported tag set is
  allowed, now including `style`/`script`/`textarea` (raw-text), the void
  elements `link`, `img`, and `input`, and the form tags `form`, `button`,
  `label`, `select`, `option`. Mismatched and unclosed tags are `Parse` errors.
- **Future expansion:** the HTML5 tree construction algorithm.

## 2.5. Inline script execution (mocha_js + mocha_js_dom)

- **Input:** the parsed `Document` and its inline `<script>` sources (in document
  order). External `<script src>` is `UnsupportedFeature`.
- **Output:** the mutated `Document`.
- **Owning crates:** `mocha_js` (interpreter + host-object mechanism), `mocha_js_dom`
  (window/document/console globals, DOM read/mutate/query, JS event listeners, a
  deterministic timer queue), orchestrated by `mocha_shell`.
- **Behaviour:** scripts run on one shared interpreter and can mutate the DOM
  (text, attributes, class/id, append/remove nodes) and the shared form-control
  state (`value`/`checked`/`disabled`/`selectedIndex`, `form.submit()` — the
  latter is recorded, never navigated). After all scripts and pending timers
  run, the pipeline re-runs style/layout/paint once (coarse invalidation).
- **Current limitations:** see [dom-bindings.md](dom-bindings.md) — a tiny DOM API,
  no incremental relayout, no live `NodeList`/event loop/promises, no security
  model. A script error aborts the render with a clear error.

## 3. DOM tree

- **Owning crate:** `mocha_dom`.
- **Current limitations:** structure only — element/text/comment/doctype nodes,
  parent/child links, traversal. Attributes (`id`, `class`, `style`, …) are
  stored generically. No live collections or scripting interface.

## 4. CSS extraction + parsing

- **Input:** the `Document` (for `<style>` text) and elements' `style` attributes.
- **Output:** `Vec<Stylesheet>` plus per-element inline `Vec<Declaration>`.
- **Owning crates:** discovery/loading in `mocha_resources`
  (`collect_document_stylesheets`: inline `<style>` plus external `<link
  rel="stylesheet">` loaded through `mocha_net` and validated as `text/css`),
  parsing in `mocha_css` (`parse_stylesheet`, `parse_inline_style`). Stylesheets
  are collected in document order so the cascade's "later wins" tie-break holds.
- **Current limitations:** a small selector and property subset; `px` lengths and
  named/hex colors only. Unknown properties, unsupported units, `!important`, and
  unsupported selectors are errors. External CSS is supported; CSS `url(...)`
  resources, web fonts, and `<base>` are not. See
  [subresources.md](subresources.md).
- **Future expansion:** more of the CSS grammar, more value types, `@media`, etc.

## 5. Computed style tree

- **Input:** the `Document` and the parsed stylesheets.
- **Output:** a `StyledNode` tree (`ComputedStyle` per node).
- **Owning crate:** `mocha_style`.
- **Current limitations:** cascade is UA defaults → author rules → inline, with
  specificity and source-order tie-break; inheritance for `color`, `font-size`,
  `font-weight`. No `!important`, origin layers, or user styles. This crate owns
  the default styles that layout used to hard-code.
- **Future expansion:** full cascade origins and a richer UA stylesheet.

## 6. Style → layout

- **Input:** a `&StyledNode` and a `LayoutViewport`.
- **Output:** a `LayoutBox` tree (block / anonymous-block / line / text-run
  boxes) with computed border-box `Rect`s, carrying the color/border fields paint
  needs. Anonymous, line, and text-run boxes have `node_id == None`.
- **Owning crate:** `mocha_layout` (split into `geometry`, `box_tree`, `context`,
  `block`, `inline`, `line`, `debug`).
- **Behaviour:** block-level children stack vertically with a margin/border/
  padding box model; runs of inline content are broken into line boxes of text
  runs, image atoms, and control atoms with word/item wrapping; inline content
  among block siblings is wrapped in anonymous block boxes. A loaded `<img>` is
  a **replaced element**: an `Image(image_id)` box sized from CSS → attributes →
  intrinsic dimensions, laid out inline (default, sharing a line with text and
  raising line height) or block. Form controls are inline replaced items too:
  a `Control(ControlBox)` box with simple default sizes (text input 160×24,
  checkbox/radio 13×13, buttons sized from their label, textarea from
  `rows`/`cols`, select 160×24), overridable by CSS `width`/`height`.
  `display: none` produces no box. See
  [images-and-replaced-elements.md](images-and-replaced-elements.md) and
  [forms-and-controls.md](forms-and-controls.md).
- **Current limitations:** text width is **estimated** (`chars * font * 0.6`),
  not measured; line height is the tallest item on the line. No margin collapse,
  `text-align`, `white-space` modes, hyphenation (long items overflow), floats, or
  positioning. No baseline/`vertical-align` (inline items are top-aligned).
  Inline-level elements are flattened into text runs/atoms; inline
  backgrounds/borders are deferred.
- **Future expansion:** real font metrics, richer inline boxes, baseline
  alignment, and more of the box model.

## 7. Layout → display list

- **Input:** a `&LayoutBox`.
- **Output:** `Vec<DisplayCommand>` (`DrawRect`, `DrawBorder`, `DrawText`,
  `DrawImage`, `DrawControl`).
- **Owning crate:** `mocha_paint`.
- **Current limitations:** a debug representation only. Box-generating boxes
  (block / inline / anonymous-block) emit `DrawRect` for a non-transparent
  background and `DrawBorder` for a non-zero border; text runs emit `DrawText`;
  image boxes emit `DrawImage` (referencing a decoded-image id); control boxes
  emit `DrawControl` (type, geometry, value/label, checked, disabled); line
  boxes paint nothing. No gradients, stacking contexts, or compositing. Drawing
  order follows DOM tree order (no z-index).
- **Future expansion:** a richer command set, z-index stacking, and GPU rendering.

## 8. Display list → output

The display list flows to different outputs depending on the consuming crate:

**Terminal output (mocha_shell):**
- **Owning crate:** `mocha_shell`.
- **Output:** plain text, one command per line (debug format for inspection).
- **Limitations:** no window, no pixel rendering.

**Desktop rasterization (mocha_desktop with mocha_raster):**
- **Owning crates:** `mocha_raster` (rasterizer), `mocha_desktop` (window/chrome).
- **Behaviour:** the display list is rasterized into a pixel buffer via `mocha_raster::rasterize`
  (which converts `DrawRect` / `DrawBorder` / `DrawText` / `DrawImage` / `DrawControl`
  commands into pixels), then `mocha_desktop` composites this buffer with browser
  chrome (address bar, buttons) and draws the result to a window via `minifb` (if
  the `gui` feature is enabled).
- **Browser chrome:** address bar, back/forward/reload buttons, tab area (reserved
  for M13). Chrome is **not** HTML/CSS content — it is drawn natively by `mocha_desktop`.
- **Limitations:** no GPU acceleration, text is rendered with a debug bitmap font,
  image scaling is basic (nearest-neighbor), no antialiasing, no accessibility.

## Interaction (after rendering)

Separate from the render pipeline, the layout tree also supports **hit testing**
(`mocha_layout::hit_test`) to map a point to a DOM node, which feeds the internal
**event system** (`mocha_events`), link **default actions** (`mocha_nav`), and
form **default actions** (`mocha_forms`: checkbox toggle, radio group selection,
reset, submit identification — all honouring `preventDefault` and `disabled`).
See [events.md](events.md) and [forms-and-controls.md](forms-and-controls.md).

**Window input (M12):** `mocha_desktop` routes window clicks and keyboard input
to the browser state. Clicks are hit-tested against the layout tree or chrome
elements. Keyboard input reaches the address bar (if focused) or the page.
See [desktop-shell.md](desktop-shell.md). **Limitations:** no text editing/caret,
no focus/selection, no IME, no pointer/wheel/touch, no drag-drop.

The JavaScript interpreter (`mocha_js`, see
[javascript-interpreter.md](javascript-interpreter.md)) is now part of the render
pipeline: inline `<script>` runs against the DOM via the `mocha_js_dom` bindings
(see [dom-bindings.md](dom-bindings.md)) before style/layout. It can still be run
standalone via `--eval-js`.
