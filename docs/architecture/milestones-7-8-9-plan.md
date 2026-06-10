# Implementation Plan — Milestones 7, 8, 9 (+ Part A hardening)

> **Status: design only — no code written yet.** This document is the
> execution-ready blueprint for JavaScript DOM bindings (M7), subresource
> loading (M8), and images / replaced elements (M9). It is written against the
> *actual* current source (every crate was read end-to-end), so the signatures
> and data-model diffs below are concrete contracts, not sketches.

---

## 0. Environment status (read this first)

This plan was produced in an environment with **no Rust toolchain installed**:
`cargo`/`rustc`/`rustup` are absent from PATH and the filesystem, there is no
crate registry cache, no network to fetch crates, and no `target/` directory
(the repo has never been compiled here). Consequences:

1. **Nothing below has been compiled or tested.** Every gate in this plan
   (`cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D
   warnings`, `cargo test --all`) **must be run on a real toolchain** before any
   milestone is declared complete. The borrow-checker-heavy host-object bridge
   (§B.1) in particular needs a compiler in the loop and should be expected to
   need 1–2 iteration rounds.
2. **Part A baseline is unverified.** An earlier `cargo test --all 2>&1 | tail`
   reported `tail`'s exit code, masking `cargo: command not found`. The baseline
   must be re-run for real as step 1 of execution.
3. **M9 image decoding depends on the `image` crate** (user-selected). It cannot
   be fetched here, so M9 is wired for the crate but actual decode only runs once
   the crate is available offline/vendored. The project rule *"do not write image
   decoders from scratch"* is respected: no hand-rolled decoder.

**Execution order (gated):** Part A → M7 → verify+doc → M8 → verify+doc → M9 →
verify+doc. Do not start a milestone until the previous one's gate is green.

---

## 1. Assumptions, scope, non-goals, success criteria

### Assumptions
- The reported M1–M6 state matches what is in the tree (verified by reading, not
  running). The JS interpreter is a tree-walker with `JsValue`, environments,
  closures, objects/arrays, `console`/`Math` builtins, and a 100k-step budget.
- `<script>` is currently **rejected** by `mocha_html`; `<link>`/`<img>` are
  rejected too. `mocha_dom` already has stable `NodeId`s and most mutation
  primitives (`create_element`, `create_text`, `append_child`, `remove_child`,
  `get_attribute`, `tag_name`, `traverse_depth_first`, `ancestors`).
- Coarse invalidation is acceptable: **run scripts, then style/layout/paint
  once.** No incremental relayout this milestone.
- Tests must never touch the public internet; use local files + the existing
  `mocha_net::test_server::TestServer`.

### Scope (what gets built)
- **M7:** a real host-object mechanism in `mocha_js`; a new `mocha_js_dom` crate
  bridging the interpreter to the DOM; `window`/`document`/`console` globals;
  the DOM read/mutate/query APIs listed in §B; inline `<script>` execution in
  document order with coarse re-render; JS `addEventListener` + a deterministic
  `setTimeout` task queue.
- **M8:** base-URL resolution (incl. dot-segment normalization), `<link
  rel="stylesheet">` discovery + loading through `mocha_net`, content-type
  validation, document-order cascade; a new `mocha_resources` crate.
- **M9:** the `image` crate dependency, a new `mocha_image` crate, `<img>`
  parsing, image resource loading + decoding, replaced-element layout
  (intrinsic/attribute/CSS sizing), and a `DrawImage` display command.

### Strict non-goals (return clear errors, never fake)
Full DOM/HTML parsing algorithm, `innerHTML` correctness beyond a small
fragment, MutationObserver, custom/shadow DOM, canvas, storage, fetch/XHR/
WebSocket, Promise/async/await, modules, **external `<script src>`** (stays
`UnsupportedFeature` through M9), defer/async, real event loop/microtasks,
incremental relayout, CSP/same-origin/security, CSS `url(...)` resources, fonts,
`srcset`/`<picture>`/lazy/responsive images, SVG, animated GIF, video, real
raster-window output. None of these may be claimed as working.

### Success criteria (per the prompt, restated as the gate)
M1–M6 tests/examples still pass; inline script mutates DOM and the final display
list changes; script-created element appears in the final display list; script
style mutation changes final color/font-size; `getElementById`/`querySelector`/
`createElement`/`createTextNode`/`appendChild`/`removeChild`/`textContent`/
`setAttribute`/`getAttribute`/`className`/`id` all work; script errors are clear;
`<script src>` stays unsupported; external `<link>` stylesheet works over
file+HTTP with content-type validation and correct cascade order; inline style
still wins; failed stylesheet errors clearly; `<img>` parses as a void element,
resolves against base URL, loads over file+HTTP, decodes dimensions (PNG+JPEG
via the `image` crate), validates content-type, lays out as a replaced element
with intrinsic/attribute/CSS sizing, and paints a `DrawImage`; image failures are
clear. `fmt`/`clippy`/`test` all green. No forbidden engine; nothing faked.

