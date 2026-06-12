# Mocha Browser

Mocha Browser is an experimental from-scratch browser engine and desktop browser.

Mocha is not based on Chromium, WebKit, Gecko, Servo, Electron, CEF, Tauri WebView, system WebView, V8, SpiderMonkey, JavaScriptCore, QuickJS, Deno, or Node.js.

Current status: Milestone 20 (web compatibility hardening) implemented. Mocha is a
functioning **experimental** browser, not a production browser and **not
Chromium-compatible**.

Mocha is not safe for general web browsing yet.

| Subsystem | Status | Notes |
| --- | --- | --- |
| HTML | Basic subset | Not HTML5-complete; no `<head>`/`<title>`, error recovery, or tag soup |
| CSS | Basic subset | No flex/grid/positioning/float/media queries |
| Layout | Basic block/inline | No tables/flex/grid; fixed-advance debug font |
| JS | Tiny custom interpreter | Not ECMAScript-compliant; no classes/promises/modules |
| DOM | Basic bindings | Not the full Web API surface; no real event loop |
| Network | HTTP/file | HTTPS/TLS unsupported |
| Storage | Profile/cookies/localStorage foundations | Needs an http(s) origin; minimal |
| Security | Policy/sandbox/process prototypes | Not production-secure; not site isolation |
| Desktop | Basic browser UI + tabs | Experimental |
| DevTools | Headless snapshots | Not Chrome DevTools / CDP |
| Compatibility | Local harness (Level 1) | Experimental subset; not web-platform-tests |

See [docs/architecture/compatibility-level-1.md](docs/architecture/compatibility-level-1.md)
for the precise supported subset and [docs/release-readiness.md](docs/release-readiness.md)
for how to run everything.

## Project goals

Build a real, understandable browser engine from first principles, one small
milestone at a time. The long-term architecture is inspired by modern
multi-process browsers, but the current implementation is intentionally tiny.
Correctness and honesty are valued over breadth: unsupported behaviour fails
with a clear error rather than being faked.

## Current milestone

**Milestone 16: Origin model and security foundation.** The `mocha_security`
crate defines explicit security decisions and policy objects: same-origin checks,
URL context restrictions, conservative `file://` policy helpers, mixed-content
awareness, a tiny CSP parser/evaluator, origin-keyed permission state,
future-facing certificate error data, and renderer/browser capability sets. This
is **not** a full sandbox, complete web security, site isolation, or HTTPS/TLS.
See [security-foundation.md](docs/architecture/security-foundation.md) and
[content-security-policy.md](docs/architecture/content-security-policy.md).

**Milestone 17: Multi-process architecture prototype.** The `mocha_ipc` and
`mocha_process` crates add a versioned typed IPC protocol, a `mocha_renderer`
child process, a browser-side renderer manager, clean shutdown, test crash
detection, and respawn. This is **not** a production multi-process browser, not
site isolation, and not OS sandboxing. Normal shell/desktop paths remain
single-process by default. See [ipc.md](docs/architecture/ipc.md) and
[multiprocess-prototype.md](docs/architecture/multiprocess-prototype.md).

**Milestone 18: Security sandbox prototype.** The `mocha_sandbox` crate adds a
capability-based renderer policy, honest platform sandbox status, and a
prepared-document path where the renderer receives already-loaded HTML instead
of a file path or URL to load. This is **not** a production OS sandbox, not site
isolation, and not secure for general browsing. See
[security-sandbox.md](docs/architecture/security-sandbox.md) and
[resource-broker.md](docs/architecture/resource-broker.md).

**Milestone 20: Web compatibility hardening and standards test harness.** Two
crates — `mocha_compat` (a local compatibility test harness: a hand-parsed
manifest, snapshot normalization, and a pass/fail/skip/unsupported runner over
`mocha_engine`) and `mocha_perf` (a render performance baseline) — plus a 90-case
[Compatibility Level 1](docs/architecture/compatibility-level-1.md) suite, a
malformed-input crash corpus (`tests/crash`/`tests/corpus`), and raster-checksum
visual regression (`tests/visual`). This defines and measures a small, honest
compatibility baseline and hardens the engine. It is **not** web-platform-tests,
**not** Chromium-level compatibility, and proves nothing about modern web pages.
See [compatibility-testing.md](docs/architecture/compatibility-testing.md),
[performance-baselines.md](docs/architecture/performance-baselines.md), and
[release-readiness.md](docs/release-readiness.md).

