# Milestone Roadmap

Mocha Browser is built one milestone at a time. **Milestones 1–10 are implemented
today**; everything after them is direction, not code. Each milestone lists its
goal, what is explicitly not included, and how completion is verified.

## Milestone 1: Engine laboratory — complete

- **Goal:** load a local HTML file and run it through tokenizer → tree builder →
  DOM → layout → display list, printing the display list to the terminal.
- **Not included:** JavaScript, CSS, networking, the real HTML5 algorithm,
  windowing, images, fonts.
- **Verification:** `cargo test --all` passes (unit tests per crate plus the
  end-to-end integration test), and
  `cargo run -p mocha_shell -- examples/basic/index.html` prints a display list
  containing the document's text.

## Milestone 2: Basic CSS engine — complete

- **Goal:** a CSS tokenizer and parser (`mocha_css`), plus selector matching,
  specificity, cascade, inheritance, and computed style (`mocha_style`), with
  layout and paint consuming computed style instead of hard-coded defaults.
  Supports `<style>` blocks and inline `style` attributes.
- **Not included:** external/linked CSS, networking, JavaScript, dynamic DOM
  mutation, media queries, pseudo-classes/elements, attribute selectors,
  `>`/`+`/`~` combinators, `!important`, `em`/`rem`/`%`, `rgb()`/`calc()`/`var()`,
  flexbox/grid.
- **Verification:** unit tests across `mocha_css`/`mocha_style`/`mocha_layout`/
  `mocha_paint`/`mocha_shell` plus integration tests over `examples/styled`, and
  `cargo run -p mocha_shell -- examples/styled/index.html` prints a colored
  display list with `<style>` text not painted.

## Milestone 3: Real layout foundation — complete

- **Goal:** real block and inline formatting — line boxes, word wrapping, and
  anonymous block boxes for mixed content — with a margin/border/padding box
  model, replacing the old vertical-stacking layout and fake inline boxes.
- **Not included:** real font metrics (text measurement stays estimated), margin
  collapse, `text-align`/`white-space`, inline backgrounds/borders, flexbox/grid,
  floats, positioning, JavaScript, networking.
- **Verification:** `mocha_layout` unit tests for block stacking, box-model
  offsets, inline line sharing, word wrapping, and anonymous blocks; paint tests;
  integration tests over `examples/layout/*`; and the `--dump-layout` output.

## Milestone 4: Networking and navigation — complete

- **Goal:** load `file://` and `http://` resources through `mocha_net` with
  redirect following, content-type handling, and a simple in-memory cache, plus a
  `mocha_nav` history (navigate/back/forward/reload). `https://` is deferred.
- **Not included:** HTTPS/TLS, cookies, auth, proxies, HTTP/2-3, real HTTP cache
  semantics, charset decoding beyond UTF-8, subresource loading (external CSS,
  images, scripts), origin/security policy, JavaScript.
- **Verification:** `mocha_url`/`mocha_net`/`mocha_nav` unit tests plus shell
  integration tests against a localhost `std::net` test server (success,
  redirect, redirect loop, text/plain rejection, cache hit, back/forward).

## Milestone 5: DOM events — complete

- **Goal:** an internal event model (`mocha_events`) with capture/target/bubble
  dispatch, listener registration/removal, `once` listeners, propagation control
  and cancelation, plus `click`/mouse/keyboard event data, a layout hit-test
  bridge, minimal `<a href>` support, and a link navigation default action.
- **Not included:** JavaScript listeners (Milestone 7), real window/OS input,
  pointer/touch/wheel/focus events, `passive` listeners, accessibility events.
- **Verification:** `mocha_events` dispatch/order/propagation tests, `mocha_dom`
  helper tests, `mocha_layout` hit-test tests, `mocha_nav` default-action tests,
  and an `events_pipeline` integration test (hit-test link → dispatch click →
  resolve navigation; `preventDefault` suppresses it).

## Milestone 6: Custom JavaScript interpreter — complete

