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

The current milestone is Milestone 6: a from-scratch JavaScript-subset interpreter (`mocha_js`) — lexer, parser, AST, tree-walking evaluator with values/scopes/closures/objects/arrays, small built-ins (`console.log`, `Math`), and an execution step limit. It evaluates standalone snippets (shell `--eval-js`) and uses no existing JS engine or parser. It is NOT wired to the DOM, `window`, `document`, events, or `<script>` tags.

Do not add DOM bindings, `<script>` execution, timers, promises, async/await, modules, classes, prototypes/full `this`, an existing JS engine/parser, real window input, HTTPS/TLS, or desktop window rendering during Milestone 6. DOM↔JS bindings (and the DOM mutation APIs they require) are Milestone 7.

## Verification commands

Before declaring any task complete, run and pass:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
