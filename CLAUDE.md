# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

Tradeoff: These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

Don't assume. Don't hide confusion. Surface tradeoffs.

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them; don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

Minimum code that solves the problem. Nothing speculative.

- No features beyond what was asked.
- No abstractions for single-use code.
- No flexibility or configurability that was not requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: Would a senior engineer say this is overcomplicated? If yes, simplify.

## 3. Surgical Changes

Touch only what you must. Clean up only your own mess.

When editing existing code:
- Don't improve adjacent code, comments, or formatting.
- Don't refactor things that are not broken.
- Match existing style, even if you would do it differently.
- If you notice unrelated dead code, mention it; do not delete it.

When your changes create orphans:
- Remove imports, variables, and functions that your changes made unused.
- Do not remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

Define success criteria. Loop until verified.

Transform tasks into verifiable goals:
- Add validation → write tests for invalid inputs, then make them pass.
- Fix the bug → write a test that reproduces it, then make it pass.
- Refactor X → ensure tests pass before and after.

For multi-step tasks, state a brief plan:

1. Step → verify: check
2. Step → verify: check
3. Step → verify: check

Strong success criteria let you loop independently. Weak criteria require clarification.

## Mocha Browser Project Rules

Mocha Browser is a from-scratch experimental browser engine.

Do not use Chromium, WebKit, Gecko, Servo, Electron, CEF, Tauri WebView, system WebView, V8, SpiderMonkey, JavaScriptCore, QuickJS, Deno, or Node.js.

Do not fake browser behavior.

Unsupported features must return clear errors.

Every crate must have tests.

Every milestone must produce a runnable result.

Milestones 1–21 are implemented (see docs/architecture/milestones.md for the authoritative list): engine pipeline (1–3), networking/navigation (4), events (5), a from-scratch JS interpreter + DOM bindings (6–7), subresources/images/forms (8–10), the minifb desktop window with chrome, tabs, and wheel scrolling (11–13), SQLite profile storage (14), cookies/web storage (15), security policy objects (16), IPC/multi-process and sandbox prototypes (17–18), DevTools snapshots (19), the compat/perf harness (20), and real networking (21). Recent addition:
- Milestone 21 (real networking): the hand-written HTTP/1.1 client in `mocha_net` decodes `Transfer-Encoding: chunked` and `Content-Encoding: gzip` (via `mocha_gzip`, a from-scratch RFC 1951/1952 inflate + gzip decoder with CRC-32 checks and a 64 MiB zip-bomb cap), validates `Content-Length` (truncation is a clear error), sends `Accept-Encoding: gzip`, and loads `https://` over **rustls** (ring provider) with certificates verified against the embedded `webpki-roots` Mozilla store — no certificate-error override exists. Redirects follow across http↔https. `test_server::TestServer::start_tls` serves TLS with a committed self-signed localhost cert (testdata/); tests trust it explicitly via `DefaultLoader::with_extra_tls_root`. TLS is the deliberate library exception (like `image`/`rusqlite`/`minifb`): the TLS protocol is never hand-rolled, the HTTP protocol on top of it always is. Building `rusqlite`/`ring` needs a C compiler (`gcc` on the windows-gnu toolchain).

The next milestone is Milestone 22: text quality — real font metrics and glyph rasterization in the desktop shell (system font loading, a real text-measurement path replacing the estimated/fixed-advance text, font-weight/style matching, then line breaking improvements). Still out of scope and must return clear errors: keep-alive/HTTP/2-3, `br`/`zstd`/`deflate` encodings, certificate-error overrides/revocation/HSTS, external `<script src>`, CSS `url(...)`, web fonts, promises/async/await/modules/classes, a real event loop, incremental relayout, flexbox/grid/floats/positioning, responsive images/SVG/canvas, form validation, POST form submission. Do not add an existing JS engine/parser or browser engine.

## Verification commands

Before declaring any task complete, run and pass:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
