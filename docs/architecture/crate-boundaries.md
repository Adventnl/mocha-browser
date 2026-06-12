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

mocha_forms
- form-control state (value/checked/selected/disabled keyed by node), form
  ownership, click default actions (checkbox/radio/reset/submit), and the GET
  form-submission model (successful controls + form-urlencoded query URLs)
- no HTML tokenization, no CSS/layout/paint, no network execution, no UI
- depends on: mocha_error, mocha_dom, mocha_url, mocha_events

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
  script execution against a shared Document, and form-control properties
  (value/checked/disabled/…, form.submit()) backed by a shared
  mocha_forms::FormState
- no layout/paint/network knowledge
- depends on: mocha_error, mocha_js, mocha_dom, mocha_forms, mocha_html,
  mocha_style

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

mocha_engine
- high-level document loading and rendering orchestration (M1–M10 pipeline)
- loads via mocha_nav/mocha_net, runs inline scripts (mocha_js_dom), loads
  subresources (mocha_resources/mocha_image), computes style/layout/paint
- produces a display list and image refs, completely agnostic to output (terminal
  or window)
- used by both mocha_shell and mocha_desktop; the core stateless pipeline
- depends on: mocha_error, mocha_url, mocha_html, mocha_dom, mocha_style,
  mocha_layout, mocha_paint, mocha_net, mocha_nav, mocha_js, mocha_js_dom,
  mocha_forms, mocha_resources, mocha_image

mocha_raster
- display list + images to pixel buffer rasterization (M11)
- owns the Surface (framebuffer), drawing primitives (rect/text/image),
  and command-to-pixel conversion
- stateless: rasterize(surface, commands, images, scroll_y) → pixels
- no window, no input, no event loop
- depends on: mocha_error, mocha_paint, mocha_image, mocha_layout (Color)

mocha_desktop
- desktop shell, browser app state, and tabs (M11–M13)
- BrowserAppState: state machine over a TabManager + chrome + address-bar + focus
- TabManager / BrowserTab / TabId: tab list, active-tab invariant, per-tab
  page/history/scroll/focus (M13)
- SessionSnapshot / SessionTab: in-memory, metadata-only session capture/restore (M13)
- InternalPage::NewTab: offline new-tab page
- DesktopPageState: per-tab document loading/rendering (calls mocha_engine)
- ChromeLayout: tab-strip/toolbar/button/address-bar positioning and hit testing
- AddressBarState: address bar editing (app-level draft)
- window.rs: native window event loop (thin, untestable layer using minifb)
- fully testable without a window; window.rs is intentionally untestable
- optional `gui` feature: enables minifb for visible windowing
- depends on: mocha_error, mocha_url, mocha_engine, mocha_raster, minifb (optional)

mocha_shell
- command-line executable (library + binary)
- alternative front-end to mocha_engine (terminal output instead of windowing)
- loads via mocha_engine, exposes hit testing (--hit-test), form-state dumping
  (--dump-form-state), and standalone JS (--eval-js)
- no window, no browser UI; purely for terminal output and testing
- depends on: mocha_error, mocha_engine
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
- `mocha_forms` owns form *semantics* (state, default actions, submission) and,
  like `mocha_nav`, may depend on `mocha_events` for its default-action helper.
  Style/layout/paint stay forms-agnostic: the shell resolves form state into a
  plain `ControlBox` (defined in `mocha_style`, mirroring the image
  `ReplacedBox`), which layout turns into a `Control` box and paint into a
  `DrawControl` command. Submission produces a URL; it never performs network
  I/O (that stays in `mocha_net`/`mocha_nav`).
- `mocha_engine` (M11+) is the unified document loading/rendering pipeline,
  shared by all frontends (terminal and desktop). This keeps pipeline logic
  in one place and allows multiple UIs to consume the same display list.
- `mocha_raster` (M11+) owns pixel rasterization and is decoupled from windowing.
  The Surface is a simple CPU framebuffer; no GPU, no compositor. `mocha_desktop`
  calls `mocha_raster` and passes the pixel buffer to `minifb` for display.
- `mocha_desktop` (M11–M13) owns browser state (tabs/page/chrome/address-bar/
  history/session), which is entirely testable (`BrowserAppState`, `TabManager`,
  and session snapshot/restore tests all pass without a window).
  The window event loop (`window.rs`) is a thin untestable layer that pumps
  events and drives the rasterizer. The optional `gui` feature gates the
  `minifb` dependency.
- `mocha_shell` is now a pure frontend (no pipeline logic); it uses `mocha_engine`
  and prints the display list as text. Both `mocha_shell` and `mocha_desktop`
  use the same engine, ensuring they render identically (except for output format).
- Future crates (`mocha_gpu`, `mocha_security`, …) are intentionally **not**
  created yet. They are described in [milestones.md](milestones.md) as
  direction only.
