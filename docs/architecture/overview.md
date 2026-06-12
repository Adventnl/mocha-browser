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

**Milestone 13: Tabs and in-memory session model.** Milestones 1–10 built a
complete document loading and rendering pipeline (HTML, CSS, layout, JavaScript
bindings, forms). **Milestone 11** introduced the desktop shell and a software
rasterizer that draws display lists to a pixel buffer and displays them in a
window via `mocha_desktop`. **Milestone 12** added minimal browser chrome: an
address bar, back/forward/reload buttons, and window event routing. **Milestone
13** makes the shell multi-tab: a `TabManager` owns the tabs and the active-tab
invariant, each tab keeps its own page/history/scroll/focus, a tab strip sits
above the toolbar, a simple internal new-tab page renders offline, and an
in-memory `SessionSnapshot` captures tab metadata for restore (not persisted —
that is M14). See [tabs-and-session.md](tabs-and-session.md).

The pipeline still loads documents, executes JavaScript, resolves subresources,
and generates display lists exactly as before (Milestones 1–10). The display list
is now rasterized by `mocha_raster` and drawn by `mocha_desktop`'s window. The
shell still exists for terminal/headless output mode.

The engine uses **no existing browser or JavaScript engine**. See
[dom-bindings.md](dom-bindings.md), [subresources.md](subresources.md),
[images-and-replaced-elements.md](images-and-replaced-elements.md),
[forms-and-controls.md](forms-and-controls.md),
[rendering-pipeline.md](rendering-pipeline.md), [desktop-shell.md](desktop-shell.md),
[rasterization.md](rasterization.md), and
[limitations.md](limitations.md) for detail and what is intentionally absent.

## Long-term architecture direction

Over the roadmap (see [milestones.md](milestones.md)), Mocha has grown and is
intended to continue growing toward the shape of a modern browser: a CSS engine,
a real layout/box model, a networking and navigation layer, a DOM event system,
a custom JavaScript interpreter with DOM bindings, a desktop shell with
rasterization, browser chrome, and tabs (Milestones 11–13, implemented), then
persistent storage and profiles, cookies and origin-aware web storage, a
multi-process (browser/renderer) architecture with typed IPC, a security/origin
model, and finally broader standards compliance. **Milestone 14 (profile
storage) is next.**

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