**Milestone 19: DevTools foundation.** The `mocha_devtools` crate adds a
deterministic headless snapshot model for the final DOM, computed styles, layout
tree, display list, document network metadata, console output, and structured
event/storage/security/IPC/process logs. `mocha_shell --devtools-snapshot`
prints that snapshot for local files or `http://` URLs. This is **not** Chrome
DevTools, not CDP, and not an interactive debugger. See
[devtools.md](docs/architecture/devtools.md).

**Milestone 15: Cookies and origin-aware storage.** Two crates — `mocha_origin`
(a minimal `(scheme, host, port)` origin with a conservative `file://` policy) and
`mocha_cookie` (`Set-Cookie` parsing and an in-memory jar with domain/path/secure/
expiry matching and deterministic `Cookie` headers) — plus persistence in
`mocha_storage` (a `cookies` table and an origin-keyed `local_storage` table via
schema migration 2). `mocha_net` gains a `CookieProvider` trait so the HTTP client
can attach `Cookie` and store `Set-Cookie` without depending on storage. The JS
runtime exposes `document.cookie` and `localStorage`/`sessionStorage` (origin-keyed;
unavailable without an http(s) origin). See
[cookies-and-web-storage.md](docs/architecture/cookies-and-web-storage.md) and
[origin-model.md](docs/architecture/origin-model.md). This is **not** a complete
cookie or security model.

**Milestone 14: Profile storage.** The `mocha_storage` crate is the persistent
browser-profile foundation backed by embedded SQLite (`rusqlite` `bundled`): visit
history, bookmarks, settings, download metadata, and a persisted session snapshot,
with versioned migrations and a **private** (in-memory) profile mode. The desktop
shell can open a profile and save/restore a session (`--profile DIR --dump-session`).
See [docs/architecture/profile-storage.md](docs/architecture/profile-storage.md).

**Milestone 13 (tabs and in-memory session)** remains the desktop model: a
`TabManager` owns the open tabs and the active-tab invariant; each `BrowserTab`
keeps its own page, navigation history, scroll, and focus; a tab strip sits above
the toolbar; the address bar follows the active tab; and an in-memory
`SessionSnapshot` (now persistable via `mocha_storage`) captures tab metadata. See
[docs/architecture/tabs-and-session.md](docs/architecture/tabs-and-session.md).

The page content still renders via the shared `mocha_engine` pipeline through
style/layout/paint into a display list, then `mocha_raster` for pixel
rasterization and `mocha_desktop` for windowing. The command-line shell still
exists for terminal output mode.

## What works

- Parsing a small, well-formed subset of HTML (`html`, `body`, `h1`, `h2`, `p`,
  `div`, `span`, `a`, `style`, `script`, `link`, `img`, `form`, `input`,
  `button`, `label`, `textarea`, `select`, `option`, plus doctype and comments).
  `style`, `script`, and `textarea` use minimal raw-text handling; `link`,
  `img`, and `input` are void elements. This is **not** the HTML5 parser.
  `<script src>` is unsupported; `<link>` is honored only for
  `rel="stylesheet"`; `<img>` stores `src`/`width`/`height`/`alt` and lays out
  as a replaced element.
- Building a minimal arena-backed DOM tree.
- Basic CSS from `<style>` blocks and inline `style` attributes:
  type / class / id / universal / descendant selectors, specificity, cascade
  (UA defaults → author rules → inline), and inheritance of `color`,
  `font-size`, and `font-weight`.
- A small property set (`display`, `color`, `background-color`, `font-size`,
  `font-weight`, `width`, `height`, `margin*`, `padding*`, `border-width`,
  `border-color`) with `px` lengths and named / hex colors.
