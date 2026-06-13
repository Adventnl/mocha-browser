//! Mocha Browser's local compatibility test harness.
//!
//! The harness loads a small manifest of HTML cases, renders each one through
//! [`mocha_engine`], and compares a **normalized** snapshot of the result against
//! an expectation. It reports `pass` / `fail` / `unsupported` / `skip` / `xfail`
//! counts and exits non-zero on any *unexpected* failure.
//!
//! This is deliberately tiny and honest. It is **not** web-platform-tests, **not**
//! a Chromium-level conformance suite, and it proves nothing about modern web
//! compatibility — only that the documented [Compatibility Level 1] subset keeps
//! working and that unsupported features keep failing clearly. See
//! `docs/architecture/compatibility-testing.md`.
//!
//! ## Manifest format
//!
//! A minimal, hand-parsed TOML subset (no `serde`/`toml` dependency): a sequence
//! of `[[test]]` tables, each with `key = value` lines where a value is a quoted
//! string or a number.
//!
//! ```toml
//! [[test]]
//! name = "html_basic_document"
//! path = "html/basic-document.html"
//! category = "html"
//! expect = "html/basic-document.expect.txt"
//!
//! [[test]]
//! name = "css_unsupported_float"
//! path = "css/float.html"
//! category = "css"
//! status = "unsupported"
//! expect_error_contains = "unsupported"
//! ```
//!
//! ## Blessing snapshots
//!
//! Run with `MOCHA_BLESS=1` to (re)write every `expect` file from the current
//! normalized render. Review the diff before committing.

use std::collections::HashMap;
use std::path::Path;

use mocha_devtools::{format_snapshot, snapshot_rendered_page};
use mocha_engine::{format_form_state, render_url, RenderOptions};
use mocha_error::{MochaError, MochaResult};
use mocha_layout::format_layout_tree;
use mocha_paint::format_display_list;

/// Which snapshot of a render a test compares against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotMode {
    /// The painted display list (default).
    Display,
    /// The layout tree dump.
    Layout,
    /// The full headless DevTools snapshot.
    Devtools,
    /// The form-control state dump.
    FormState,
}

impl SnapshotMode {
    fn parse(value: &str) -> MochaResult<SnapshotMode> {
        match value {
            "display" => Ok(SnapshotMode::Display),
            "layout" => Ok(SnapshotMode::Layout),
            "devtools" => Ok(SnapshotMode::Devtools),
            "form-state" => Ok(SnapshotMode::FormState),
            other => Err(manifest_err(format!(
                "unknown mode {other:?} (want display|layout|devtools|form-state)"
            ))),
        }
    }
}

/// A test's declared intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    /// Expected to meet its expectation (the default).
    Pass,
    /// A documented-unsupported feature: the render is expected to error.
    Unsupported,
    /// Known-broken: the expectation is expected *not* to be met yet.
    Xfail,
    /// Not run (records a reason and a TODO).
    Skip,
}

impl TestStatus {
    fn parse(value: &str) -> MochaResult<TestStatus> {
        match value {
            "pass" => Ok(TestStatus::Pass),
            "unsupported" => Ok(TestStatus::Unsupported),
            "xfail" => Ok(TestStatus::Xfail),
            "skip" => Ok(TestStatus::Skip),
            other => Err(manifest_err(format!(
                "unknown status {other:?} (want pass|unsupported|xfail|skip)"
            ))),
        }
    }
}

/// One compatibility case.
#[derive(Debug, Clone)]
pub struct CompatTest {
    pub name: String,
    pub path: String,
    pub category: String,
    pub mode: SnapshotMode,
    pub status: TestStatus,
    pub reason: Option<String>,
    /// Path (relative to the manifest) of a blessed expected-snapshot file.
    pub expect: Option<String>,
    /// A substring the normalized snapshot must contain.
    pub expect_contains: Option<String>,
    /// A substring the render's error message must contain.
    pub expect_error_contains: Option<String>,
    /// Optional viewport width override (CSS px).
    pub viewport_width: Option<f32>,
}

/// A parsed manifest: the cases plus the manifest's own directory (used to
/// resolve `path` and `expect` entries).
#[derive(Debug, Clone)]
pub struct CompatManifest {
    pub dir: String,
    pub tests: Vec<CompatTest>,
}

