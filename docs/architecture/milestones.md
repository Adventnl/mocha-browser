# Milestone Roadmap

Mocha Browser is built one milestone at a time. **Milestones 1–9 are implemented
today**; everything after them is direction, not code. Each milestone lists its
goal, what is explicitly not included, and how completion is verified.

## Milestone 1: Engine laboratory — done

- **Goal:** load a local HTML file and run it through tokenizer → tree builder →
  DOM → layout → display list, printing the display list to the terminal.
- **Not included:** JavaScript, CSS, networking, the real HTML5 algorithm,
  windowing, images, fonts.
- **Verification:** `cargo test --all` passes (unit tests per crate plus the
  end-to-end integration test), and
  `cargo run -p mocha_shell -- examples/basic/index.html` prints a display list
  containing the document's text.

## Milestone 2: Basic CSS engine — done (current)

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

## Milestone 3: Real layout foundation — done (current)

- **Goal:** real block and inline formatting — line boxes, word wrapping, and
  anonymous block boxes for mixed content — with a margin/border/padding box
  model, replacing the old vertical-stacking layout and fake inline boxes.
- **Not included:** real font metrics (text measurement stays estimated), margin
  collapse, `text-align`/`white-space`, inline backgrounds/borders, flexbox/grid,
  floats, positioning, JavaScript, networking.
- **Verification:** `mocha_layout` unit tests for block stacking, box-model
  offsets, inline line sharing, word wrapping, and anonymous blocks; paint tests;
  integration tests over `examples/layout/*`; and the `--dump-layout` output.

## Milestone 4: Networking and navigation — done (current)

- **Goal:** load `file://` and `http://` resources through `mocha_net` with
  redirect following, content-type handling, and a simple in-memory cache, plus a
  `mocha_nav` history (navigate/back/forward/reload). `https://` is deferred.
- **Not included:** HTTPS/TLS, cookies, auth, proxies, HTTP/2-3, real HTTP cache
  semantics, charset decoding beyond UTF-8, subresource loading (external CSS,
  images, scripts), origin/security policy, JavaScript.
- **Verification:** `mocha_url`/`mocha_net`/`mocha_nav` unit tests plus shell
  integration tests against a localhost `std::net` test server (success,
  redirect, redirect loop, text/plain rejection, cache hit, back/forward).

## Milestone 5: DOM events — done (current)

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

## Milestone 6: Custom JavaScript interpreter — done (current)

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

## Milestone 7: JavaScript DOM bindings — done (current)

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

## Milestone 8: Subresource loading — done (current)

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

## Milestone 9: Images and replaced elements — done (current)

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

## Milestone 10: Forms and basic input controls — next

- **Goal:** parse basic form controls and wire input/change/submit behaviour into
  the event and (eventually) navigation layers.
- **Not included:** a full forms/validation model.
- **Verification:** tests for control parsing and form event/submit behaviour.

## Beyond Milestone 10 (direction, not code)

- **Multi-process architecture:** a browser process and renderer process(es) with
  typed IPC and crash recovery.
- **Storage and profile system:** history, bookmarks, settings, session restore.
- **Security foundation:** an origin model, same-origin checks, mixed content.
- **Desktop product shell:** tabs, address bar, navigation chrome, and a real
  raster/window surface to actually draw the display list (including images).
- **Web compatibility hardening:** standards test suites, fuzzing, visual
  regression — without promising full web compatibility.