- **Block layout** with a simple margin/border/padding box model, and **inline
  layout** with line boxes, word wrapping, and anonymous block boxes for mixed
  block/inline content. Inline text and `<span>`s share a line until the width
  runs out; long text wraps at word boundaries.
- A display list of `DrawRect` / `DrawBorder` / `DrawText` commands carrying
  colors, plus a layout-tree dump (`--dump-layout`), printed via `mocha_shell`.
- **Document loading** of local paths, `file://`, and `http://` URLs through
  `mocha_net` (a std-only blocking HTTP/1.1 client), with redirect following (up
  to 10), content-type gating (only HTML renders), a simple in-memory cache, and
  a `mocha_nav` back/forward/reload history model.
- **Internal DOM events** (`mocha_events`): capture/target/bubble dispatch,
  listener registration/removal, `once` listeners, `stopPropagation` /
  `stopImmediatePropagation` / `preventDefault`, and `click`/mouse/keyboard event
  data — plus layout **hit testing** (`--hit-test X,Y`), minimal `<a href>`
  support, and a link navigation **default action**. These are engine-internal;
  there is no real window input.
- **A from-scratch JavaScript interpreter** (`mocha_js`): lexer → parser → AST →
  tree-walking interpreter for a small subset — numbers, strings, booleans,
  `null`/`undefined`, objects, arrays, functions, **closures**, `if`/`while`/`for`,
  operators, `console.log` capture, and small `Math`/array/string built-ins, with
  an execution step limit. Run snippets standalone with `--eval-js "<source>"`.
- **JavaScript DOM bindings** (`mocha_js_dom`): a real host-object mechanism wires
  the interpreter to the DOM. Inline `<script>` runs in document order and can use
  `window`/`document`/`console`, `getElementById`/`querySelector(All)`,
  `createElement`/`createTextNode`, `appendChild`/`removeChild`, `textContent`/
  `innerHTML`, `getAttribute`/`setAttribute`, `id`/`className`,
  `addEventListener`, and a deterministic `setTimeout`/`clearTimeout`. DOM
  mutations are reflected in the final style/layout/paint (coarse invalidation).
- **External stylesheets** (`mocha_resources`): `<link rel="stylesheet">` is
  resolved against the document base URL, loaded through `mocha_net`, content-type
  validated, and folded into the document-order cascade (inline `style` still
  wins).
- **Images** (`mocha_image` + the `image` crate): `<img>` is parsed as a void
  element, loaded, and decoded (PNG/JPEG) for its intrinsic size. Images lay out
  as replaced elements (inline by default, or block) using CSS, then attribute,
  then intrinsic dimensions, and paint as `DrawImage` commands. As of Milestone
  11 the desktop shell's `mocha_raster` resolves `DrawImage` onto the window
  surface.
- **Forms and basic controls** (`mocha_forms`): `<form>`, `<input>` (text,
  password, checkbox, radio, submit, reset, hidden), `<button>`, `<label>`,
  `<textarea>`, `<select>`/`<option>`. Dynamic value/checked/selected/disabled
  state lives in a `FormState` keyed by DOM node (attributes only initialize
  it); JavaScript reads and writes `value`/`checked`/`disabled`/`type`/`name`,
  `select.value`/`selectedIndex`, and `form.submit()` (recorded, never
  navigated). Controls lay out as inline replaced items with simple default
  sizes (CSS `width`/`height` override) and paint as `DrawControl` commands;
  `--dump-form-state` prints the control state. Click default actions —
  checkbox toggle, radio group selection, form reset, submit identification —
  honour `preventDefault` and `disabled`. GET submission builds a
  form-urlencoded query URL; **POST is a clear error**. Unsupported
  `input`/`button` types fail form processing clearly. **There is no real
  typing, focus, caret, or validation.**

## What does not work

Not implemented (see [docs/architecture/limitations.md](docs/architecture/limitations.md)
and [networking-and-navigation.md](docs/architecture/networking-and-navigation.md)):