### Risk areas
1. **Borrow-checker churn** in threading `&mut Interpreter` through host member
   access and keeping `Rc<RefCell<Document>>` shared (§B.1, §B.6). Highest risk.
2. **JS event dispatch needs the interpreter live during propagation** — solved
   by owning interpreter+bridge together in a `DomRuntime` and walking the path
   in `mocha_js_dom` rather than via `mocha_events`' `FnMut` model (§B.5.4).
3. **Inline replaced layout** (images sharing a line with text) requires
   generalizing the word stream to an item stream in `mocha_layout::line` (§D.5).
4. **`image` crate availability** (offline). Wired but decode-gated until present.
5. **`Rc` keep-alive** across the scripts→style boundary — solved by borrowing
   the shared `Document` rather than `try_unwrap` (§B.6).

### Dependency plan
- M7, M8: **std-only.** New crates `mocha_js_dom`, `mocha_resources` depend only
  on existing workspace crates + `mocha_error`.
- M9: add exactly one external crate, justified:
  ```toml
  # workspace Cargo.toml [workspace.dependencies]
  image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
  ```
  `default-features = false` keeps the dependency tree minimal (no GIF/WebP/TIFF/
  etc. codecs, no `rayon`); `png` + `jpeg` cover the milestone. Only `mocha_image`
  depends on it. No browser engine, JS engine, or webview is added.

---

## Part A — Milestone 6 hardening gate

Static review of `mocha_js` (read in full). Findings + planned actions:

**A1 — doc honesty.** Re-read `docs/architecture/javascript-interpreter.md` and
ensure it claims only: standalone custom interpreter, small tested subset,
lexer/parser/AST/interpreter, functions/closures, objects/arrays, basic
builtins, `console.log` capture, step limit. Remove any wording implying
ECMAScript/DOM/`window`/`document`/script tags/timers/promises/classes/full
`this`. (Action: audit + edit doc; no code.)

**A2 — runtime stability.** The library path looks panic-free: errors are
`MochaError::JavaScript(_)`/`Parse(_)`, the step budget guards loops, and
`unwrap`s are confined to `#[cfg(test)]`. One honest **known limitation to
document, not fix**: deep recursion (e.g. `fact(100000)`) or deeply nested
expressions recurse on the native stack and can overflow before the step budget
trips, since `tick()` counts logical steps, not stack depth. Add a one-line note
to the interpreter docs. (No behavioral change in Part A.)

**A3 — scope readiness.** The interpreter currently has **no host mechanism**:
`Function::Native` is a bare `fn` pointer that cannot capture shared DOM state,
`get_member` takes `&self`, and there is no `JsValue` variant for host-backed
objects. This is the single biggest enabler and is implemented at the **start of
M7** (§B.1), not Part A. Documented here so the gate is honest about what M6 can
and cannot do today.

**Part A gate:** run the full targeted suite for real
(`cargo test -p mocha_js -p mocha_events -p mocha_nav -p mocha_shell`, the three
integration tests, fmt, clippy) and the example/`--eval-js`/HTTPS-fails commands.
Fix any real breakage before touching M7.

---

## Part B — Milestone 7: JavaScript DOM bindings

### B.1 `mocha_js` host-object extension (the core enabler)

**New value variant** in [value.rs](../../crates/mocha_js/src/value.rs):
```rust
pub enum JsValue {
    Number(f64), Str(String), Bool(bool), Null, Undefined,
    Object(Rc<RefCell<HashMap<String, JsValue>>>),
    Array(Rc<RefCell<Vec<JsValue>>>),
    Function(Rc<Function>),
    Host(Rc<dyn HostObject>),   // NEW
}
```
Update every `match` on `JsValue` (small, the compiler enumerates them):
`is_truthy` → `true`; `type_name` → `"object"`; `to_number` → `NaN`;
`stringify` → `format!("[object {}]", h.class_name())`; `strict_equals` →
`(Host(a), Host(b)) => Rc::ptr_eq(a, b)`; custom `Debug` arm.

**New trait** (in a new `mocha_js::host` module, re-exported from `lib.rs`):
```rust
pub trait HostObject {
    /// Name used for `[object X]` / debugging (e.g. "HTMLDivElement", "Document").
    fn class_name(&self) -> &str;
    /// `obj.name` read. Return `Undefined` for unknown props (JS semantics).
    fn get(&self, it: &mut Interpreter, name: &str) -> MochaResult<JsValue>;
    /// `obj.name = value` write.
    fn set(&self, it: &mut Interpreter, name: &str, value: JsValue) -> MochaResult<()>;
    /// `obj.name(args)` method call.
    fn call(&self, it: &mut Interpreter, name: &str, args: Vec<JsValue>) -> MochaResult<JsValue>;
}
```
The trait takes `&self` (not `&mut self`): all host objects carry their own
shared mutable state behind `Rc<RefCell<…>>` (the DOM bridge), so interior
mutability does the work and the same host can be aliased freely by JS.

