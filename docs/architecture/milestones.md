# Milestone Roadmap

Mocha Browser is built one milestone at a time. **Milestones 1–21 are implemented
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

## Milestone 14: Profile storage — complete

- **Goal:** a persistent browser-profile foundation: the `mocha_storage` crate
  (embedded SQLite via `rusqlite` `bundled`) with a profile directory, versioned
  schema migrations, and history/bookmarks/settings/downloads/session stores, plus
  a private (in-memory) profile mode.
- **Not included:** cookies and origin-keyed web storage (M15), encryption, sync,
  passwords, full UI surfacing (the desktop integration is one headless command),
  multi-process access.
- **Verification:** `cargo test -p mocha_storage` (profile dir creation + file
  rejection; migration version/idempotency/tables; history/bookmarks/settings/
  downloads/session CRUD; persistent-reopen vs. private non-persistence) and the
  `--profile DIR --dump-session` desktop command. See
  [profile-storage.md](profile-storage.md).

## Milestone 15: Cookies and origin-aware storage — complete

- **Goal:** browser web-state foundations — a minimal origin model
  (`mocha_origin`), a cookie model/jar (`mocha_cookie`), cookie + `localStorage`
  persistence in `mocha_storage` (migration 2), an `http` cookie integration in
  `mocha_net` (a `CookieProvider` trait), and `document.cookie` /
  `localStorage` / `sessionStorage` JS bindings.
- **Not included:** full RFC 6265bis, third-party/partitioned-cookie policy,
  cookie UI, real `SameSite` enforcement, IndexedDB/Cache API/quotas/StorageEvent,
  a complete security model. JS-side `localStorage` persistence and automatic
  page-load cookie wiring are deferred.
- **Verification:** `cargo test -p mocha_origin -p mocha_cookie -p mocha_storage`
  (origin equality/normalization/file policy; cookie parse/match/order; cookie +
  localStorage persistence and private non-persistence) and `cargo test -p
  mocha_net` (local-test-server cookie round-trips) and `cargo test -p
  mocha_js_dom` (document.cookie + web storage bindings). See
  [cookies-and-web-storage.md](cookies-and-web-storage.md) and
  [origin-model.md](origin-model.md).

## Milestone 16: Origin model and security foundation — complete

- **Goal:** add `mocha_security`, a policy crate for explicit security decisions:
  same-origin checks, URL context restrictions, conservative file policy helpers,
  mixed-content awareness, a tiny CSP parser/evaluator, permission state,
  certificate error data, and renderer/browser/network capability sets.
- **Not included:** full sandboxing, complete web security, site isolation,
  HTTPS/TLS, full CSP, CORS, Fetch, or broad runtime enforcement across every
  render path.
- **Verification:** `cargo test -p mocha_security` plus the full workspace gate.
  See [security-foundation.md](security-foundation.md) and
  [content-security-policy.md](content-security-policy.md).

## Milestone 17: Multi-process architecture prototype — complete

- **Goal:** add `mocha_ipc` and `mocha_process`, a versioned typed IPC protocol,
  a `mocha_renderer` child process, renderer lifecycle management, render
  request/response, clean shutdown, test crash handling, and respawn.
- **Not included:** OS sandboxing, site isolation, network process, GPU process,
  production crash reporting, or moving the normal desktop path to multiprocess
  by default. The M17 renderer may still call `mocha_engine::render_url`
  directly and therefore has direct file/http capability.
- **Verification:** `cargo test -p mocha_ipc`, `cargo test -p mocha_process`,
  and the full workspace gate. See [ipc.md](ipc.md) and
  [multiprocess-prototype.md](multiprocess-prototype.md).

## Milestone 18: Security sandbox prototype — complete

- **Goal:** add `mocha_sandbox`, a capability-based renderer policy, honest
  platform sandbox status, and a prepared-document renderer path that rejects
  direct URL/file loads after a restricted policy is applied.
- **Not included:** production OS sandboxing, site isolation, network process,
  GPU process, complete resource brokering, or exploit mitigation.
- **Verification:** `cargo test -p mocha_sandbox`, `cargo test -p mocha_ipc`,
  `cargo test -p mocha_process`, and the full workspace gate. See
  [security-sandbox.md](security-sandbox.md) and
  [resource-broker.md](resource-broker.md).

## Milestone 19: DevTools foundation — complete

