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
- block + inline layout consuming computed style
- modules: geometry, box_tree, context, block, inline, line, debug
- produces block / anonymous-block / line / text-run boxes
- no CSS parsing, no DOM dependency
- depends on: mocha_error, mocha_style

mocha_paint
- display list generation (colors, borders)
- no window rendering, no CSS parsing
- depends on: mocha_error, mocha_layout

mocha_net
- resource loading: file + http (no TLS), redirects, content-type, memory cache
- no navigation history, no HTML/CSS/layout/paint
- depends on: mocha_error, mocha_url

mocha_events
- internal DOM event model and dispatch (capture/target/bubble)
- listeners are Rust callbacks; no JavaScript, no URL/navigation knowledge
- depends on: mocha_error, mocha_dom

mocha_js
- from-scratch JavaScript-subset interpreter (lexer/parser/AST/interpreter)
- a host-object mechanism (JsValue::Host + the HostObject trait) lets embedders
  back JS values with native state, but mocha_js itself has no DOM/HTML/CSS/
  events/network knowledge
- no existing JS engine or parser is used
- depends on: mocha_error

mocha_image
- image format detection + PNG/JPEG decoding (intrinsic dimensions only)
- the ONLY crate with a third-party dependency: the `image` crate (png+jpeg)
- no network/HTML/layout/DOM knowledge
- depends on: mocha_error, image (external)

mocha_js_dom
- bridges mocha_js to mocha_dom: window/document/console globals, DOM
  read/mutate/query, JS event listeners, a deterministic timer queue, inline
  script execution against a shared Document
- no layout/paint/network knowledge
- depends on: mocha_error, mocha_js, mocha_dom, mocha_html, mocha_style

mocha_resources
- subresource discovery + loading (external <link> CSS, <img> images) resolved
  against a base URL, with content-type validation; preserves document order
- no raw HTTP, no layout/paint/JS
- depends on: mocha_error, mocha_url, mocha_dom, mocha_css, mocha_net, mocha_image

mocha_nav
- navigation history (navigate/back/forward/reload) over a ResourceLoader
- link default-action interpretation (click on <a href> → Navigate)
- no protocol details, no rendering
- depends on: mocha_error, mocha_url, mocha_net, mocha_dom, mocha_events

mocha_shell
- command-line executable (library + binary)
- loads via mocha_nav/mocha_net, runs inline scripts (mocha_js_dom), loads
  subresources (mocha_resources/mocha_image), then renders through the engine
- exposes hit testing (--hit-test) and standalone JS (--eval-js); no browser UI yet
- depends on: mocha_error, mocha_url, mocha_html, mocha_dom, mocha_style,
  mocha_layout, mocha_paint, mocha_net, mocha_nav, mocha_js, mocha_js_dom,
  mocha_resources, mocha_image
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
- `mocha_net` performs the actual network/file I/O; `mocha_shell` no longer reads
  files or speaks HTTP directly — it loads through `mocha_nav`/`mocha_net` and
  then renders. `mocha_net` depends on no rendering crate, and `mocha_nav` owns
  only history (it does not render — that boundary keeps navigation reusable).
- See [networking-and-navigation.md](networking-and-navigation.md) for the
  loading/navigation design, [events.md](events.md) for the event system, and
  [javascript-interpreter.md](javascript-interpreter.md) for the JS interpreter.
- `mocha_js` stays DOM-agnostic: it provides only a generic host-object mechanism
  (`JsValue::Host`), and the DOM↔JS bridge lives in the separate `mocha_js_dom`
  crate (Milestone 7). See [dom-bindings.md](dom-bindings.md). The interpreter is
  still usable standalone via `--eval-js`.
- `mocha_image` is the single boundary where a third-party dependency lives. Mocha
  does not write an image decoder from scratch; `mocha_image` wraps the `image`
  crate behind a tiny `MochaResult`-returning API so no other crate sees it.
- `mocha_resources` owns subresource policy (discovery, base-URL resolution,
  content-type checks) and depends on `mocha_net` for I/O but not on
  layout/paint/JS — keeping subresource loading reusable.
- `mocha_events` is the event core and stays free of URL/navigation knowledge;
  link default-action interpretation lives in `mocha_nav` (which may depend on
  `mocha_events`). JS event listeners are dispatched by `mocha_js_dom` (which has
  the live interpreter), mirroring `mocha_events`' semantics. The point→node
  `hit_test` bridge lives in `mocha_layout`. This keeps each boundary one-directional.
- Future crates (`mocha_gpu`, `mocha_security`, `mocha_browser`, …) are
  intentionally **not** created yet. They are described in
  [milestones.md](milestones.md) as direction only.