**Interpreter wiring** in [interpreter.rs](../../crates/mocha_js/src/interpreter.rs):
- Change `fn get_member(&self, …)` → `fn get_member(&mut self, …)`. Add:
  ```rust
  JsValue::Host(h) => { let h = h.clone(); h.get(self, property) }
  ```
  (clone the `Rc` first to release the borrow on `object` before `&mut self`).
- In `assign_target`'s `Expr::Member` arm, add a `JsValue::Host(h)` case →
  `h.clone().set(self, property, value)`.
- In `call_member`, **before** the array special-case, add:
  ```rust
  if let JsValue::Host(h) = &object { let h = h.clone(); return h.call(self, property, args); }
  ```
- In `get_index`/`set_index`, route `JsValue::Host(h)` to `h.get/.set(self,
  &index.stringify(), …)` so `el["textContent"]` works.

**New public Interpreter API** (for the bridge crate):
```rust
impl Interpreter {
    pub fn define_global(&mut self, name: &str, value: JsValue);          // env.define on global
    pub fn call_function(&mut self, f: JsValue, args: Vec<JsValue>) -> MochaResult<JsValue>; // wraps call_value
    pub fn record_console(&mut self, line: String);                       // pushes to self.console
}
```
`console.log` keeps working unchanged (the existing `Object`+`Native` console in
`builtins.rs` writes to `self.console`); the bridge does not need to replace it.

**`mocha_js` tests to add:** a test-only `HostObject` (e.g. a counter) proving
get/set/call round-trip through `Interpreter`; host identity (`===`) via
`ptr_eq`; unknown host property returns `undefined`; calling an unknown host
method errors clearly.

### B.2 `mocha_dom` mutation/query helpers

Most primitives exist. Add to [mocha_dom](../../crates/mocha_dom/src/lib.rs):
```rust
pub fn text_content(&self, node: NodeId) -> MochaResult<String>;          // concat descendant Text in pre-order
pub fn set_text_content(&mut self, node: NodeId, text: impl Into<String>) -> MochaResult<()>;
    // Element: drop all children, append one Text child. Text: replace text. Comment/Doctype: Err(Dom).
pub fn set_attribute(&mut self, node: NodeId, name: impl Into<String>, value: impl Into<String>) -> MochaResult<()>;
    // Element only (else Err(Dom)); replace existing same-named attr or push.
pub fn get_attribute_owned(&self, node: NodeId, name: &str) -> MochaResult<Option<String>>;
pub fn get_element_by_id(&self, id: &str) -> MochaResult<Option<NodeId>>; // pre-order, first match
```
`create_element`/`create_text`/`append_child`/`remove_child` already exist and
keep `NodeId` stable. `append_child` already rejects re-parenting; the binding
detaches first (calls `remove_child(old_parent, child)`) when JS moves a node.

### B.3 `mocha_style` query-selector API + `mocha_css` selector parser

`mocha_css`: expose the already-internal selector-list parser:
```rust
// mocha_css/src/parser.rs → re-export from lib.rs
pub fn parse_selector_list(input: &str) -> MochaResult<Vec<Selector>>;
    // Parser::parse_selector_list(), then assert fully consumed (no trailing tokens / no '{').
```
`mocha_style`: add a DOM-walking matcher reusing `selector_matches` +
`ElementDescriptor` (already present):
```rust
pub fn query_selector(doc: &Document, selector: &str) -> MochaResult<Option<NodeId>>;
pub fn query_selector_all(doc: &Document, selector: &str) -> MochaResult<Vec<NodeId>>;
    // parse_selector_list(selector)?; pre-order walk building ancestor ElementDescriptors;
    // return elements where ANY selector in the list matches; document order; all() returns all.
```
Supported grammar is exactly M2's (type/`.class`/`#id`/`*`/compound/descendant);
unsupported selectors surface the existing `UnsupportedFeature` from the parser.
No second selector engine is created.

### B.4 `mocha_html` — `<script>` raw text (+ groundwork for void tags)

- Add `"script"` to `SUPPORTED_TAGS`.
- Tokenizer: generalize the existing `<style>` raw-text path so **`script` is
  also raw-text** — its body is captured verbatim until `</script>` (case-
  insensitive), preserving `<`, `>`, quotes. Refactor `run()`'s `raw_style`
  check into a `raw_text_tag(name) -> bool` covering `style` + `script`.
- Tree builder: `<script>` becomes an element with one raw `Text` child (exactly
  like `<style>`). Unterminated `<script>` → existing `Parse` error.