- **`https://`** (no TLS — returns a clear error), authentication, proxies,
  HTTP/2-3, real HTTP cache semantics, charset decoding beyond UTF-8. **Cookies**
  are now supported as a minimal jar with `Set-Cookie`/`Cookie` HTTP integration
  and profile persistence (Milestone 15) — but not full RFC 6265bis, no
  third-party/partitioned-cookie policy, and `Secure` cookies need HTTPS.
- Subresource loading beyond external CSS and images: external `<script src>`,
  CSS `url(...)` resources, web fonts, and a `<base>` element are unsupported.
- **JavaScript**: a small custom subset, **not** ECMAScript-compliant. No live
  `NodeList`, MutationObserver, real event loop/microtasks, promises,
  async/await, modules, classes, full `this`/prototypes, ternary `?:`, `switch`,
  or `try`/`catch`. DOM bindings are a tiny hand-picked surface; there is no
  security model. Invalidation is coarse (no incremental relayout).
- **Image rendering**: `DrawImage` commands are rasterized to the display surface
  in desktop mode. Responsive images (`srcset`/`<picture>`), SVG, and animation
  are unsupported.
- Mature window input: desktop mode supports crude click routing and simple text
  entry/backspace for text-editable controls, but there is no mature focus,
  caret, text selection, IME, accessibility, or pointer/touch/wheel gesture
  handling. Hit testing does not account for z-index/transforms/scrolling/clipping.
- **DevTools (M19):** headless snapshots and structured log data exist through
  `mocha_devtools` and `mocha_shell --devtools-snapshot`, but there is no Chrome
  DevTools Protocol, remote debugger, breakpoint UI, live editing, heap view,
  profiler, or interactive inspector.
- **Tabs (M13) + a SQLite profile (M14) + cookies/web storage (M15):** history,
  bookmarks, settings, download metadata, persistent session snapshots, a cookie
  jar/store, and origin-keyed `localStorage`/`sessionStorage`, plus a private
  in-memory profile. But the interactive shell does not yet surface
  history/bookmarks UI, auto-restore sessions, or drive page loads through the
  cookie jar automatically; JS `localStorage` is not yet wired to the persistent
  store; and there are no passwords, encryption, tab drag/reorder, pinned tabs,
  tab groups, crash recovery, or multiprocess isolation.
- **Security foundation (M16):** policy objects and tests exist for origin
  checks, scheme/file decisions, mixed content, CSP, permissions, certificate
  errors, and capabilities. They are not a full security model, not OS sandboxing,
  and not site isolation; broad runtime enforcement is still future work.
- **Multi-process prototype (M17):** a renderer child process can render through
  typed IPC and recover from a test crash, but it is not sandboxed, not site
  isolation, not a network/GPU process split, and not the default desktop render
  path.
- **Sandbox prototype (M18):** capability restrictions and a prepared-document
  renderer path exist. OS-level sandboxing is not applied, external subresource
  brokering is incomplete, and the legacy direct-load renderer path remains
  explicitly unsandboxed.
- The real HTML5 parsing algorithm and real CSS error recovery.
- `!important`, media queries, pseudo-classes/elements, attribute selectors, the
  `>`/`+`/`~` combinators, `em`/`rem`/`%` units, `rgb()`/`calc()`/`var()`.
- Real font metrics (text width is **estimated** from character count), margin
  collapse, `text-align`, `white-space` modes, hyphenation; long words can
  overflow. Baseline/`vertical-align` for inline images (top-aligned). Inline
  backgrounds/borders are deferred (inline text color and font size are honored).
- **Forms beyond the basics**: no mature focus, caret, text selection, or IME;
  no validation or validation UI; no `POST` bodies or `multipart/form-data`; no
  file/date/color/range/number inputs; no `:checked`/`:disabled`/`:focus`
  pseudo-classes; no `<optgroup>`, `<fieldset>`, `<legend>`, or the `form`
  attribute; label clicks do not activate controls; controls are printed as
  `DrawControl` in terminal mode and debug-rasterized in desktop mode, not real
  native widgets.
