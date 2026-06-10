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

The current milestone is Milestone 9: images and replaced elements. Milestones 1–9 are implemented. Recent additions:
- Milestone 7 (JS DOM bindings): `mocha_js` gained a host-object mechanism (`JsValue::Host` + `HostObject`); the `mocha_js_dom` crate wires `window`/`document`/`console`, DOM read/mutate/query, JS event listeners, and a deterministic timer queue, and runs inline `<script>` in document order (coarse invalidation — style/layout/paint run once after scripts).
- Milestone 8 (subresources): the `mocha_resources` crate loads external `<link rel="stylesheet">` CSS against the document base URL (dot-segment normalization in `mocha_url`); document-order cascade.
- Milestone 9 (images): the `mocha_image` crate decodes PNG/JPEG via the `image` crate (the workspace's only third-party dependency); `<img>` lays out as a replaced element (inline/block) and paints `DrawImage`. Pixels are not rasterized.

The next milestone is Milestone 10: forms and basic input controls. Still out of scope and must return clear errors: HTTPS/TLS, external `<script src>`, CSS `url(...)`, promises/async/await/modules/classes, a real event loop, incremental relayout, image rasterization to a window, responsive images/SVG/canvas, and desktop window rendering. Do not add an existing JS engine/parser or browser engine.

## Verification commands

Before declaring any task complete, run and pass:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