- `mocha_style` UA default: `script` → `display: none` (so script text never
  lays out or paints, same as `style`). Add `"script" => "none"` to `ua_defaults`.
- **External scripts stay unsupported:** script collection (§B.5) treats a
  `<script src=...>` as `UnsupportedFeature` ("external scripts are not supported").
- **Script execution does NOT happen in the tokenizer/parser.** Collection +
  execution live in the bridge/shell pipeline (§B.5/§B.6).

Script collection helper (in `mocha_js_dom` or shell):
```rust
pub fn collect_inline_scripts(doc: &Document) -> MochaResult<Vec<String>>;
    // pre-order; for each <script>: if it has a `src` attr → Err(UnsupportedFeature);
    // else concatenate its raw Text children. Returns sources in document order.
```

### B.5 New crate `mocha_js_dom`

Cargo deps: `mocha_error`, `mocha_js`, `mocha_dom`, `mocha_html` (for
`innerHTML` fragment parse), `mocha_style` (for query), `mocha_events` (event
types). **Not** layout/paint/net.

**Shared bridge state** (interior-mutable, cloned into every host):
```rust
struct DomBridge {
    doc: Rc<RefCell<Document>>,
    listeners: RefCell<Vec<JsListener>>,   // JS event listeners
    timers: RefCell<Vec<TimerTask>>,       // deterministic setTimeout queue
    next_listener_id: Cell<u64>,
    next_timer_id: Cell<u64>,
    dirty: Cell<bool>,                      // set on any DOM mutation (coarse invalidation)
}
struct JsListener { id: u64, node: NodeId, event_type: String, capture: bool, callback: JsValue }
struct TimerTask  { id: u64, callback: JsValue, /* delay ignored except ordering */ canceled: bool }
type Bridge = Rc<DomBridge>;
```

**Host objects** (each holds a `Bridge`; nodes also hold a `NodeId`):
- `DocumentHost { bridge, body_cache }` — `class_name = "Document"`.
  - `get`: `"body"` → NodeHost for the `<body>` element (or `Undefined`);
    `"documentElement"` optional; `"title"` optional (read first `<h1>`? — defer,
    return `Undefined`).
  - `call`:
    - `getElementById(id)` → NodeHost or `null`.
    - `querySelector(sel)` → NodeHost or `null` (via `mocha_style::query_selector`).
    - `querySelectorAll(sel)` → **JS `Array`** of NodeHosts (document order; not live).
    - `createElement(tag)` → NodeHost for a detached element; **tag must be in the
      create-allow set** `{html, body, h1, h2, p, div, span, a, style, script,
      img, link}` else `Err(UnsupportedFeature)`. Sets `dirty`.
    - `createTextNode(text)` → NodeHost for a detached text node.
- `NodeHost { bridge, node }` — `class_name` from tag (e.g. `"HTMLParagraphElement"`,
  generic `"HTMLElement"`/`"Text"`).
  - `get`: `"textContent"` → `doc.text_content`; `"innerHTML"` → §below;
    `"id"` → attr or `""`; `"className"` → `class` attr or `""`;
    `"tagName"` → uppercased tag; `"nodeType"` optional;
    `"parentNode"`/`"firstChild"` optional (defer unless cheap).
  - `set`: `"textContent"` → `doc.set_text_content` (+dirty); `"innerHTML"` →
    parse fragment + replace children (§below, +dirty); `"id"`/`"className"` →
    `set_attribute("id"/"class", …)` (+dirty).
  - `call`:
    - `setAttribute(name, value)` (+dirty) / `getAttribute(name)` → value or `null`.
    - `appendChild(childHost)` → detach child from old parent if any, then
      `append_child`; returns the child host (+dirty).
    - `removeChild(childHost)` → `remove_child`; returns the removed child (+dirty).
    - `addEventListener(type, fn[, capture])` → push `JsListener` (§B.5.4).
    - `removeEventListener(type, fn[, capture])` → remove first structurally-equal
      registration (compare `node`,`type`,`capture`,`Rc::ptr_eq` on the function).
- `ConsoleHost` — optional; the existing builtin `console` already works, so M7
  can **reuse it** and skip a host console. (Documented choice.)
- `EventHost { state: Rc<RefCell<EventState>> }` — passed to a JS listener during
  dispatch (§B.5.4). `get`: `"type"`, `"target"` (NodeHost), `"currentTarget"`
  (NodeHost), `"defaultPrevented"`. `call`: `preventDefault()`,
  `stopPropagation()`, `stopImmediatePropagation()` — all mutate `EventState`.
- `WindowHost { bridge, document: JsValue /* the one DocumentHost */ }`.
  - `get`: `"document"` → the stored `DocumentHost` value (so `window.document ===
    document` holds by `Rc::ptr_eq`); `"console"` → the console value.
  - `call`: `setTimeout(fn, delay)` → push `TimerTask`, return its id (number);
    `clearTimeout(id)` → mark canceled.