/// What happened to one case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Expectation met.
    Passed,
    /// An `unsupported` case errored as expected.
    UnsupportedExpected,
    /// Expectation not met (or an unexpected `xpass`). Carries a detail message.
    Failed(String),
    /// Not run.
    Skipped,
    /// An `xfail` case failed as expected.
    Xfail,
    /// An `expect` file was (re)written under `MOCHA_BLESS=1`.
    Blessed,
}

/// One case's result.
#[derive(Debug, Clone)]
pub struct CompatResult {
    pub name: String,
    pub category: String,
    pub outcome: Outcome,
}

/// The aggregate of a manifest run.
#[derive(Debug, Clone, Default)]
pub struct CompatSummary {
    pub results: Vec<CompatResult>,
}

impl CompatSummary {
    fn count(&self, predicate: impl Fn(&Outcome) -> bool) -> usize {
        self.results
            .iter()
            .filter(|r| predicate(&r.outcome))
            .count()
    }

    pub fn total(&self) -> usize {
        self.results.len()
    }
    pub fn passed(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Passed))
    }
    pub fn unsupported_expected(&self) -> usize {
        self.count(|o| matches!(o, Outcome::UnsupportedExpected))
    }
    pub fn failed(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Failed(_)))
    }
    pub fn skipped(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Skipped))
    }
    pub fn xfail(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Xfail))
    }
    pub fn blessed(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Blessed))
    }

    /// True if any case failed unexpectedly (the CI exit-code signal).
    pub fn has_unexpected_failures(&self) -> bool {
        self.failed() > 0
    }

    /// A deterministic, human-readable report.
    pub fn format(&self) -> String {
        let mut out = String::new();
        out.push_str("Mocha Compatibility Report\n");
        out.push_str(&format!("Total: {}\n", self.total()));
        out.push_str(&format!("Passed: {}\n", self.passed()));
        out.push_str(&format!(
            "Unsupported expected: {}\n",
            self.unsupported_expected()
        ));
        out.push_str(&format!("Failed: {}\n", self.failed()));
        out.push_str(&format!("Skipped: {}\n", self.skipped()));
        out.push_str(&format!("Xfail: {}\n", self.xfail()));
        if self.blessed() > 0 {
            out.push_str(&format!("Blessed: {}\n", self.blessed()));
        }
        let failures: Vec<_> = self
            .results
            .iter()
            .filter_map(|r| match &r.outcome {
                Outcome::Failed(detail) => Some((r, detail)),
                _ => None,
            })
            .collect();
        if !failures.is_empty() {
            out.push_str("\nFailures:\n");
            for (result, detail) in failures {
                out.push_str(&format!(
                    "  [{}] {}: {}\n",
                    result.category, result.name, detail
                ));
            }
        }
        out
    }
}

/// Parse and run every case in the manifest at `path`.
pub fn run_manifest(path: &Path) -> MochaResult<CompatSummary> {
    let bless = std::env::var_os("MOCHA_BLESS").is_some();
    let manifest = load_manifest(path)?;
    let mut results = Vec::with_capacity(manifest.tests.len());
    for test in &manifest.tests {
        let outcome = run_test(test, &manifest.dir, bless);
        results.push(CompatResult {
            name: test.name.clone(),
            category: test.category.clone(),
            outcome,
        });
    }
    Ok(CompatSummary { results })
}

/// Parse a manifest file (without running it).
pub fn load_manifest(path: &Path) -> MochaResult<CompatManifest> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| manifest_err(format!("cannot read manifest {}: {e}", path.display())))?;
    let dir = parent_dir(&path.to_string_lossy());
    let tests = parse_manifest_str(&source)?;
    Ok(CompatManifest { dir, tests })
}

// === running ================================================================