- Fonts, canvas, accessibility.
- Flexbox/grid, floats, positioning.
- Security sandboxing.
- Production multi-process architecture and OS sandboxing.

`file://` and `http://` document loading are supported; `https://` is not
implemented. Unsupported tags/features, unsupported CSS, unsupported URL schemes,
non-HTML document content types, and unsupported subresources (e.g. `<script
src>`) return clear errors; they are not silently ignored. `<style>` and
`<script>` text is never painted.

## Build, test, and run

The toolchain is pinned to **stable** Rust via `rust-toolchain.toml` (with
`rustfmt` and `clippy`); `rustup` selects it automatically. CI runs the full gate
(fmt / clippy / build / test) on both Linux and Windows. The `image` crate
(PNG/JPEG, default features off) is used by `mocha_image` for image decoding. The
optional `gui` feature uses the `minifb` crate for a visible desktop window; without
it, only terminal output is available. The rest of the workspace is std-only. No
browser engine, webview, or JavaScript engine is used. Node.js is **not** used for
build, test, or runtime.

Build:

```bash
cargo build --all
```

Test:

```bash
cargo test --all
```

Format check:

```bash
cargo fmt --all --check
```

Clippy (warnings are errors):

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Run the examples:

```bash
cargo run -p mocha_shell -- examples/basic/index.html
cargo run -p mocha_shell -- examples/styled/index.html
cargo run -p mocha_shell -- examples/layout/article.html
cargo run -p mocha_shell -- examples/layout/inline-wrap.html
cargo run -p mocha_shell -- examples/layout/box-model.html
cargo run -p mocha_shell -- examples/js/dom-basic.html
cargo run -p mocha_shell -- examples/js/dom-style-mutation.html
cargo run -p mocha_shell -- examples/js/event-listener.html
cargo run -p mocha_shell -- examples/resources/external-css.html
cargo run -p mocha_shell -- examples/images/basic-image.html
cargo run -p mocha_shell -- examples/images/inline-image.html
cargo run -p mocha_shell -- examples/images/sized-image.html
cargo run -p mocha_shell -- examples/forms/basic-form.html
cargo run -p mocha_shell -- examples/forms/checkbox-radio.html
cargo run -p mocha_shell -- examples/forms/textarea-select.html
cargo run -p mocha_shell -- examples/forms/js-form-state.html
cargo run -p mocha_shell -- examples/forms/form-submit.html
```

Load over `file://` or `http://`, dump the layout tree or form state, or show
response headers:

```bash
cargo run -p mocha_shell -- "file://$(pwd)/examples/basic/index.html"
cargo run -p mocha_shell -- --dump-layout examples/layout/inline-wrap.html
cargo run -p mocha_shell -- --dump-form-state examples/forms/basic-form.html
cargo run -p mocha_shell -- --show-headers --no-cache http://127.0.0.1:8080/index.html
cargo run -p mocha_shell -- --hit-test 20,40 examples/layout/inline-wrap.html
cargo run -p mocha_shell -- --eval-js "let x = 1 + 2 * 3; x;"
```

`https://` is not implemented and exits with a clear error.

## Repository structure

