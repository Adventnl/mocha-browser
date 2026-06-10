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

**Milestone 3: Real Layout Foundation.** Mocha loads a single local HTML file,
extracts and parses its CSS (`<style>` blocks and inline `style` attributes),
computes styles via selector matching, cascade, and inheritance, and lays the
styled tree out with real block and inline formatting — line boxes, word
wrapping, and anonymous block boxes for mixed content — before painting a colored
display list to the terminal. Text measurement is still estimated, not real font
metrics. No window is opened, and no external CSS or networking is involved. See
[rendering-pipeline.md](rendering-pipeline.md) for the stages and
[limitations.md](limitations.md) for what is intentionally absent.

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
