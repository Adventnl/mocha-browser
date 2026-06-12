# Compatibility Testing

Milestone 20 added `mocha_compat`, a small local harness that holds the engine to
the [Compatibility Level 1](compatibility-level-1.md) subset. This document
explains how it works, how to add tests, and — importantly — what it does **not**
prove.

## What it is (and is not)

`mocha_compat` renders local HTML cases through `mocha_engine` and compares a
normalized snapshot of the result against an expectation. It reports
`pass` / `fail` / `unsupported` / `skip` / `xfail` counts and exits non-zero on
any *unexpected* failure.

It is **not** web-platform-tests, **not** a Chromium-level conformance suite, and
proves nothing about real-world web pages. It proves only that the documented
Level 1 subset keeps working and that unsupported features keep failing clearly.

## Running

```bash
# full suite
cargo run -p mocha_compat -- tests/compat/manifest.toml

# stable CI subset
cargo run -p mocha_compat -- tests/compat/ci-manifest.toml

# regenerate blessed `expect` snapshot files, then review the diff
MOCHA_BLESS=1 cargo run -p mocha_compat -- tests/compat/manifest.toml
```

Exit code: `0` when there are no unexpected failures, `1` otherwise.

## Manifest format

A hand-parsed minimal TOML subset (no `serde`/`toml` dependency, in keeping with
the rest of the project): a sequence of `[[test]]` tables whose values are quoted
strings or numbers.

| key | meaning |
| --- | --- |
| `name` | unique test name (required) |
| `path` | HTML file relative to the manifest, or an absolute URL like `https://…` (required) |
| `category` | grouping label, e.g. `html`, `css`, `js` |
| `mode` | snapshot to compare: `display` (default), `layout`, `devtools`, `form-state` |
| `status` | `pass` (default), `unsupported`, `xfail`, `skip` |
| `reason` | required for `skip`/`xfail` |
| `expect` | path (relative to the manifest) of a blessed expected-snapshot file |
| `expect_contains` | substring the normalized snapshot must contain |
| `expect_error_contains` | substring the render's error message must contain |
| `viewport_width` | optional viewport width override (CSS px) |

A non-`skip` test must declare exactly one of `expect` / `expect_contains` /
`expect_error_contains`.

## Statuses

- **pass** — the expectation must be met; otherwise the test fails.
- **unsupported** — a *documented* unsupported feature; the render is expected to
  error (use `expect_error_contains`). It is only acceptable when the feature is
  listed as unsupported in [compatibility-level-1.md](compatibility-level-1.md).
- **xfail** — known-broken: the expectation is expected *not* to be met. A `reason`
  is required. If it unexpectedly passes ("xpass"), that is a failure, so the
  marker gets cleaned up.
- **skip** — not run; records a `reason` (and usually a TODO). Used where a
  behaviour needs infrastructure the static file harness lacks (e.g. an http(s)
  origin for cookies/storage, or input-event dispatch). Those behaviours are
  covered by unit/integration tests elsewhere.

New unexpected failures fail CI. Adding an `unsupported`/`xfail`/`skip` without a
documented reason is not allowed.

## Snapshot normalization

Snapshots are normalized before comparison so they are stable across platforms and
runs (`mocha_compat::normalize_snapshot`):

- Windows path separators (`\`) become `/`.
- Decimal numbers are rounded to 2 places (absorbs tiny float jitter).
- ISO-8601 timestamps and `<n>ms` durations become `<TIME>`.

DOM node ids are left as-is: the arena numbers nodes deterministically in document
order, so they are already stable. Because cases are addressed by **relative**
paths (cwd = workspace root), snapshots contain only `file://tests/compat/…` URLs
— no absolute prefixes leak. `mocha_compat::strip_prefixes` is available for the
absolute-path case.

Blessed `expect` files are written already-normalized, so a re-bless and a verify
compare like for like.

## Adding a test

1. Drop an HTML file under `tests/compat/<category>/`.
2. Add a `[[test]]` block to `tests/compat/manifest.toml` (and, if it belongs in
   the fast core set, `ci-manifest.toml`).
3. Prefer `expect_contains` / `expect_error_contains` for robustness; use a blessed
   `expect` file when you want to pin a whole snapshot.
4. Run the harness; for a blessed test, run once with `MOCHA_BLESS=1`, then review
   the generated `.expect.txt`.

## How CI uses it

CI runs the stable core subset on every push:

```bash
cargo run -p mocha_compat -- tests/compat/ci-manifest.toml
```

The full manifest is broader and runs locally. The harness library's own unit
tests (manifest parsing, normalization, classification) run as part of
`cargo test --all`.

## What it does not prove

It does not prove modern-web compatibility, correctness on real sites, security,
performance, or completeness of any subsystem. It is a guardrail around a small,
documented, honest subset — nothing more.
