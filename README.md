# Mocha Browser

Mocha Browser is an experimental from-scratch browser engine and desktop browser.

Mocha is not based on Chromium, WebKit, Gecko, Servo, Electron, CEF, Tauri WebView, system WebView, V8, SpiderMonkey, JavaScriptCore, QuickJS, Deno, or Node.js.

Current status: Milestone 5 (DOM Events) implemented.

Mocha is not safe for general web browsing yet.

## Project goals

Build a real, understandable browser engine from first principles, one small
milestone at a time. The long-term architecture is inspired by modern
multi-process browsers, but the current implementation is intentionally tiny.
Correctness and honesty are valued over breadth: unsupported behaviour fails
with a clear error rather than being faked.

## Current milestone

**Milestone 4: Networking and Navigation.** Load a document from a local path,
`file://`, or `http://` URL through a resource loader and navigation-history
layer, then run the rendering engine and print a display list to the terminal:

```text
input URL/path -> mocha_url -> mocha_nav/mocha_net (load: file/http, redirects,
content-type, cache) -> HTML tokenizer/tree builder -> DOM -> <style> extraction
-> CSS parser -> computed style -> block & inline layout -> display list -> terminal
```

## What works

- Parsing a small, well-formed subset of HTML (`html`, `body`, `h1`, `h2`, `p`,
  `div`, `span`, `style`, plus doctype and comments).
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
  there is no JavaScript and no real window input.

## What does not work

Not implemented (see [docs/architecture/limitations.md](docs/architecture/limitations.md)
and [networking-and-navigation.md](docs/architecture/networking-and-navigation.md)):

- **`https://`** (no TLS — returns a clear error), cookies, authentication,
  proxies, HTTP/2-3, real HTTP cache semantics, charset decoding beyond UTF-8.
- Subresource loading: external / linked CSS (`<link>` is rejected), images,
  scripts, fonts.
- **JavaScript** (event listeners are Rust-only), real window/OS input or event
  loop, pointer/touch/wheel/focus events; hit testing ignores
  z-index/transforms/scrolling/clipping.
- The real HTML5 parsing algorithm and real CSS error recovery.
- `!important`, media queries, pseudo-classes/elements, attribute selectors, the
  `>`/`+`/`~` combinators, `em`/`rem`/`%` units, `rgb()`/`calc()`/`var()`.
- Real font metrics (text width is **estimated** from character count), margin
  collapse, `text-align`, `white-space` modes, hyphenation; long words can
  overflow. Inline backgrounds/borders are deferred (inline text color and font
  size are honored).
- Forms, images, fonts, canvas, SVG, accessibility.
- Flexbox/grid, floats, positioning.
- Security sandboxing, multi-process architecture, tabs, and desktop windowing.

Unsupported tags, unsupported CSS, and non-`file` URLs return clear errors; they
are not silently ignored. `<style>` CSS text is never painted.

## Build, test, and run

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
```

Load over `file://` or `http://`, dump the layout tree, or show response headers:

```bash
cargo run -p mocha_shell -- "file://$(pwd)/examples/basic/index.html"
cargo run -p mocha_shell -- --dump-layout examples/layout/inline-wrap.html
cargo run -p mocha_shell -- --show-headers --no-cache http://127.0.0.1:8080/index.html
cargo run -p mocha_shell -- --hit-test 20,40 examples/layout/inline-wrap.html
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
    mocha_shell/    CLI that wires the pipeline together
  docs/architecture/  overview, pipeline, milestones, boundaries, limitations,
                      networking-and-navigation, events
  examples/basic/     plain HTML example
  examples/styled/    HTML + CSS example
  examples/layout/    article / inline-wrap / box-model layout examples
  tests/integration/  rendering + navigation pipeline tests
  tests/visual/       future render targets (no image comparison yet)
```

See [docs/architecture/crate-boundaries.md](docs/architecture/crate-boundaries.md) for
the responsibility of each crate.

## Milestone roadmap

The full roadmap lives in [docs/architecture/milestones.md](docs/architecture/milestones.md).

1. Engine laboratory (done)
2. Basic CSS engine (done)
3. Real layout foundation (done)
4. Networking and navigation (done)
5. DOM events (current)
6. Custom JavaScript interpreter
7. JavaScript DOM bindings
8. Multi-process architecture
9. Storage and profile system
10. Security foundation
11. Desktop product shell
12. Web compatibility hardening

## Safety warning

Mocha Browser is an experiment. It is **not** secure, **not** standards
compliant, and **not** able to browse the modern web. Do not use it to open
untrusted content or as a general-purpose browser.

## License

MIT. See [LICENSE](LICENSE).
