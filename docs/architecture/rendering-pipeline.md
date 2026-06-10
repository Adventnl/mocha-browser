# Rendering Pipeline

As of Milestone 4 the pipeline is:

```text
input URL/path
  -> URL parse/normalize (mocha_url)
  -> load: file/http, redirects, content-type, cache (mocha_net via mocha_nav)
  -> content-type check + UTF-8 decode
  -> HTML tokenizer
  -> HTML tree builder
  -> DOM tree
  -> <style> / inline style extraction
  -> CSS tokenizer
  -> CSS parser
  -> selector matching + cascade + inheritance
  -> computed style tree
  -> block & inline layout tree
  -> display list
  -> terminal output
```

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
  error. `<style>` uses a **minimal raw-text mode**: its body is captured verbatim
  until the literal `</style>` (so CSS with `<`/`>` survives), but this is not the
  full HTML raw-text/RCDATA algorithm. A missing `</style>` is a `Parse` error.
- **Future expansion:** the HTML5 tokenization state machine and proper raw-text
  modes.

## 2. Tokens → DOM (HTML tree builder)

- **Input:** `Vec<HtmlToken>`.
- **Output:** a `mocha_dom::Document`.
- **Owning crate:** `mocha_html` (`lib.rs`).
- **Current limitations:** a simple explicit stack. Only the supported tag set is
  allowed (now including `style`); `<script>` and `<link>` are rejected (the
  latter with a clear "external stylesheets not supported" error). Mismatched and
  unclosed tags are `Parse` errors.
- **Future expansion:** the HTML5 tree construction algorithm.

## 3. DOM tree

- **Owning crate:** `mocha_dom`.
- **Current limitations:** structure only — element/text/comment/doctype nodes,
  parent/child links, traversal. Attributes (`id`, `class`, `style`, …) are
  stored generically. No live collections or scripting interface.

## 4. CSS extraction + parsing

- **Input:** the `Document` (for `<style>` text) and elements' `style` attributes.
- **Output:** `Vec<Stylesheet>` plus per-element inline `Vec<Declaration>`.
- **Owning crates:** extraction in `mocha_style` (`collect_stylesheets`); parsing
  in `mocha_css` (`parse_stylesheet`, `parse_inline_style`).
- **Current limitations:** a small selector and property subset; `px` lengths and
  named/hex colors only. Unknown properties, unsupported units, `!important`, and
  unsupported selectors are errors. No external/linked CSS.
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
  runs with word wrapping; inline content among block siblings is wrapped in
  anonymous block boxes. `display: none` produces no box.
- **Current limitations:** text width is **estimated** (`chars * font * 0.6`),
  not measured; line height is `max_font * 1.2`. No margin collapse, `text-align`,
  `white-space` modes, hyphenation (long words overflow), floats, or positioning.
  Inline-level elements are flattened into text runs (no inline boxes are
  produced); inline backgrounds/borders are deferred.
- **Future expansion:** real font metrics, richer inline boxes, and more of the
  box model.

## 7. Layout → display list

- **Input:** a `&LayoutBox`.
- **Output:** `Vec<DisplayCommand>` (`DrawRect`, `DrawBorder`, `DrawText`), each
  carrying color.
- **Owning crate:** `mocha_paint`.
- **Current limitations:** a debug representation only. Box-generating boxes
  (block / inline / anonymous-block) emit `DrawRect` for a non-transparent
  background and `DrawBorder` for a non-zero border; text runs emit `DrawText`;
  line boxes paint nothing. No images, gradients, stacking contexts, or
  compositing.
- **Future expansion:** a richer command set fed to a real GPU compositor.

## 8. Display list → terminal output

- **Owning crate:** `mocha_shell`.
- **Current limitations:** plain text, one command per line. No window.
- **Future expansion:** a desktop window and compositor consuming the same list.
