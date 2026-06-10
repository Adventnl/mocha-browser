# Mocha Browser

Mocha Browser is an experimental from-scratch browser engine and desktop browser.

Mocha is not based on Chromium, WebKit, Gecko, Servo, Electron, CEF, Tauri WebView, system WebView, V8, SpiderMonkey, JavaScriptCore, QuickJS, Deno, or Node.js.

Current status: Milestone 3 (Real Layout Foundation) implemented.

Mocha is not safe for general web browsing yet.

## Project goals

Build a real, understandable browser engine from first principles, one small
milestone at a time. The long-term architecture is inspired by modern
multi-process browsers, but the current implementation is intentionally tiny.
Correctness and honesty are valued over breadth: unsupported behaviour fails
with a clear error rather than being faked.

## Current milestone

**Milestone 3: Real Layout Foundation.** Load a local HTML file, extract and
parse its CSS, compute styles, lay it out with real block and inline formatting
(line boxes and word wrapping), and print a display list to the terminal:

```text
bytes -> HTML tokenizer/tree builder -> DOM -> <style> extraction
      -> CSS tokenizer/parser -> selector matching -> cascade -> inheritance
      -> computed style tree -> block & inline layout (line boxes, wrapping)
      -> display list -> terminal
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

## What does not work

Not implemented (see [docs/architecture/limitations.md](docs/architecture/limitations.md)):

- External / linked CSS (`<link>` is rejected with a clear error), networking,
  JavaScript.
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

Dump the layout tree instead of the display list:

```bash
cargo run -p mocha_shell -- --dump-layout examples/layout/inline-wrap.html
```

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
    mocha_shell/    CLI that wires the pipeline together
  docs/architecture/  overview, pipeline, milestones, boundaries, limitations
  examples/basic/     plain HTML example
  examples/styled/    HTML + CSS example
  examples/layout/    article / inline-wrap / box-model layout examples
  tests/integration/  end-to-end pipeline tests
  tests/visual/       future render targets (no image comparison yet)
```

See [docs/architecture/crate-boundaries.md](docs/architecture/crate-boundaries.md) for
the responsibility of each crate.

## Milestone roadmap

The full roadmap lives in [docs/architecture/milestones.md](docs/architecture/milestones.md).

1. Engine laboratory (done)
2. Basic CSS engine (done)
3. Real layout foundation (current)
4. Networking and navigation
5. DOM events
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