- **Goal:** add `mocha_devtools`, a deterministic headless inspection snapshot
  for the final DOM, computed styles, layout tree, display list, document
  network metadata, console output, and structured event/storage/security/IPC/
  process logs; expose it through `mocha_shell --devtools-snapshot`.
- **Not included:** Chrome DevTools, CDP, a remote debugging server, breakpoints,
  JavaScript stepping, live editing, heap snapshots, profiling, or UI panels.
- **Verification:** `cargo test -p mocha_devtools`, the shell snapshot smoke
  test, and the full workspace gate. See [devtools.md](devtools.md).

## Milestone 20: Web compatibility hardening and standards test harness — complete

- **Goal:** turn Mocha into a more testable, honest experimental browser by
  defining a measurable [Compatibility Level 1](compatibility-level-1.md) target
  and building the infrastructure to hold the engine to it: the `mocha_compat`
  harness (hand-parsed manifest, snapshot normalization, pass/fail/skip/
  unsupported/xfail runner over `mocha_engine`), a 90-case local compatibility
  suite under `tests/compat`, a malformed-input crash corpus
  (`tests/crash`/`tests/corpus`), raster-checksum visual regression
  (`tests/visual`), and the `mocha_perf` render performance baseline.
- **Not included:** full web compatibility, Chromium-level behaviour,
  web-platform-tests, production security, HTTPS/TLS, or any new web feature.
  The harness measures and protects the existing subset; it does not grow it.
- **Verification:** `cargo run -p mocha_compat -- tests/compat/manifest.toml`
  (no unexpected failures → exit 0), `cargo test --test crash_corpus`,
  `cargo test --test visual_regression`,
  `cargo run -p mocha_perf -- examples/layout/article.html`, and the full
  workspace gate. See [compatibility-testing.md](compatibility-testing.md),
  [performance-baselines.md](performance-baselines.md), and
  [../release-readiness.md](../release-readiness.md).

## Milestone 21: Real networking — HTTP/1.1 hardening and HTTPS/TLS — complete

- **Goal:** make the hand-written HTTP/1.1 client able to talk to real web
  servers: `Transfer-Encoding: chunked` decoding, `Content-Encoding: gzip`
  (the new from-scratch `mocha_gzip` crate — RFC 1951 inflate + RFC 1952
  container with CRC-32/ISIZE verification and a zip-bomb output cap),
  `Content-Length` truncation validation, `Accept-Encoding: gzip` requests, and
  `https://` via **rustls** (the `ring` provider) with certificates verified
  against the embedded Mozilla root store (`webpki-roots`). Redirects now
  follow across http↔https. The localhost test server gains a TLS mode with a
  committed self-signed certificate that tests must trust explicitly
  (`DefaultLoader::with_extra_tls_root`); `mocha_security` allows https
  subresources and form-action URLs.
- **Not included:** keep-alive/pipelining, HTTP/2-3, `br`/`zstd`/`deflate`
  encodings (clear errors), real HTTP cache semantics, certificate-error
  interstitials or overrides, revocation (CRL/OCSP), HSTS, client certificates,
  the OS trust store, TLS UI (padlock), POST, authentication, and proxies. TLS
  itself is a vetted library (a deliberate exception to "from scratch" — TLS is
  never hand-rolled); the HTTP protocol and gzip/DEFLATE decoding stay
  hand-written.
- **Verification:** `cargo test -p mocha_gzip` (inflate block types, real GNU
  gzip fixtures, corrupt/truncated/bomb errors), `cargo test -p mocha_net`
  (chunked/gzip/truncation over a live localhost server; TLS handshake against
  the rustls test server; default rejection of its self-signed certificate;
  http→https redirect), updated engine/shell/integration/compat https cases,
  the full workspace gate, and a live load:
  `cargo run -p mocha_shell -- https://example.com/`.

## Beyond Milestone 21 (direction, not code)

- **Expand compatibility coverage:** more cases, a server-backed compat mode for
  cookies/storage/CSP, and broader categories — without promising full web
  compatibility or importing web-platform-tests.
- **Text quality** (real font metrics/glyph rasterization, font fallback, line
  breaking), **JS/DOM and CSS/layout improvements**, **stronger sandboxing**,
  **performance work**, **accessibility**, and **DevTools growth** — each a
  separate, honest milestone.
