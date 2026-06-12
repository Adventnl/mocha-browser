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

**Milestone 19: DevTools foundation.** Milestones 1–10 built a
complete document loading and rendering pipeline (HTML, CSS, layout, JavaScript
bindings, forms). **Milestone 11** introduced the desktop shell and software
rasterizer, **Milestone 12** added minimal browser chrome, **Milestone 13** made
the shell multi-tab, **Milestone 14** added the SQLite-backed profile foundation,
and **Milestone 15** added `mocha_origin`, `mocha_cookie`, cookie/localStorage
persistence tables, optional cookie-aware HTTP loading, and JS bindings.
**Milestone 16** adds `mocha_security`, a policy layer for same-origin checks,
scheme/file restrictions, mixed-content awareness, CSP policy evaluation,
permissions, certificate error data, and future renderer capabilities.
**Milestone 17** adds `mocha_ipc`, `mocha_process`, and a `mocha_renderer` child
process prototype for typed IPC, renderer lifecycle, crash detection, and
respawn. **Milestone 18** adds `mocha_sandbox` and a capability-restricted
prepared-document path. **Milestone 19** adds `mocha_devtools` and a headless
snapshot/log foundation exposed through `mocha_shell --devtools-snapshot`. See
[tabs-and-session.md](tabs-and-session.md),
[profile-storage.md](profile-storage.md), and
[cookies-and-web-storage.md](cookies-and-web-storage.md), plus
[security-foundation.md](security-foundation.md),
[multiprocess-prototype.md](multiprocess-prototype.md), and
[security-sandbox.md](security-sandbox.md), plus [devtools.md](devtools.md).

The M16-M19 security, process, sandbox, and DevTools work is intentionally
foundational: it defines
and tests policy/process/sandbox objects, but it is not a production OS sandbox,
complete web security, site isolation, HTTPS/TLS, Chrome DevTools, or CDP. The
normal desktop/shell paths remain single-process by default. The default
page-loading path still does not automatically use the cookie jar, and JS
storage/cookies are per-render unless an embedder wires persistent state into a
runtime.

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
rasterization, browser chrome, tabs, persistent profiles, cookies,
origin-aware web storage, a security policy foundation, a multi-process
prototype with typed IPC, a sandbox prototype, and a DevTools snapshot
foundation (Milestones 1–19, implemented), then broader standards compliance.

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