- **Goal:** a from-scratch JS lexer, parser, AST, and tree-walking interpreter
  (`mocha_js`) with numbers/strings/booleans/null/undefined, objects, arrays,
  functions and closures, `if`/`while`/`for`, operators, `console.log` capture,
  small `Math`/array/string built-ins, and an execution step limit. No
  third-party JS engine or parser. Standalone evaluation via `--eval-js`.
- **Not included:** DOM bindings, `<script>` execution, timers, promises,
  async/await, modules, classes, prototypes/full `this`, JIT, full ECMAScript.
- **Verification:** `mocha_js` lexer/parser/interpreter tests (arithmetic,
  control flow, functions, closures, objects/arrays, built-ins, error paths,
  step limit) plus shell `--eval-js` tests.

## Milestone 7: JavaScript DOM bindings — complete

- **Goal:** a real host-object mechanism in `mocha_js` plus a `mocha_js_dom`
  bridge that installs `window`/`document`/`console` globals, exposes DOM
  read/mutate/query APIs (`getElementById`, `querySelector(All)`,
  `createElement`/`createTextNode`, `appendChild`/`removeChild`, `textContent`,
  `innerHTML`, `getAttribute`/`setAttribute`, `id`/`className`), JS event
  listeners, and a deterministic `setTimeout`/`clearTimeout` queue. Inline
  `<script>` runs in document order, then style/layout/paint run once (coarse
  invalidation).
- **Not included:** full Web IDL surface, live `NodeList`, MutationObserver, real
  event loop/microtasks, promises/modules/classes, external `<script src>`,
  incremental relayout, security model. See
  [dom-bindings.md](dom-bindings.md).
- **Verification:** `mocha_js` host tests, `mocha_dom` mutation/query tests,
  `mocha_style` query-selector tests, `mocha_js_dom` tests, and a
  `js_dom_pipeline` integration test (script mutates DOM and the display list
  changes; created element renders; style/class mutation changes the final paint;
  JS click listener + `preventDefault`; timers).

## Milestone 8: Subresource loading — complete

- **Goal:** discover and load external `<link rel="stylesheet">` stylesheets
  (`mocha_resources`), resolve them against the document base URL (with
  dot-segment normalization in `mocha_url`), validate content type, and integrate
  them into the document-order cascade. `<link>` becomes a void element.
- **Not included:** external `<script src>`, CSS `url(...)` resources, web fonts,
  a `<base>` element, dynamic/incremental subresource loading. See
  [subresources.md](subresources.md).
- **Verification:** `mocha_resources` unit tests and a `subresource_pipeline`
  integration test (external CSS over local file and HTTP, content-type
  validation, cascade order, inline-style precedence, clear errors).

## Milestone 9: Images and replaced elements — complete

- **Goal:** `<img>` support — void-element parsing, base-URL resolution, loading
  through `mocha_net`, PNG/JPEG decoding (`mocha_image`, built on the `image`
  crate), intrinsic/attribute/CSS sizing, inline and block replaced-element
  layout, and a `DrawImage` display command.
- **Not included:** rasterization to a window, responsive images
  (`srcset`/`<picture>`), SVG, animation, canvas, baseline/`vertical-align`,
  image backgrounds/borders. See
  [images-and-replaced-elements.md](images-and-replaced-elements.md).
- **Verification:** `mocha_image` decode tests, `mocha_resources` image tests, and
  an `image_pipeline` integration test (intrinsic/attribute/CSS sizing, inline
  image in document order, block stacking, `DrawImage` emission, content-type
  rejection, missing/corrupt image failing clearly).

## Milestone 10: Forms and basic input controls — complete

