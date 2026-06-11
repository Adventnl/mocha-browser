# Rendering Pipeline

As of Milestone 10 the pipeline is:

```text
input URL/path
  -> URL parse/normalize (mocha_url)
  -> load: file/http, redirects, content-type, cache (mocha_net via mocha_nav)
  -> content-type check + UTF-8 decode
  -> HTML tokenizer
  -> HTML tree builder
  -> DOM tree
  -> form state init from attributes (mocha_forms)
  -> inline <script> execution + DOM/form-state mutation (mocha_js + mocha_js_dom)
  -> subresources: external <link> CSS (mocha_resources) + <img> images (mocha_image)
  -> <style> / <link> / inline style extraction
  -> CSS tokenizer + parser
  -> selector matching + cascade + inheritance
  -> computed style tree (+ replaced-element image boxes + control boxes)
  -> block & inline layout tree (text runs + image boxes + control boxes)
  -> display list (DrawRect / DrawBorder / DrawText / DrawImage / DrawControl)
  -> terminal output
```

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
  boxes paint nothing. **`DrawImage`/`DrawControl` are emitted but no pixels are
  rasterized** — there is no graphics surface and no real widgets. No gradients,
  stacking contexts, or compositing.
- **Future expansion:** a richer command set fed to a real GPU compositor that
  actually rasterizes images.

## 8. Display list → terminal output

- **Owning crate:** `mocha_shell`.
- **Current limitations:** plain text, one command per line. No window.
- **Future expansion:** a desktop window and compositor consuming the same list.

## Interaction (after rendering)

Separate from the render pipeline, the layout tree also supports **hit testing**
(`mocha_layout::hit_test`) to map a point to a DOM node, which feeds the internal
**event system** (`mocha_events`), link **default actions** (`mocha_nav`), and
form **default actions** (`mocha_forms`: checkbox toggle, radio group selection,
reset, submit identification — all honouring `preventDefault` and `disabled`).
See [events.md](events.md) and [forms-and-controls.md](forms-and-controls.md).
There is no real window input yet.

The JavaScript interpreter (`mocha_js`, see
[javascript-interpreter.md](javascript-interpreter.md)) is now part of the render
pipeline: inline `<script>` runs against the DOM via the `mocha_js_dom` bindings
(see [dom-bindings.md](dom-bindings.md)) before style/layout. It can still be run
standalone via `--eval-js`.
