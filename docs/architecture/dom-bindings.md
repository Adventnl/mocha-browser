# JavaScript DOM Bindings (Milestone 7)

Milestone 7 bridges the from-scratch JavaScript interpreter (`mocha_js`) to the
DOM (`mocha_dom`) so that small inline `<script>`s can read and mutate the
document, register event listeners, and schedule deterministic timers. The bridge
lives in the `mocha_js_dom` crate.

**This is not a browser DOM.** The API surface is deliberately tiny, there is no
live `NodeList`, no real event loop, no microtasks, and no security model. Mocha
does not claim JavaScript-DOM compatibility.

## Host-object model

The interpreter gained a real host mechanism (not string matching, not a second
engine):

- A new value variant, `JsValue::Host(Rc<dyn HostObject>)`.
- A trait `HostObject { class_name, as_any, get, set, call }`. Implementors carry
  their own interior-mutable state behind `Rc<RefCell<…>>`, so a host can be
  aliased freely by script and `===` compares by pointer identity (`Rc::ptr_eq`).
- The interpreter routes `obj.prop` reads, `obj.prop = v` writes, `obj.method(a)`
  calls, and string indexing (`obj["prop"]`) to the trait methods.
- A `Function::NativeClosure` variant lets the bridge install global functions
  that capture shared state (e.g. `setTimeout`).

Unknown host properties return `undefined` (JS semantics); unknown host methods
return a clear `MochaError::JavaScript` error.

## Globals

`mocha_js_dom::DomRuntime` installs three globals before running scripts:

- `window` — a host object. `window.document === document`; exposes
  `window.console`, `window.setTimeout`, `window.clearTimeout`.
- `document` — a host object backing the shared DOM.
- `console` — the interpreter's built-in console (its `log` output is captured).
- `setTimeout` / `clearTimeout` are also bare globals (native closures).
- `localStorage` / `sessionStorage` — host objects exposing
  `getItem`/`setItem`/`removeItem`/`clear`/`length` (Milestone 15).

### Web state (Milestone 15)

`DomRuntime::with_url(document, url)` gives scripts the document URL, so
`document.cookie` and web storage have an origin:

- **`document.cookie`** — getter returns the non-`HttpOnly` cookies for the
  document URL (`n1=v1; n2=v2`); setter stores one cookie (ignoring an `HttpOnly`
  attribute from script). Backed by a per-render in-memory `mocha_cookie::CookieJar`.
- **`localStorage` / `sessionStorage`** — origin-keyed string storage (missing
  key → `null`). Per-render in-memory backends (JS-side persistence and the
  tab-scoped `sessionStorage` wiring are deferred).
- All three require an **http(s) origin**: on a `file://` or in-memory document
  they return a clear `MochaError::Security`.

See [cookies-and-web-storage.md](cookies-and-web-storage.md).

## DOM API surface

`document`:

- `getElementById(id)` → node host or `null`
- `querySelector(sel)` / `querySelectorAll(sel)` → node host / JS array of node
  hosts, in document order (reusing `mocha_style`'s selector matcher; the M2
  grammar: type/class/id/universal/compound/descendant). Unsupported selectors
  error clearly.
- `createElement(tag)` → detached element host. Only a fixed allow-set of tags is
  creatable (the parser's supported set: `html, body, h1, h2, p, div, span, a,
  style, script, img, link, form, input, button, label, textarea, select,
  option`); anything else is `UnsupportedFeature`.
- `createTextNode(text)` → detached text host.
- `document.body` / `document.documentElement` → node host or `null`.

Node (element or text) host:

- read: `textContent`, `innerHTML`, `id`, `className`, `tagName`/`nodeName`
- write: `textContent`, `innerHTML`, `id`, `className`
- methods: `getAttribute(name)`, `setAttribute(name, value)`,
  `appendChild(child)`, `removeChild(child)`, `addEventListener(type, fn[, capture])`,
  `removeEventListener(type, fn[, capture])`

`appendChild` detaches the child from its previous parent first. Unknown
("expando") property writes are accepted but not persisted onto the DOM.

### Form-control properties (Milestone 10)

Form controls additionally expose properties backed by the shared
`mocha_forms::FormState` (not by DOM attributes — attributes only initialize the
state; see [forms-and-controls.md](forms-and-controls.md)):

- read/write: `value`, `checked`, `selected`, `disabled`; read-only: `type`
  (normalized), `name`.
- selects: `value` derives from the selected option (setting it selects the
  matching option, or deselects all on no match); `selectedIndex` gets/sets the
  index (`-1` only for an option-less select).
- setting `checked = true` on a radio unchecks the rest of its group.
- `form.submit()` records a pending submission request (first call wins) that
  the embedder takes via `DomRuntime::take_pending_submission()` — it never
  navigates. Calling `submit()` on a non-form node is a clear error.

On non-control nodes these properties read as `undefined` and writes fall
through to the expando rule above. `DomRuntime::init_form_state()` initializes
state for the whole document before scripts run (erroring on unsupported
control types); `DomRuntime::form_state()` hands the embedder the same shared
state for layout/paint/submission.

### `innerHTML`

The setter parses the string as a small HTML fragment with the existing parser
(only supported tags; scripts inside the fragment do **not** execute), then
replaces the element's children by deep-copying the parsed nodes into the live
document arena. The getter is a minimal serializer of the element's children. The
full innerHTML parsing/serialization algorithm is out of scope.

## Event listeners

JS listeners are stored in the bridge (not in `mocha_events`, whose `FnMut`
listeners cannot re-enter the interpreter). `DomRuntime::dispatch_event(type,
target)` walks the DOM flow itself — capturing (root→parent), at-target (capture
then bubble), bubbling (parent→root) — invoking matching JS callbacks with an
`event` host object that supports `type`, `target`, `currentTarget`,
`defaultPrevented`, `preventDefault()`, `stopPropagation()`,
`stopImmediatePropagation()`. It returns whether the default action should
proceed. Since there is no real window, tests/host code dispatch events
programmatically. This mirrors `mocha_events`' semantics with a small amount of
deliberate duplication.

## Timers

`setTimeout(fn, delay)` enqueues a task in a **deterministic queue** — there is no
real clock; only insertion order matters. `clearTimeout(id)` cancels by id.
`DomRuntime::run_pending_timers()` drains the queue in insertion order (skipping
cancelled tasks) and is called once, after all inline scripts run. Tasks queued
during a callback run in the same drain (FIFO), with a runaway guard.

## Script execution & invalidation

The shell pipeline:

1. parses HTML to a DOM,
2. collects inline `<script>` sources in document order (external `<script src>`
   is `UnsupportedFeature`),
3. runs them on one shared interpreter (so a later script sees an earlier
   script's globals), then runs pending timers,
4. then runs style → layout → paint **once** over the final DOM.

This is **coarse invalidation**: there is no incremental relayout. A script
(parse or runtime) error aborts the render with a clear error; `console.log`
output is written to stderr so it never corrupts the rendered stdout.

`<script>` is parsed as raw text (so `<`/`>` in source survive) and has UA
`display: none`, so its text is never laid out or painted.

## Limitations

No live `NodeList`, no `MutationObserver`, no real event loop/microtasks, no
`Promise`/`async`/`await`, no modules/classes, no full `this`/prototype chain, no
external scripts, no real timers, no DOM Level-2+ surface, no security model. The
interpreter subset itself has no ternary `?:`, no `switch`, no `try`/`catch`, and
deep recursion can overflow the native stack (the step budget counts logical
steps, not stack depth).