fn run_test(test: &CompatTest, dir: &str, bless: bool) -> Outcome {
    if test.status == TestStatus::Skip {
        return Outcome::Skipped;
    }
    let target = join_path(dir, &test.path);
    let render = render_snapshot(&target, test.mode, test.viewport_width);

    // Error-expecting cases compare the *error message*.
    if let Some(substr) = &test.expect_error_contains {
        let met = match &render {
            Err(error) => error
                .to_string()
                .to_lowercase()
                .contains(&substr.to_lowercase()),
            Ok(_) => false,
        };
        let detail = match &render {
            Ok(_) => format!("expected error containing {substr:?} but render succeeded"),
            Err(error) => format!("error {:?} does not contain {substr:?}", error.to_string()),
        };
        return classify(test, met, detail);
    }

    // Snapshot cases compare normalized output.
    let normalized = match &render {
        Ok(raw) => normalize_snapshot(raw),
        Err(error) => return classify(test, false, format!("render error: {error}")),
    };

    // Bless mode: (re)write the expected file from the current render.
    if bless {
        if let Some(expect) = &test.expect {
            let expect_path = join_path(dir, expect);
            return match std::fs::write(&expect_path, format!("{normalized}\n")) {
                Ok(()) => Outcome::Blessed,
                Err(error) => Outcome::Failed(format!("cannot write {expect_path}: {error}")),
            };
        }
    }

    if let Some(expect) = &test.expect {
        let expect_path = join_path(dir, expect);
        let expected = match std::fs::read_to_string(&expect_path) {
            // Normalize line endings: git `core.autocrlf` checkouts turn the
            // committed LF expectation files into CRLF on Windows, while the
            // rendered snapshot always uses LF.
            Ok(text) => text.replace("\r\n", "\n"),
            Err(_) => {
                return classify(
                    test,
                    false,
                    format!("missing expected file {expect_path} (run with MOCHA_BLESS=1)"),
                )
            }
        };
        let met = normalized.trim_end() == expected.trim_end();
        let detail = snapshot_diff(&expected, &normalized);
        return classify(test, met, detail);
    }

    if let Some(substr) = &test.expect_contains {
        let met = normalized.contains(substr);
        let detail = format!("normalized snapshot does not contain {substr:?}");
        return classify(test, met, detail);
    }

    Outcome::Failed("test has no expectation".to_string())
}

/// Map `(declared status, expectation met)` onto a final outcome.
fn classify(test: &CompatTest, met: bool, detail: String) -> Outcome {
    match test.status {
        TestStatus::Pass => {
            if met {
                Outcome::Passed
            } else {
                Outcome::Failed(detail)
            }
        }
        TestStatus::Unsupported => {
            if met {
                Outcome::UnsupportedExpected
            } else {
                Outcome::Failed(detail)
            }
        }
        TestStatus::Xfail => {
            if met {
                Outcome::Failed(format!(
                    "xpass: expected to fail but expectation was met ({})",
                    test.reason.as_deref().unwrap_or("no reason given")
                ))
            } else {
                Outcome::Xfail
            }
        }
        TestStatus::Skip => Outcome::Skipped,
    }
}

/// Render `target` and produce the raw (un-normalized) snapshot for `mode`.
fn render_snapshot(
    target: &str,
    mode: SnapshotMode,
    viewport_width: Option<f32>,
) -> MochaResult<String> {
    let options = RenderOptions {
        viewport_width: viewport_width.unwrap_or(RenderOptions::default().viewport_width),
        ..RenderOptions::default()
    };
    match mode {
        SnapshotMode::Display => Ok(format_display_list(
            &render_url(target, &options)?.display_list,
        )),
        SnapshotMode::Layout => Ok(format_layout_tree(
            &render_url(target, &options)?.layout_root,
        )),
        SnapshotMode::Devtools => {
            let page = render_url(target, &options)?;
            Ok(format_snapshot(&snapshot_rendered_page(
                &page,
                Some(target.to_string()),
            )?))
        }
        SnapshotMode::FormState => {
            let mut page = render_url(target, &options)?;
            format_form_state(&page.document, &mut page.form_state)
        }
    }
}

fn snapshot_diff(expected: &str, actual: &str) -> String {
    let exp: Vec<&str> = expected.trim_end().lines().collect();
    let act: Vec<&str> = actual.trim_end().lines().collect();
    for (index, (e, a)) in exp.iter().zip(act.iter()).enumerate() {
        if e != a {
            return format!(
                "snapshot differs at line {}: expected {e:?}, got {a:?}",
                index + 1
            );
        }
    }
    if exp.len() != act.len() {
        return format!(
            "snapshot length differs: expected {} lines, got {}",
            exp.len(),
            act.len()
        );
    }
    "snapshot differs".to_string()
}

// === snapshot normalization (Part E) ========================================

