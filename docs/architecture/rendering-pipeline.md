# Rendering Pipeline

As of Milestone 2 the pipeline is:

```text
bytes
  -> HTML tokenizer
  -> HTML tree builder
  -> DOM tree
  -> <style> / inline style extraction
  -> CSS tokenizer
  -> CSS parser
  -> selector matching + cascade + inheritance
  -> computed style tree
  -> layout tree
  -> display list
  -> terminal output
```

Each stage is described below with its input, output, owning crate, current
limitations, and intended future expansion.

## 1. Bytes → tokens (HTML tokenizer)

- **Input:** the file contents as a `&str` (read by `mocha_shell`).
- **Output:** `Vec<HtmlToken>` (doctype, start/end tags, text, comments).
- **Owning crate:** `mocha_html` (`tokenizer.rs`).
- **Current limitations:** recognises a tiny hand-written grammar. Whitespace is
  collapsed and whitespace-only text is dropped. Malformed input is a `Parse`
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
- **Output:** a `LayoutBox` tree with computed border-box `Rect`s, carrying the
  color/border fields paint needs.
- **Owning crate:** `mocha_layout`.
- **Current limitations:** vertical stacking with a simple box model
  (margin/border/padding). `display: none` produces no box. Width/height
  properties override the defaults. Text size is estimated, not measured. No real
  inline formatting, wrapping, floats, or positioning.
- **Future expansion:** a real block/inline formatting model with text
  measurement and wrapping (Milestone 3).

## 7. Layout → display list

- **Input:** a `&LayoutBox`.
- **Output:** `Vec<DisplayCommand>` (`DrawRect`, `DrawBorder`, `DrawText`), each
  carrying color.
- **Owning crate:** `mocha_paint`.
- **Current limitations:** a debug representation only. Non-transparent
  backgrounds emit `DrawRect`; non-zero borders emit `DrawBorder`; text emits
  `DrawText`. No images, gradients, stacking contexts, or compositing.
- **Future expansion:** a richer command set fed to a real GPU compositor.

## 8. Display list → terminal output

- **Owning crate:** `mocha_shell`.
- **Current limitations:** plain text, one command per line. No window.
- **Future expansion:** a desktop window and compositor consuming the same list.
