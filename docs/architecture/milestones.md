# Milestone Roadmap

Mocha Browser is built one milestone at a time. **Milestones 1–5 are implemented
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

## Milestone 6: Custom JavaScript interpreter — next

- **Goal:** a from-scratch JS lexer, parser, AST, and tree-walking interpreter
  with values, functions, and closures. No third-party JS engine.
- **Not included:** DOM bindings, JIT, full ECMAScript coverage.
- **Verification:** interpreter tests over a language subset (arithmetic,
  control flow, functions, closures).

## Milestone 7: JavaScript DOM bindings

- **Goal:** expose `window`, `document`, `querySelector`, events, timers, and
  DOM mutation to the interpreter.
- **Not included:** full Web IDL surface.
- **Verification:** tests where scripts query and mutate the DOM and receive
  events.

## Milestone 8: Multi-process architecture

- **Goal:** split into a browser process and renderer process(es) with typed IPC
  and crash recovery.
- **Not included:** GPU process split, full sandbox (Milestone 10).
- **Verification:** tests for IPC round-trips and renderer crash isolation.

## Milestone 9: Storage and profile system

- **Goal:** history, bookmarks, settings, session restore, and a private
  profile.
- **Not included:** cloud sync, accounts.
- **Verification:** tests for persistence and session restore.

## Milestone 10: Security foundation

- **Goal:** an origin model with same-origin checks, mixed-content handling, and
  a permissions model.
- **Not included:** a complete sandbox hardening pass.
- **Verification:** tests for origin comparisons and same-origin enforcement.

## Milestone 11: Desktop product shell

- **Goal:** tabs, an address bar, navigation buttons, and browser chrome.
- **Not included:** extensions, devtools.
- **Verification:** UI/integration tests for tab and navigation behaviour.

## Milestone 12: Web compatibility hardening

- **Goal:** run standards test suites, fuzzing, and visual regression; improve
  compatibility.
- **Not included:** a promise of full web compatibility.
- **Verification:** tracked pass rates on chosen test suites plus a fuzzing
  harness in CI.
