# Architecture Overview

## What Mocha Browser is

Mocha Browser is a from-scratch, experimental browser engine and (eventually) a
desktop browser. It is written in Rust as a Cargo workspace of small,
single-responsibility crates. The goal is to build a browser engine that a
person can actually read and understand, growing it one verifiable milestone at
a time.

## What Mocha Browser is not

Mocha is **not** built on, forked from, or embedding any existing engine:

- Not Chromium, WebKit, Gecko, or Servo.
- Not Electron, CEF, Tauri WebView, or any system WebView.
- Not V8, SpiderMonkey, JavaScriptCore, QuickJS, Deno, or Node.js.

General-purpose libraries (testing, logging, image/font/TLS/graphics libraries
in later milestones) may be used, but never an existing browser engine or
JavaScript engine.

Mocha is also not secure, not standards compliant, and not capable of browsing
the modern web. It must never claim otherwise.

## Current milestone

**Milestone 10: Forms and basic input controls.** The pipeline loads a document,
runs its inline `<script>`s against the DOM, loads its external stylesheets and
images, and renders text, images, and form controls to a display list. Building
on the from-scratch interpreter (`mocha_js`, Milestone 6):

- **Milestone 7 — JavaScript DOM bindings** (`mocha_js_dom`): a real host-object
  mechanism wires the interpreter to the DOM. Inline `<script>` runs in document
  order and can use `window`/`document`/`console`, query and mutate the DOM,
  register event listeners, and schedule deterministic timers. DOM mutations are
  reflected in a single post-script style/layout/paint pass (coarse invalidation).
- **Milestone 8 — Subresource loading** (`mocha_resources`): external
  `<link rel="stylesheet">` CSS is resolved against the document base URL, loaded,
  content-type validated, and folded into the document-order cascade.
- **Milestone 9 — Images** (`mocha_image`, the workspace's only third-party
  dependency): `<img>` is parsed, loaded, and decoded (PNG/JPEG) for its intrinsic
  size, laid out as a replaced element (inline or block), and painted as a
  `DrawImage` command. **Pixels are not rasterized to a window.**
- **Milestone 10 — Forms** (`mocha_forms`): form controls carry dynamic
  value/checked/selected/disabled state outside the DOM, exposed to JavaScript;
  controls lay out as inline replaced items and paint as `DrawControl` commands;
  programmatic clicks toggle checkboxes, select radio groups, and identify
  submissions (honouring `preventDefault`/`disabled`); GET submission builds a
  form-urlencoded query URL, and **POST is a clear error**. There is still no
  interactive window — no real typing, focus, or caret.

It uses **no existing browser or JavaScript engine**. See
[dom-bindings.md](dom-bindings.md), [subresources.md](subresources.md),
[images-and-replaced-elements.md](images-and-replaced-elements.md),
[forms-and-controls.md](forms-and-controls.md),
[rendering-pipeline.md](rendering-pipeline.md), and
[limitations.md](limitations.md) for detail and what is intentionally absent.

## Long-term architecture direction

Over the roadmap (see [milestones.md](milestones.md)), Mocha is intended to grow
toward the shape of a modern browser: a CSS engine, a real layout/box model, a
networking and navigation layer, a DOM event system, a custom JavaScript
interpreter with DOM bindings, a multi-process (browser/renderer) architecture
with typed IPC, storage and profiles, a security/origin model, and finally a
desktop product shell with tabs and chrome. None of that exists yet, and it is
documented as direction only — not implemented.

## Why the project starts terminal-first

Opening a GPU-backed window adds a large amount of incidental complexity
(windowing, event loops, compositing) that is unrelated to the core engine. By
emitting a display list and printing it to the terminal, Milestone 1 keeps the
focus on the actual engine pipeline — tokenizing, tree building, layout, and
display-list generation — while remaining fully testable from the command line.
A real compositor consumes an equivalent display list in a later milestone.

## Why faking browser behavior is forbidden

A browser that silently pretends to support features it does not have is worse
than one that fails honestly: it hides bugs, misleads users, and accumulates
fictional capability that later milestones must untangle. Mocha's rule is that
unsupported behaviour returns a clear error (for example
`UnsupportedFeature` or `NotImplemented`) instead of a plausible-looking but
fake result. This keeps the engine's real capabilities legible at every step.
