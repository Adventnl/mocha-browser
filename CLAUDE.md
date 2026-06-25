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

Unsupported features must be reported, never faked. Since Milestone 23 the pipeline **fails open**: a stylesheet, script, or image it cannot process is *skipped* and recorded as a render diagnostic (surfaced in the desktop badge, the shell's stderr, and the DevTools snapshot) so the page still renders — this matches the HTML/CSS specs' forward-compatible parsing. Genuinely fatal conditions (network/transport failure, non-HTML content type, invalid UTF-8) still return a clear `MochaError`.

Every crate must have tests.

Every milestone must produce a runnable result.

Milestones 1–24 are implemented (see docs/architecture/milestones.md for the authoritative list): engine pipeline (1–3), networking/navigation (4), events (5), a from-scratch JS interpreter + DOM bindings (6–7), subresources/images/forms (8–10), the minifb desktop window with chrome, tabs, and wheel scrolling (11–13), SQLite profile storage (14), cookies/web storage (15), security policy objects (16), IPC/multi-process and sandbox prototypes (17–18), DevTools snapshots (19), the compat/perf harness (20), real networking (21), real proportional page font metrics (22), forgiving fail-open HTML parsing that renders real content pages (23), and the real-web CSS selector engine — combinators, attribute selectors, and structural pseudo-classes (24). Recent additions:
- Milestone 21 (real networking): the hand-written HTTP/1.1 client in `mocha_net` decodes `Transfer-Encoding: chunked` and `Content-Encoding: gzip` (via `mocha_gzip`, a from-scratch RFC 1951/1952 inflate + gzip decoder with CRC-32 checks and a 64 MiB zip-bomb cap), validates `Content-Length` (truncation is a clear error), sends `Accept-Encoding: gzip`, and loads `https://` over **rustls** (ring provider) with certificates verified against the embedded `webpki-roots` Mozilla store — no certificate-error override exists. Redirects follow across http↔https. `test_server::TestServer::start_tls` serves TLS with a committed self-signed localhost cert (testdata/); tests trust it explicitly via `DefaultLoader::with_extra_tls_root`. TLS is the deliberate library exception (like `image`/`rusqlite`/`minifb`): the TLS protocol is never hand-rolled, the HTTP protocol on top of it always is. Building `rusqlite`/`ring` needs a C compiler (`gcc` on the windows-gnu toolchain).
- Milestone 23 (forgiving parsing): `mocha_html` now recovers from real-world markup instead of rejecting it — **any** tag name is accepted (unknown tags style as block; head metadata is non-rendered), malformed/mismatched/unclosed markup recovers, a few implied end tags nest `<p>`/list items/table cells, the full void-element set is recognized, and HTML character references are decoded. `mocha_style` UA defaults cover the common content tags (headings `h1`–`h6` bold, inline text semantics, `ul`/`ol`/`li` with markers, sectioning elements, `blockquote`/`pre`). `mocha_engine` **fails open**: a stylesheet, script, or image it cannot process is skipped and added to `RenderedPage.diagnostics` (the desktop draws an "N features not supported" badge; the shell prints them to stderr) rather than aborting. CSS recovery is still **coarse** — an unsupported stylesheet is skipped *wholesale* (per-declaration recovery is M24).

- Milestone 24 (real-web CSS selectors): the selector engine grows from the type/class/id/universal/descendant subset to the grammar real pages rely on. `mocha_css` now parses the child (`>`), next-sibling (`+`), and subsequent-sibling (`~`) **combinators**; **attribute selectors** (`[a]`, `[a=v]`, `[a~=v]`, `[a|=v]`, `[a^=v]`, `[a$=v]`, `[a*=v]`, quoted values); and **structural pseudo-classes** (`:root`, `:empty`, `:first/last/only-child`, `:first/last/only-of-type`, `:nth-child(an+b)` incl. `odd`/`even`, `:nth-last-child`, `:nth-of-type`, `:nth-last-of-type`, `:not(<compound>)`), with correct specificity. `mocha_style`'s matcher is **redesigned** to navigate the DOM by `NodeId` so it can resolve combinators and sibling/structural state. Dynamic pseudo-classes (`:hover`, `:focus`, …) and pseudo-elements (`::before`, …) parse but are **inert** — the rule is retained, never matched, so state is never faked. Forgiving parsing is unchanged: an unknown pseudo-class or malformed selector is still skipped per-item and recorded as a diagnostic.

The next milestone is broader CSS values (Milestone 25 direction): `font-family` into the M22 font matcher, more shorthands, and skipping `@media`/`@font-face`/`@keyframes` value blocks cleanly while keeping the rest of the sheet (per-declaration recovery and `rgb()/hsl()`, `%`/`em`/`rem`, `line-height`, `text-align` are already in place). Still unsupported — at page level these are **skipped with a diagnostic** (not faked, not page-fatal); transport-level failures still error clearly: keep-alive/HTTP/2-3, `br`/`zstd`/`deflate` encodings, certificate-error overrides/revocation/HSTS, external `<script src>`, CSS `url(...)`, web fonts, promises/async/await/modules/classes, a real event loop, incremental relayout, `:is()`/`:where()`/`:has()`, generated content for `::before`/`::after`, live `:hover`/`:focus` tracking, grid/floats/positioning, responsive images/SVG/canvas, form validation, POST form submission. Do not add an existing JS engine/parser or browser engine.

## Verification commands

Before declaring any task complete, run and pass:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
