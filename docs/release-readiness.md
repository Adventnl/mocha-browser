# Release Readiness (post-Milestone 20)

Mocha Browser is a **functioning experimental browser**: it has its own engine,
tabs, storage, cookies, a JS/DOM subset, forms/images/subresources, a security
policy foundation, sandbox/process prototypes, headless DevTools snapshots, and —
as of Milestone 20 — a compatibility test harness, crash corpus, visual
regression, and performance baseline.

It is **not** a production browser, **not** Chromium-compatible, **not** secure
for general browsing, and does **not** support HTTPS. Do not use it to browse the
real web.

## Status table

| Subsystem | Status | Notes |
| --- | --- | --- |
| HTML | Basic subset | Not HTML5-complete; no `<head>`/`<title>`, no tag-soup recovery |
| CSS | Basic subset | No flex/grid/positioning/float/media queries/`var()`/`calc()` |
| Layout | Basic block/inline | No tables/flex/grid; fixed-advance debug font |
| JS | Tiny custom interpreter | Not ECMAScript-compliant; no classes/promises/modules/try-catch |
| DOM | Basic bindings | Not the full Web API surface; no real event loop |
| Network | HTTP(S)/file | HTTP/1.1 + chunked + gzip; HTTPS via rustls; no HTTP/2-3, keep-alive, POST |
| Storage | Profile/cookies/localStorage foundations | Needs an http(s) origin; minimal |
| Security | Policy/sandbox/process prototypes | Not production-secure; not site isolation; CSP not enforced in-pipeline |
| Desktop | Basic browser UI + tabs | Experimental |
| DevTools | Headless snapshots | Not Chrome DevTools / CDP |
| Compatibility | Local harness (Level 1) | Experimental subset; not web-platform-tests |

## What works

See [architecture/compatibility-level-1.md](architecture/compatibility-level-1.md)
for the precise supported HTML/CSS/layout/JS/DOM subset. In short: small,
well-formed HTML; a CSS cascade with basic selectors and properties; block +
inline layout with word wrapping; PNG/JPEG images and form controls as replaced
boxes; inline scripts over a tiny JS/DOM subset; a desktop shell with chrome,
tabs, and sessions; profile/cookie/storage foundations; and headless DevTools
snapshots.

## What does not work

Full HTML5, flexbox/grid/positioning, modern JavaScript (classes, modules,
promises, async/await, try/catch, RegExp, Date), the broad Web API surface,
HTTPS/TLS, external `<script src>`, CSS `url(...)`, canvas/SVG/media, a real event
loop, accessibility, production security/sandboxing, and CSP enforcement in the
render pipeline. Unsupported features fail with a clear error rather than being
faked.

## How to run

```bash
# terminal shell (display list / layout / form state / devtools snapshot)
cargo run -p mocha_shell -- examples/basic/index.html
cargo run -p mocha_shell -- --dump-layout examples/layout/article.html
cargo run -p mocha_shell -- --dump-form-state examples/forms/basic-form.html
cargo run -p mocha_shell -- --devtools-snapshot examples/js/dom-basic.html
cargo run -p mocha_shell -- --eval-js "let x = 1 + 2 * 3; x;"

# desktop (headless display-list dump; or a real window behind the `gui` feature)
cargo run -p mocha_desktop -- --dump-display-list examples/basic/index.html
cargo run -p mocha_desktop --features gui -- examples/basic/index.html

# https loads over TLS (Milestone 21)
cargo run -p mocha_shell -- https://example.com/
```

## How to test

```bash
# the full gate
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all
cargo test --all

# compatibility harness (full suite and CI subset)
cargo run -p mocha_compat -- tests/compat/manifest.toml
cargo run -p mocha_compat -- tests/compat/ci-manifest.toml

# crash corpus (malformed HTML/CSS/JS/URL never panic)
cargo test -p mocha_compat --test crash_corpus

# visual raster-checksum regression
cargo test -p mocha_compat --test visual_regression

# render performance baseline (informational; not a gate)
cargo run -p mocha_perf -- examples/layout/article.html
```

To regenerate blessed snapshots/checksums after an intended change, prefix with
`MOCHA_BLESS=1` and review the diff before committing.

## Known limitations and warnings

- **Not Chromium / not modern-web compatible.** The compatibility harness tests
  the documented Level 1 subset only; it is not web-platform-tests.
- **Not secure.** The origin/CSP/sandbox/process pieces are prototypes and
  foundations, not a production security model or site isolation.
- **No HTTPS/TLS.** Only `file://` and `http://` load.
- **Performance numbers are baselines**, not targets, and are not asserted in CI.
- **Visual checksums** assume deterministic f32 layout and the internal debug
  font; regenerate with `MOCHA_BLESS=1` after an intended rendering change.

## Post-Milestone-20 roadmap

Expand compatibility coverage (including a server-backed mode for
cookies/storage/CSP), decide the HTTPS/TLS approach, improve JS/DOM and
CSS/layout, strengthen sandboxing and performance, add accessibility, and grow
DevTools — each as a separate, honest milestone. Mocha is **not** a finished
browser.
