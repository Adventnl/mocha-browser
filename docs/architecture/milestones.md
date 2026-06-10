# Milestone Roadmap

Mocha Browser is built one milestone at a time. **Milestones 1 and 2 are
implemented today**; everything after them is direction, not code. Each milestone
lists its goal, what is explicitly not included, and how completion is verified.

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

## Milestone 3: Real layout foundation

- **Goal:** a real box model with block and inline formatting contexts, text
  measurement, and line wrapping.
- **Not included:** flexbox/grid, JavaScript, networking.
- **Verification:** layout tests for wrapping, nested boxes, and margin/padding
  geometry.

## Milestone 4: Networking and navigation

- **Goal:** load `file`, `http`, and `https` resources; handle redirects,
  history, and reload.
- **Not included:** caching policy depth, service workers, JavaScript.
- **Verification:** tests against a local fixture server covering success,
  redirect, and error responses.

## Milestone 5: DOM events

- **Goal:** an `EventTarget` model with capture/bubble phases and click,
  keyboard, and mouse events.
- **Not included:** JavaScript bindings (Milestone 7).
- **Verification:** tests for dispatch order across capturing and bubbling.

## Milestone 6: Custom JavaScript interpreter

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