/// Normalize a snapshot so it is stable across platforms and runs:
///
/// - convert Windows path separators (`\`) to `/`,
/// - round decimal numbers to 2 places (absorbs tiny float jitter),
/// - replace timestamps/durations with `<TIME>`.
///
/// Node ids are left as-is: the DOM arena numbers nodes deterministically in
/// document order, so they are already stable (see the `node_ids_are_stable`
/// test). Absolute path prefixes are stripped separately by [`strip_prefixes`].
pub fn normalize_snapshot(input: &str) -> String {
    let slashed = input.replace('\\', "/");
    let no_time = replace_timestamps(&slashed);
    round_floats(&no_time)
}

/// Replace each of `prefixes` (longest first) with `<DIR>`. Used when a manifest
/// lives at an absolute path so absolute directories do not leak into snapshots.
pub fn strip_prefixes(input: &str, prefixes: &[String]) -> String {
    let mut sorted: Vec<&String> = prefixes.iter().filter(|p| !p.is_empty()).collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let mut out = input.replace('\\', "/");
    for prefix in sorted {
        let normalized = prefix.replace('\\', "/");
        out = out.replace(&normalized, "<DIR>");
    }
    out
}

/// Round every `<digits>.<digits>` run to 2 decimal places.
fn round_floats(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let number: String = chars[start..i].iter().collect();
                match number.parse::<f64>() {
                    Ok(value) => out.push_str(&format!("{}", (value * 100.0).round() / 100.0)),
                    Err(_) => out.push_str(&number),
                }
            } else {
                for &ch in &chars[start..i] {
                    out.push(ch);
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Replace ISO-8601 date-times and `<number>ms` durations with `<TIME>`.
fn replace_timestamps(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = match_iso_datetime(&chars, i) {
            out.push_str("<TIME>");
            i = end;
        } else if let Some(end) = match_ms_duration(&chars, i) {
            out.push_str("<TIME>");
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Match `YYYY-MM-DDTHH:MM:SS` (optional `.fff` and trailing `Z`); return its end.
fn match_iso_datetime(chars: &[char], start: usize) -> Option<usize> {
    // Only attempt at a 4-digit run boundary.
    if start > 0 && chars[start - 1].is_ascii_digit() {
        return None;
    }
    let shape = "dddd-dd-ddTdd:dd:dd";
    let mut i = start;
    for token in shape.chars() {
        let c = *chars.get(i)?;
        let ok = match token {
            'd' => c.is_ascii_digit(),
            other => c == other,
        };
        if !ok {
            return None;
        }
        i += 1;
    }
    // Optional fractional seconds.
    if chars.get(i) == Some(&'.') {
        let mut j = i + 1;
        while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
        }
        if j > i + 1 {
            i = j;
        }
    }
    if chars.get(i) == Some(&'Z') {
        i += 1;
    }
    Some(i)
}

/// Match `<digits>(.<digits>)?ms`, but only at a number boundary; return its end.
fn match_ms_duration(chars: &[char], start: usize) -> Option<usize> {
    if !chars.get(start)?.is_ascii_digit() {
        return None;
    }
    if start > 0 && (chars[start - 1].is_ascii_digit() || chars[start - 1].is_alphabetic()) {
        return None;
    }
    let mut i = start;
    while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    if chars.get(i) == Some(&'.') {
        let mut j = i + 1;
        while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
        }
        if j > i + 1 {
            i = j;
        }
    }
    if chars.get(i) == Some(&'m') && chars.get(i + 1) == Some(&'s') {
        // Avoid matching identifiers like `123msg`.
        if chars.get(i + 2).is_some_and(|c| c.is_alphanumeric()) {
            return None;
        }
        return Some(i + 2);
    }
    None
}

// === manifest parsing =======================================================

#[derive(Debug, Clone)]
enum RawValue {
    Str(String),
    Num(f64),
}

fn parse_manifest_str(input: &str) -> MochaResult<Vec<CompatTest>> {
    let mut tables: Vec<HashMap<String, RawValue>> = Vec::new();
    for (number, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[test]]" {
            tables.push(HashMap::new());
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            manifest_err(format!(
                "line {}: expected `key = value`, got {line:?}",
                number + 1
            ))
        })?;
        let key = key.trim().to_string();
        let value = parse_value(value.trim(), number + 1)?;
        let table = tables.last_mut().ok_or_else(|| {
            manifest_err(format!(
                "line {}: `{key}` appears before any [[test]]",
                number + 1
            ))
        })?;
        table.insert(key, value);
    }
    tables.iter().map(test_from_table).collect()
}