**B.5.1 `innerHTML` (basic).** Setter: `mocha_html::parse_fragment(html)` — a
new small entry that runs the existing tokenizer+tree-builder but returns the
children of an implicit root (no `<html>/<body>` required), only the
already-supported tags, **scripts inside innerHTML do NOT execute** (collected as
inert script elements). Replace the element's children with the parsed nodes.
Invalid fragment → clear `Parse`/`UnsupportedFeature` error. Getter: a minimal
serializer of children (tag+attrs+text); if that proves fiddly, **return
`UnsupportedFeature` for reading** and keep the setter — documented.

**B.5.2 NodeList.** `querySelectorAll` returns a plain `JsValue::array(Vec<Host>)`
(per the prompt's preferred option). Not live.

**B.5.3 The runtime entry point:**
```rust
pub struct DomRuntime { interp: Interpreter, bridge: Bridge, window: JsValue, document: JsValue }
impl DomRuntime {
    pub fn new(doc: Rc<RefCell<Document>>) -> Self;          // builds bridge+hosts, define_global window/document/console
    pub fn run_script(&mut self, source: &str) -> MochaResult<()>; // parse+run on the SHARED interpreter (globals persist across scripts)
    pub fn run_pending_timers(&mut self) -> MochaResult<()>; // §B.5.5
    pub fn dispatch_event(&mut self, ty: &str, target: NodeId) -> MochaResult<bool>; // §B.5.4; returns !defaultPrevented
    pub fn take_console_output(&mut self) -> Vec<String>;
    pub fn is_dirty(&self) -> bool;
}
```
Scripts run on **one shared `Interpreter`** so a `<script>` can use a global
defined by an earlier `<script>` (matches browser behavior for the inline subset).

**B.5.4 JS event dispatch.** Implemented here (not via `mocha_events`' `FnMut`)
because invoking a JS listener needs the live `Interpreter`. Algorithm mirrors
the DOM flow using `Document::ancestors`:
1. Build `EventState { ty, target, current: None, default_prevented:false,
   prop_stopped:false, immediate_stopped:false, cancelable:true, bubbles:true }`
   in an `Rc<RefCell<…>>`; wrap in an `EventHost`.
2. Path = `ancestors(target)` reversed (root→parent) for capture, then `target`,
   then `ancestors(target)` (parent→root) for bubble.
3. For capture nodes (capture listeners), target (capture then bubble), bubble
   nodes (bubble listeners): for each matching `JsListener` (snapshot ids first),
   set `current`, `interp.call_function(callback, [event_host])`; stop per
   `prop_stopped`/`immediate_stopped`. `once` not required (not in M7 JS scope).
4. Return `!default_prevented`. The shell can then, for an `<a href>` target with
   default not prevented, invoke the existing `mocha_nav` default-action helper —
   satisfying "preventDefault suppresses navigation". (Mirrors `mocha_events`
   semantics; the minor duplication is documented in `dom-bindings.md`.)

**B.5.5 Timers.** `setTimeout(fn, delay)` enqueues a `TimerTask` (delay recorded
but **only insertion order matters** — no real clock). `run_pending_timers`
drains the queue **in insertion order**, skipping canceled tasks, calling each
via `interp.call_function`. New tasks queued during a callback run in the same
drain (FIFO) — documented; guard with a sane cap to avoid infinite enqueue.
`clearTimeout(id)` marks `canceled`.

### B.6 Shell pipeline integration

Today [layout_html](../../crates/mocha_shell/src/lib.rs) does
parse→collect `<style>`→style→layout. New path:
```rust
fn layout_html(input: &str) -> MochaResult<LayoutBox> {
    let document = mocha_html::parse_html(input)?;
    let scripts = collect_inline_scripts(&document)?;          // Err on <script src>
    let doc = Rc::new(RefCell::new(document));
    if !scripts.is_empty() {
        let mut rt = DomRuntime::new(doc.clone());
        for src in &scripts { rt.run_script(src)?; }           // document order; first error aborts render
        rt.run_pending_timers()?;                              // zero-delay tasks after scripts
        // console output: printed by the shell when --show-headers or always to stderr (documented)
    }
    let doc = doc.borrow();                                     // borrow the (possibly mutated) DOM
    let stylesheets = mocha_style::collect_stylesheets(&doc)?;  // M8 replaces this with resource-aware collection
    let styled = mocha_style::build_style_tree(&doc, &stylesheets)?;
    let viewport = LayoutViewport { width: DEFAULT_VIEWPORT_WIDTH, ..Default::default() };
    build_layout_tree(&styled, viewport)
}
```
Key points: a **single full style/layout/paint after scripts** (coarse
invalidation, documented). The shared `Document` is **borrowed**, not
`try_unwrap`'d, so JS closures keeping host objects alive don't break extraction.
Script errors propagate as `MochaError::JavaScript`/`Parse` and fail the render
with a clear message + non-zero exit (existing `main.rs` behavior).

A script that mutates `class`/`id`/`style`/text or appends elements is reflected
because styling reads the mutated DOM. Newly created+appended visible elements
appear in the final display list; `<script>` text never paints (`display:none`).

### B.7 M7 examples & tests

Examples: `examples/js/dom-basic.html`, `examples/js/dom-style-mutation.html`,
`examples/js/event-listener.html`, `examples/js/timer.html` (exactly as in the
prompt).

Tests (crate-targeted + `tests/integration/js_dom_pipeline.rs`):
- `mocha_js`: host get/set/call round-trip; host `===` identity; unknown prop →
  undefined.
- `mocha_html`: `<script>` raw text preserves `<`; unterminated `<script>` errors;
  `<script src>` collection → `UnsupportedFeature`.
- `mocha_dom`: `set_text_content` on element replaces children with one text node;
  on text node replaces text; on comment errors; `set_attribute` element-only;
  `get_element_by_id`.
- `mocha_style`: `query_selector`/`query_selector_all` by type/class/id/descendant
  in document order; unsupported selector errors.
- `mocha_js_dom`: inline script changes `textContent`; `setAttribute("style",…)`
  then re-style changes color/font-size; `className` change flips selector match;
  `createElement`+`appendChild` adds a node; `removeChild`; `createTextNode`;
  `querySelectorAll` length; `addEventListener`+`dispatch_event` runs JS + mutates
  DOM; `preventDefault` suppresses the anchor default action; `setTimeout` mutates
  DOM and `clearTimeout` prevents it; timer order = insertion order.
- shell/integration: script-created element appears in the final display list;
  script style mutation changes the final `DrawText` color/font-size; `<script>`
  text not painted; script error fails render clearly; `document.body` works;
  `console.log` captured deterministically.

---

## Part C — Milestone 8: Subresource loading

### C.1 Base-URL & dot-segment normalization (`mocha_url`)
`Url::join` already handles absolute/scheme-relative/absolute-path/relative. Add
**dot-segment normalization** so `../style.css` resolves:
```rust
fn normalize_path(path: &str) -> String; // resolve "." and ".." against '/'-segments, keep leading '/'
```
Apply it inside `join` after computing the merged path (both http and file). Add
tests: `resolve("style.css")` against `…/dir/page.html` → `…/dir/style.css`;
`resolve("/style.css")` → root; `resolve("../style.css")` → parent; unsupported
scheme rejected; http→file subresource rejected (reuse existing redirect guard).
Base URL itself = `ResourceResponse.final_url` (already tracked). `<base href>` is
**deferred** (documented), since the document final URL suffices.

### C.2 New crate `mocha_resources`
Deps: `mocha_error`, `mocha_url`, `mocha_dom`, `mocha_css`, `mocha_net`. Not shell.
```rust
pub enum SubresourceKind { Stylesheet, Image }     // Image used in M9
pub struct DiscoveredResource { pub node: NodeId, pub kind: SubresourceKind, pub url: Url }
pub fn discover_stylesheets(doc: &Document, base: &Url) -> MochaResult<Vec<DiscoveredResource>>;
    // pre-order: <style> (inline, no url) AND <link rel=stylesheet href> in document order.
    // Only rel=="stylesheet" handled; other rel → ignored (browsers ignore unknown links) [documented].
    // <link> without href → Err(Network/Parse "link rel=stylesheet missing href").
pub fn load_stylesheet(loader: &mut dyn ResourceLoader, url: &Url) -> MochaResult<Stylesheet>;
    // load; require ResourceType::Css (text/css, or missing content-type with .css ext);
    // text/html-as-css → Err; parse via mocha_css::parse_stylesheet; never execute.
```

### C.3 Ordered cascade collection (`mocha_style` or shell)
Replace the shell's `collect_stylesheets(&doc)` call with a resource-aware
collector that walks the document **once in order**, emitting either the parsed
CSS of a `<style>` element or the loaded+parsed CSS of a `<link rel=stylesheet>`.
The resulting `Vec<Stylesheet>` preserves document order, so the existing cascade
(`sheet_index` tie-break in `mocha_style::specified_values`) gives correct
"later sheet wins" behavior, and inline `style="…"` still wins (it is applied
after all sheets — unchanged).

### C.4 HTML `<link>` as a void element
- Add `"link"` to `SUPPORTED_TAGS`; introduce `VOID_TAGS = {"link", "img"}` (img
  in M9). Tree builder: a void start tag is appended but **not pushed** on the
  open stack (no `</link>` needed). Remove the current special-case error that
  rejects `<link>`.
- `mocha_style` UA default for `link`: `display: none` (link never lays out).

### C.5 Dynamic subresources policy
Scripts run first (M7), **then** stylesheets are collected once from the final
DOM. So a JS-created `<link rel=stylesheet>` *is* picked up **iff**
`createElement("link")` + `appendChild` placed it in the tree before collection —
which the M7 create-allow set permits. No incremental/dynamic loading is
implemented; this single post-script collection is the documented model. CSS
`url(...)` stays unsupported (the parser already rejects functions).

### C.6 M8 examples & tests
Examples: `examples/resources/external-css.html` + a sibling `style.css`.
Tests (`tests/integration/subresource_pipeline.rs` + crate tests):
relative+absolute+`..` URL resolution (file & http base); external stylesheet
from local file applies color; from local `TestServer` applies; `text/css`
required; missing content-type + `.css` ext accepted; `text/html`-as-css rejected;
`<style>`/`<link>` order respected; inline style beats external; missing
stylesheet errors clearly; unsupported scheme errors; `<link>` text not painted;
existing inline `<style>` still works; `<link>` without href errors;
JS-created `<link>` picked up in the final collection.

---

## Part D — Milestone 9: Images and replaced elements

### D.1 Dependency & `mocha_image` crate
Add `image` (png+jpeg, no default features) to workspace deps; new crate
`mocha_image` (deps: `mocha_error`, `image`):
```rust
pub enum ImageFormat { Png, Jpeg }
pub struct DecodedImage { pub width: u32, pub height: u32, pub format: ImageFormat }
pub fn decode(bytes: &[u8]) -> MochaResult<DecodedImage>;
    // image::guess_format → map Png/Jpeg, else Err(UnsupportedFeature "image format … not supported");
    // decode dimensions via image::load_from_memory(..).dimensions() (validates the bytes);
    // decode failure → Err(Image(..)). Pixels are not retained (terminal output has no raster surface).
```
**New error variant** `MochaError::Image(String)` in `mocha_error` (decode/format
failures) — one small, justified addition; update `Display`.

### D.2 HTML `<img>` (void element)
Add `"img"` to `SUPPORTED_TAGS` and `VOID_TAGS`. Attributes `src`, `alt`,
`width`, `height` are stored as normal attributes (already supported by the
tokenizer). No `srcset`/`picture`/`loading`/etc. UA default `img`:
`display: inline` (it is the default; no entry needed — but ensure `img` is not
forced to block). A `<img>` with no `src` → handled at load time as a clear error.

### D.3 Image resource loading (`mocha_resources` + shell)
- `discover_images(doc, base)` → `DiscoveredResource{Image}` for each `<img src>`
  (missing `src` → `Err(Layout/UnsupportedFeature "img is missing src")`).
- Load via `mocha_net`; accept `image/png`/`image/jpeg` (or missing content-type
  with `.png`/`.jpg`/`.jpeg` extension — extend `content_type::classify` or check
  in the resource layer); non-image content-type → reject; redirects use existing
  policy; file+http supported, https stays unsupported; decode via `mocha_image`.
- Build an `ImageStore { images: Vec<DecodedImage> }` (shell-owned) returning a
  `usize` id per image, plus a `HashMap<NodeId, usize>` map.
- Failure policy: **fail render clearly** on load/decode error (broken-image
  placeholder is out of scope, documented).

### D.4 Replaced content in the style tree (`mocha_style`)
Add to `StyledNode`:
```rust
pub replaced: Option<ReplacedBox>,         // None for everything except resolved <img>
pub struct ReplacedBox { pub image_id: usize, pub width: f32, pub height: f32 } // FINAL resolved size
```
`build_style_tree` sets `replaced: None` everywhere. A shell-side post-pass
`attach_images(&mut StyledNode, &Document, &NodeId→id map, &ImageStore)` walks the
styled tree and, for each `<img>` with a decoded image, computes the **final**
size and sets `replaced`:
- priority **CSS > attribute > intrinsic** per axis: `w = style.width ?
  attr("width") ? intrinsic.width`; same for height.
- if exactly one of (w,h) is specified (CSS or attr), preserve aspect ratio from
  intrinsic; if both, use both; if neither, use intrinsic. (Documented; simple.)

Putting the final size in `ReplacedBox` keeps layout trivial.

### D.5 Replaced-element layout (`mocha_layout`)
- `box_tree`: add `LayoutBoxKind::Image(usize)` (parallels `TextRun(String)`),
  carrying the image id.
- **Block image** (`display:block`): in `block::layout_block`, detect
  `styled.replaced.is_some()` → produce a box of kind `Image(id)` whose content
  size = `ReplacedBox.{width,height}`, positioned by normal block flow
  (margins/stacking via the existing `margin_box_height`). Background/border on
  images are out of scope (documented).
- **Inline image** (`display:inline`, default): generalize the inline pipeline.
  In `inline.rs`/`line.rs`, replace the `Vec<Word>` stream with a
  `Vec<InlineItem>`:
  ```rust
  enum InlineItem { Word(Word), Image { id: usize, width: f32, height: f32, node_id: NodeId, space_before: bool } }
  ```
  `collect_words` becomes `collect_items`: an inline node with `replaced` pushes an
  `Image` item (width = `ReplacedBox.width`); text still pushes `Word`s.
  `layout_words` → `layout_items`: advance `cursor_x` by item width, wrap the same
  way, and set each line's height to `max(text line-heights, image heights)` so an
  inline image **increases line height** and shares a line with text when it fits.
  Image items become `LayoutBox{kind: Image(id), rect}`; height for line-height =
  the image height. (If this proves too large in review, the documented fallback
  is to treat inline `<img>` as an atom sized by width/height without precise
  baseline — still acceptable per the prompt. No baseline/`vertical-align`.)

### D.6 Paint (`mocha_paint`)
- Add `DisplayCommand::DrawImage { image_id: usize, x: f32, y: f32, width: f32, height: f32 }`.
- `paint_box`: `LayoutBoxKind::Image(id) => push DrawImage{ id, rect.* }`.
- `to_debug_line`: `format!("DrawImage id={image_id} x={x} y={y} width={width} height={height}")`.
- Honestly documented: **`DrawImage` is emitted but nothing is rasterized to a
  window** — the terminal cannot show pixels.

### D.7 M9 examples, asset & tests
Examples: `examples/images/basic-image.html`, `inline-image.html`,
`sized-image.html`, and a tiny checked-in `examples/assets/mocha-test.png`.
**Asset note:** the PNG must be a real decodable file (the `image` crate decodes
it). Generate a minimal valid PNG when a toolchain/Node is available (a few-pixel
solid image); do not check in a large binary. Tests that need an image can
`include_bytes!` a small embedded PNG.

Tests (`tests/integration/image_pipeline.rs` + crate tests):
`<img>` parses as void; `src` stored; `mocha_image::decode` returns PNG (and JPEG)
dimensions; unsupported format errors clearly; non-image content-type rejected;
image loads through the resource loader (file + local `TestServer`); replaced box
uses intrinsic dims; width/height attrs override; CSS width/height override;
single-dimension aspect-ratio preserved; inline image affects line height; block
image stacks; paint emits `DrawImage` with expected dims; text before/after image
paints in order; no `DrawImage` when load fails; missing `src`/missing file error
clearly; HTTP `text/plain` "image" rejected.

---

## Final combined pipeline (honest)
```
input → mocha_url → mocha_nav/mocha_net (load document) → content-type check → UTF-8 decode
      → mocha_html parse → collect inline <script> (Err on src)
      → DomRuntime: install window/document/console → run scripts in order → run pending timers
      → (coarse) full re-render of the final DOM:
          discover+load <link>/<style> in document order (M8)
          discover+load+decode <img> (M9)
          mocha_style build_style_tree → attach_images
          mocha_layout (text + replaced) → mocha_paint (DrawText/DrawRect/DrawBorder/DrawImage)
      → terminal output or --dump-layout
```

## Shell CLI
Existing flags unchanged. Optional, only-if-simple: `--dump-dom` (print DOM after
scripts) and `--show-resources` (list loaded subresources). `--eval-js` stays.
No new CLI parser framework.

## Docs to write/update
New: `dom-bindings.md`, `subresources.md`, `images-and-replaced-elements.md`.
Update: `overview.md`, `rendering-pipeline.md`, `crate-boundaries.md`,
`limitations.md`, `milestones.md`, `javascript-interpreter.md`, `events.md`,
`networking-and-navigation.md`, plus README "Current status: Milestone 9".
Milestones doc: M7/M8/M9 complete, M10 (forms/input) next. **No overclaiming** —
state explicitly: not web-compatible, not full JS DOM, not secure, no external
scripts, `DrawImage` is not rasterized.

## Suggested commits (one per gate, only when the user allows)
- `chore(js): harden milestone 6 before DOM bindings`
- `feat(js-dom): add milestone 7 JavaScript DOM bindings`
- `feat(resources): add milestone 8 subresource loading`
- `feat(image): add milestone 9 image replaced elements`

## Concerns before Milestone 10
- Coarse full re-render won't scale to interactive mutation; M10 forms likely
  need at least targeted re-layout.
- Tree-walker native-stack recursion is still unguarded against deep recursion.
- JS event dispatch duplicates a little of `mocha_events`' algorithm; a future
  unification (JS listeners as first-class `mocha_events` listeners that trampoline
  into the interpreter) would remove the duplication.
- `innerHTML` reading may ship as `UnsupportedFeature`; revisit when a serializer
  is cheap.
- The `image` crate's transitive tree should be reviewed (`cargo tree`) to confirm
  it stays minimal with `default-features = false`.
```
