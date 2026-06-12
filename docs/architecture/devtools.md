# DevTools Foundation (Milestone 19)

Mocha's DevTools foundation is a headless inspection layer for the engine's
current internal products. It is intentionally small and deterministic: tests
and scripts can ask for a snapshot of a rendered page and receive stable text
containing the DOM, computed styles, layout tree, display list, and logs.

This is **not** Chrome DevTools, not the Chrome DevTools Protocol (CDP), and not
a remote debugging server. There are no breakpoints, live DOM editing,
JavaScript stepping, heap snapshots, performance profiles, protocol clients, or
browser UI panels yet.

## Crate

`mocha_devtools` owns the snapshot data model and formatter:

- `DevToolsSnapshot`: one point-in-time inspection artifact.
- `DomSnapshot`: arena node ids, node kinds, tag names, attributes, text, and
  children.
- `StyleSnapshot`: a computed-style tree built from the final document and
  stylesheets.
- `LayoutSnapshot`: layout boxes with kind, optional source node id, geometry,
  and children.
- `DisplayListSnapshot`: paint commands in paint order, preserving the existing
  debug line for shell parity.
- `NetworkLogEntry`: document-load metadata currently exposed by
  `mocha_engine::RenderedPage` (`request_url`, final URL, status,
  content-type, and cache flag).
- `ConsoleLogEntry`: captured `console.log` output from inline scripts.
- `EventLogEntry`, `StorageSnapshot`, `SecurityLogEntry`, `IpcLogEntry`, and
  `ProcessLogEntry`: structured log records that embedders can populate as
  runtime hooks grow.

`snapshot_rendered_page` takes an existing `mocha_engine::RenderedPage`, so the
DevTools foundation observes the same products the shell and desktop frontends
already use. It does not load documents independently and does not fork a second
rendering pipeline.

## Shell command

`mocha_shell --devtools-snapshot <path-or-url>` renders through the normal engine
and prints the deterministic snapshot text. It can be combined with existing
loading flags such as `--no-cache` and `--show-headers`.

Example:

```bash
cargo run -p mocha_shell -- --devtools-snapshot examples/js/dom-basic.html
```

The output is intended for tests, debugging, and future UI panels rather than
wire compatibility with browser tooling.

## Current limits

- Network logging currently records the top-level document response only.
  External CSS and image subresource log hooks are future work.
- Console logging captures `console.log` output from the custom JavaScript
  runtime; richer log levels and stack traces are future work.
- Event, storage, security, IPC, and process logs have stable data structures,
  but most runtime call sites do not append to them yet.
- The formatter is deterministic text, not JSON and not CDP.
- There is no live connection to a running renderer, no protocol transport, and
  no interactive inspector UI.

## Verification

Milestone 19 is covered by:

- `cargo test -p mocha_devtools`
- `cargo test -p mocha_shell devtools_snapshot_includes_inspector_sections`
- the full workspace gates (`cargo fmt --all --check`, clippy, build, tests)
- a shell smoke test:
  `cargo run -p mocha_shell -- --devtools-snapshot examples/js/dom-basic.html`