fn parse_value(text: &str, line: usize) -> MochaResult<RawValue> {
    if let Some(rest) = text.strip_prefix('"') {
        let body = rest
            .strip_suffix('"')
            .ok_or_else(|| manifest_err(format!("line {line}: unterminated string {text:?}")))?;
        return Ok(RawValue::Str(unescape(body)));
    }
    text.parse::<f64>()
        .map(RawValue::Num)
        .map_err(|_| manifest_err(format!("line {line}: unrecognized value {text:?}")))
}

fn unescape(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn test_from_table(table: &HashMap<String, RawValue>) -> MochaResult<CompatTest> {
    let name = require_str(table, "name")?;
    let path = require_str(table, "path")?;
    let category = opt_str(table, "category")?.unwrap_or_default();
    let mode = match opt_str(table, "mode")? {
        Some(value) => SnapshotMode::parse(&value)?,
        None => SnapshotMode::Display,
    };
    let status = match opt_str(table, "status")? {
        Some(value) => TestStatus::parse(&value)?,
        None => TestStatus::Pass,
    };
    let reason = opt_str(table, "reason")?;
    let expect = opt_str(table, "expect")?;
    let expect_contains = opt_str(table, "expect_contains")?;
    let expect_error_contains = opt_str(table, "expect_error_contains")?;
    let viewport_width = opt_num(table, "viewport_width")?.map(|value| value as f32);

    // Validation: skip/xfail must explain themselves; non-skip cases need exactly
    // one expectation.
    if matches!(status, TestStatus::Skip | TestStatus::Xfail) && reason.is_none() {
        return Err(manifest_err(format!(
            "test {name:?}: status skip/xfail requires a `reason`"
        )));
    }
    if status != TestStatus::Skip {
        let expectations = [
            expect.is_some(),
            expect_contains.is_some(),
            expect_error_contains.is_some(),
        ]
        .iter()
        .filter(|present| **present)
        .count();
        if expectations != 1 {
            return Err(manifest_err(format!(
                "test {name:?}: needs exactly one of expect / expect_contains / \
                 expect_error_contains (found {expectations})"
            )));
        }
    }

    Ok(CompatTest {
        name,
        path,
        category,
        mode,
        status,
        reason,
        expect,
        expect_contains,
        expect_error_contains,
        viewport_width,
    })
}

fn require_str(table: &HashMap<String, RawValue>, key: &str) -> MochaResult<String> {
    opt_str(table, key)?.ok_or_else(|| manifest_err(format!("missing required key `{key}`")))
}

fn opt_str(table: &HashMap<String, RawValue>, key: &str) -> MochaResult<Option<String>> {
    match table.get(key) {
        None => Ok(None),
        Some(RawValue::Str(value)) => Ok(Some(value.clone())),
        Some(_) => Err(manifest_err(format!("key `{key}` must be a string"))),
    }
}

fn opt_num(table: &HashMap<String, RawValue>, key: &str) -> MochaResult<Option<f64>> {
    match table.get(key) {
        None => Ok(None),
        Some(RawValue::Num(value)) => Ok(Some(*value)),
        Some(_) => Err(manifest_err(format!("key `{key}` must be a number"))),
    }
}

// === path helpers ===========================================================

fn parent_dir(path: &str) -> String {
    let path = path.replace('\\', "/");
    match path.rfind('/') {
        Some(index) => path[..index].to_string(),
        None => String::new(),
    }
}

fn join_path(dir: &str, rel: &str) -> String {
    // A test may point `path` at an absolute URL (e.g. `https://example.com/`) to
    // exercise the loader directly; leave those untouched.
    if rel.contains("://") {
        return rel.to_string();
    }
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() || dir == "." {
        rel.to_string()
    } else {
        format!("{dir}/{rel}")
    }
}

fn manifest_err(message: impl Into<String>) -> MochaError {
    MochaError::Shell(format!("compat manifest: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn parses_minimal_manifest() {
        let tests = parse_manifest_str(
            r#"
            # a comment
            [[test]]
            name = "ok"
            path = "html/ok.html"
            category = "html"
            expect_contains = "DrawText"

            [[test]]
            name = "unsup"
            path = "css/float.html"
            category = "css"
            status = "unsupported"
            expect_error_contains = "unsupported"
            "#,
        )
        .unwrap();
        assert_eq!(tests.len(), 2);
        assert_eq!(tests[0].name, "ok");
        assert_eq!(tests[0].mode, SnapshotMode::Display);
        assert_eq!(tests[0].status, TestStatus::Pass);
        assert_eq!(tests[1].status, TestStatus::Unsupported);
        assert_eq!(
            tests[1].expect_error_contains.as_deref(),
            Some("unsupported")
        );
    }

    #[test]
    fn manifest_rejects_missing_expectation() {
        let error =
            parse_manifest_str("[[test]]\nname = \"x\"\npath = \"x.html\"\ncategory = \"html\"\n")
                .unwrap_err();
        assert!(matches!(error, MochaError::Shell(_)));
    }

    #[test]
    fn round_floats_rounds_to_two_places() {
        assert_eq!(
            round_floats("rect=(8.0, 133.456, 12)"),
            "rect=(8, 133.46, 12)"
        );
        assert_eq!(round_floats("x=0.5 y=-3.125"), "x=0.5 y=-3.13");
    }

    #[test]
    fn normalize_converts_windows_separators() {
        assert_eq!(
            normalize_snapshot(r"url: file://tests\compat\html\x.html"),
            "url: file://tests/compat/html/x.html"
        );
    }

    #[test]
    fn strip_prefixes_replaces_absolute_dirs() {
        let out = strip_prefixes(
            "request: /home/u/repo/tests/compat/html/x.html",
            &["/home/u/repo".to_string()],
        );
        assert_eq!(out, "request: <DIR>/tests/compat/html/x.html");
    }

    #[test]
    fn replace_timestamps_strips_iso_and_durations() {
        assert_eq!(
            replace_timestamps("started 2026-06-12T15:04:05Z took 12ms"),
            "started <TIME> took <TIME>"
        );
        // Identifiers that merely contain "ms" are not touched.
        assert_eq!(replace_timestamps("123msg"), "123msg");
    }

    #[test]
    fn node_ids_are_stable() {
        // Two renders of the same document number DOM nodes identically, so the
        // raw snapshot needs no node-id normalization.
        use mocha_engine::{render_html, RenderOptions};
        let html = "<html><body><p id=\"a\">Hi</p><span>there</span></body></html>";
        let snap = |()| {
            let page = render_html(html, &RenderOptions::default()).unwrap();
            format_snapshot(&snapshot_rendered_page(&page, Some("memory".to_string())).unwrap())
        };
        assert_eq!(snap(()), snap(()));
    }

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn scratch_dir() -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("mocha_compat_{}_{}", std::process::id(), id));
        std::fs::create_dir_all(dir.join("html")).unwrap();
        std::fs::create_dir_all(dir.join("css")).unwrap();
        dir
    }

    #[test]
    fn run_manifest_classifies_each_status() {
        let dir = scratch_dir();
        std::fs::write(
            dir.join("html/ok.html"),
            "<html><body><p>Hi</p></body></html>",
        )
        .unwrap();
        // A non-HTML document still fails clearly (its content type is rejected),
        // which keeps the harness's "unsupported" classification meaningful now
        // that unsupported *CSS* is skipped rather than fatal (Milestone 23).
        std::fs::write(dir.join("css/note.txt"), "plain text, not html").unwrap();
        let manifest = r#"
            [[test]]
            name = "ok"
            path = "html/ok.html"
            category = "html"
            expect_contains = "DrawText"

            [[test]]
            name = "unsup"
            path = "css/note.txt"
            category = "css"
            status = "unsupported"
            expect_error_contains = "unsupported"

            [[test]]
            name = "willfail"
            path = "html/ok.html"
            category = "html"
            expect_contains = "THIS_STRING_NEVER_APPEARS"

            [[test]]
            name = "skipme"
            path = "html/ok.html"
            category = "html"
            status = "skip"
            reason = "demonstration"
            expect_contains = "ignored"
        "#;
        let manifest_path = dir.join("m.toml");
        std::fs::write(&manifest_path, manifest).unwrap();

        let summary = run_manifest(&manifest_path).unwrap();
        assert_eq!(summary.total(), 4);
        assert_eq!(summary.passed(), 1);
        assert_eq!(summary.unsupported_expected(), 1);
        assert_eq!(summary.failed(), 1);
        assert_eq!(summary.skipped(), 1);
        assert!(summary.has_unexpected_failures());

        std::fs::remove_dir_all(&dir).ok();
    }
}