- **Goal:** parse `form`/`input`/`button`/`label`/`textarea`/`select`/`option`;
  track dynamic control state (value/checked/selected/disabled) outside the DOM
  in `mocha_forms::FormState`; expose `value`/`checked`/`disabled`/`type`/
  `name`/`selectedIndex` and `form.submit()` to JavaScript; lay controls out as
  inline replaced items with simple default sizes; paint `DrawControl`
  commands; apply click default actions (checkbox toggle, radio group
  selection, form reset, submit identification) honouring `preventDefault` and
  `disabled`; and model GET submission (successful-control collection, base-URL
  action resolution, form-urlencoded query). `--dump-form-state` prints the
  control state.
- **Not included:** keyboard text editing, focus/caret/selection/IME,
  validation, POST bodies / `multipart/form-data`, file/date/color/range/number
  inputs, `:checked`/`:disabled`/`:focus` pseudo-classes, `<optgroup>` /
  `<fieldset>` / multiple-select, the `form` attribute, label activation,
  autofill, real rendered widgets, automatic form navigation. See
  [forms-and-controls.md](forms-and-controls.md).
- **Verification:** `mocha_forms` state/default-action/submission tests,
  `mocha_html` form-parsing tests, `mocha_style`/`mocha_layout`/`mocha_paint`
  control tests, `mocha_js_dom` form-binding tests, shell pipeline tests, and a
  `forms_pipeline` integration test (examples render `DrawControl`, JS state
  changes reach the display list, clicks toggle/select/submit unless prevented,
  GET URLs build correctly, POST fails clearly, old examples still run).

## Milestone 11: Desktop window shell — complete

- **Goal:** a real raster/window surface that draws the existing display list
  (rectangles, borders, text, images, controls) instead of printing it.
- **Not included:** tabs, address bar, navigation chrome, OS input wiring beyond
  the minimum, GPU compositing.
- **Verification:** `cargo test -p mocha_desktop` passes; all terminal examples still work; window opens and displays page with scrolling and click input routing.

## Milestone 12: Browser chrome — complete

- **Goal:** minimal browser UI (toolbar, address bar, back/forward/reload buttons)
  around a single-page document viewer. Testable state machine (`BrowserAppState`)
  separate from the window loop. Simple navigation history (back/forward).
- **Not included:** tabs (M13), error pages, loading indicators, page titles,
  bookmarks, settings, bookmarks database, HTTPS, cookies, keyboard shortcuts,
  search suggestions.
- **Verification:** `cargo test -p mocha_desktop` passes (all tests including chrome
  layout, address bar, history); terminal examples work unchanged; desktop window
  shows simple chrome (colored buttons, address bar text field), address bar accepts
  input, back/forward/reload buttons work.

## Milestone 13: Tabs and in-memory session model — complete

- **Goal:** turn the single-page desktop shell into a multi-tab browser with a
  tab strip, per-tab page/navigation/scroll/focus, a simple internal new-tab
  page, and an in-memory session snapshot/restore. All in `mocha_desktop`.
- **Not included:** persistent session files, profile directory, SQLite,
  cookies, localStorage, bookmarks/history/downloads databases, settings UI, tab
  drag/reorder, pinned tabs, tab groups, private browsing, crash recovery,
  multiprocess isolation. Sessions are **in-memory only** (persistence is M14).
- **Verification:** `cargo test -p mocha_desktop` (tab manager: start/new/switch/
  close policy, stable+unique ids, order, always-valid active; session snapshot/
  restore; chrome tab-strip layout + hit testing; address bar follows active tab;
  navigation/back/forward affect the active tab only); all terminal examples and
  `--dump-display-list` still work. See [tabs-and-session.md](tabs-and-session.md).

## Beyond Milestone 13 (direction, not code)

- **Profile and storage system (next, M14):** a profile directory, schema
  migrations, and history/bookmarks/settings/downloads/session persistence.
- **Web state (M15):** cookies and origin-aware `localStorage`/`sessionStorage`.
- **Multi-process architecture:** a browser process and renderer process(es) with
  typed IPC and crash recovery.
- **Security foundation:** an origin model, same-origin checks, mixed content.
- **Web compatibility hardening:** standards test suites, fuzzing, visual
  regression — without promising full web compatibility.