```text
mocha-browser/
  crates/
    mocha_error/    shared error types
    mocha_url/      minimal URL / path parsing (no networking)
    mocha_dom/      arena-backed DOM tree
    mocha_html/     tokenizer + stack-based tree builder
    mocha_css/      CSS tokenizer, parser, and value model
    mocha_style/    selector matching, cascade, inheritance, computed style
    mocha_layout/   block + inline layout (geometry/block/inline/line/debug)
    mocha_paint/    display-list generation (colors, borders)
    mocha_net/      resource loading (file/http), redirects, content-type, cache
    mocha_nav/      navigation history (navigate/back/forward/reload) + default actions
    mocha_events/   internal DOM event model and dispatch
    mocha_forms/    form-control state, default actions, GET submission model
    mocha_js/       from-scratch JavaScript-subset interpreter
    mocha_js_dom/   bridge: JS host objects for window/document/DOM, events, timers, forms
    mocha_resources/ subresource discovery + loading (external CSS, images)
    mocha_image/    image format detection + PNG/JPEG decoding (uses the image crate)
    mocha_origin/   minimal (scheme, host, port) web origin model
    mocha_cookie/   Set-Cookie parsing + in-memory cookie jar/matching
    mocha_storage/  SQLite profile: history/bookmarks/settings/downloads/session/cookies/localStorage (uses rusqlite)
    mocha_security/ origin/security policy helpers + tiny CSP parser/evaluator
    mocha_sandbox/  capability-based renderer policy + sandbox status (prototype)
    mocha_ipc/      versioned typed browser<->renderer IPC protocol
    mocha_process/  multi-process prototype: renderer child + browser-side manager
    mocha_devtools/ deterministic headless inspector snapshots
    mocha_engine/   high-level document loading/rendering pipeline
    mocha_raster/   display list + images to pixel buffer rasterization
    mocha_desktop/  desktop shell, browser chrome, tabs, address bar, navigation controls
    mocha_shell/    CLI that wires the pipeline together
    mocha_compat/   compatibility test harness (manifest + normalization + runner)
    mocha_perf/     render performance baseline tool
  docs/architecture/  overview, pipeline, milestones, boundaries, limitations,
                      networking-and-navigation, events, javascript-interpreter,
                      dom-bindings, subresources, images-and-replaced-elements,
                      forms-and-controls
  examples/basic/     plain HTML example
  examples/styled/    HTML + CSS example
  examples/layout/    article / inline-wrap / box-model layout examples
  examples/js/        inline <script> DOM mutation / events / timer examples
  examples/resources/ external stylesheet example (+ style.css)
  examples/images/    <img> basic / inline / sized examples
  examples/forms/     form / checkbox-radio / textarea-select / js-state / submit examples
  examples/assets/    mocha-test.png (tiny PNG asset)
  tests/integration/  rendering + navigation + events + js-dom + subresource + image + forms pipelines
  tests/compat/       Compatibility Level 1 cases + manifests (run by mocha_compat)
  tests/corpus/       malformed HTML/CSS/JS/URL inputs for the crash corpus
  tests/crash/        crash-corpus test target (no-panic robustness checks)
  tests/visual/       raster-checksum visual regression cases + expected checksums
```

See [docs/architecture/crate-boundaries.md](docs/architecture/crate-boundaries.md) for
the responsibility of each crate.

## Milestone roadmap

The full roadmap lives in [docs/architecture/milestones.md](docs/architecture/milestones.md).

1. Engine laboratory (done)
2. Basic CSS engine (done)
3. Real layout foundation (done)
4. Networking and navigation (done)
5. DOM events (done)
6. Custom JavaScript interpreter (done)
7. JavaScript DOM bindings (done)
8. Subresource loading — external stylesheets (done)
9. Images and replaced elements (done)
10. Forms and basic input controls (done)
11. Desktop window shell — a real raster surface for the display list (done)
12. Browser chrome and desktop shell (done)
13. Tabs and in-memory session model (done)
14. Profile storage — SQLite-backed history/bookmarks/settings/downloads/session (done)
15. Cookies and origin-aware storage — cookie jar, persistence, localStorage/sessionStorage (done)
16. Origin model and security foundation (done)
17. Multi-process architecture prototype (done)
18. Security sandbox prototype (done)
19. DevTools foundation — headless snapshots (done)
20. Web compatibility hardening and standards test harness (done)

Post-Milestone-20 direction (not code yet): expand compatibility test coverage,
decide the HTTPS/TLS approach, improve JS/DOM and CSS/layout compatibility,
strengthen sandboxing and performance, add accessibility, and grow DevTools.

## Safety warning

Mocha Browser is an experiment. It is **not** secure, **not** standards
compliant, and **not** able to browse the modern web. Do not use it to open
untrusted content or as a general-purpose browser.

## License

MIT. See [LICENSE](LICENSE).
