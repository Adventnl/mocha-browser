# DOM Events (Milestone 5)

Milestone 5 adds Mocha's **internal** DOM event system in `mocha_events`. It is
**not JavaScript** — listeners are Rust callbacks. Milestone 7 added JavaScript
event listeners separately, in `mocha_js_dom`: because a JS callback must re-enter
the live interpreter, `mocha_js_dom` dispatches JS listeners itself (mirroring the
capture/target/bubble semantics here) rather than registering them as
`mocha_events` `FnMut` callbacks. See [dom-bindings.md](dom-bindings.md).

## Event model

`Event` carries `event_type`, `target`, `current_target`, `phase`, `bubbles`,
`cancelable`, `default_prevented`, and a small `EventData` payload
(`Generic`, `Mouse { x, y, button }`, `Keyboard { key, code }`). Constructors:
`Event::new` (bubbling + cancelable generic), `with_options`, `mouse`, `click`,
`keyboard`. Supported event types include `click`, `mousedown`, `mouseup`,
`mousemove`, `keydown`, `keyup`.

## Event phases

```text
None  →  Capturing  →  AtTarget  →  Bubbling  →  None
```

`current_target` is set to the node currently handling the event during
dispatch, and reset to `None` afterward.

## Listener registration

`EventDispatcher` stores listeners per `NodeId`. `add_event_listener(target,
type, EventListenerOptions { capture, once }, callback)` returns a `ListenerId`;
`remove_event_listener(id)` removes it. Callbacks are
`Box<dyn FnMut(&mut Event)>`.

`passive` is intentionally **not** modelled — rather than ship a field that does
nothing, it is omitted and documented as unsupported here.

## Dispatch algorithm

`dispatch_event(document, event)`:

1. Validate the target exists (else a clear `Dom` error).
2. Build the ancestor path (parent → root) via `Document::ancestors`.
3. **Capturing:** root → parent, invoking capture listeners.
4. **At target:** capture-registered then bubble-registered listeners.
5. **Bubbling:** parent → root, invoking bubble listeners (only if `bubbles`).
6. Reset `phase` to `None` and `current_target` to `None`.
7. Return `DispatchResult { default_prevented, propagation_stopped,
   invoked_listeners }`.

Listeners on the same node and phase run in **registration order**. The set of
listeners invoked is snapshotted per node so adding/removing listeners inside a
callback does not affect the in-flight dispatch. `once` listeners are removed
after they run.

## Propagation and cancelation

- `stop_propagation()` prevents the event reaching **later nodes**; listeners
  already scheduled on the current node still run.
- `stop_immediate_propagation()` also stops **remaining listeners on the current
  node**.
- `prevent_default()` sets `default_prevented` **only for cancelable events**.

## Default actions

Default-action interpretation lives in `mocha_nav` (so `mocha_events` stays free
of URL/navigation knowledge): `default_action_for_event(document, event,
base_url) -> DefaultAction`. The only modelled action is **link navigation**: an
un-prevented `click` on (or inside) an `<a href>` yields
`DefaultAction::Navigate(url)`. `href` resolves against `base_url` when provided;
without a base only absolute hrefs resolve. `prevent_default` suppresses it.

## Hit-testing bridge

`mocha_layout::hit_test(root, x, y) -> Option<NodeId>` maps a viewport point to
the **deepest** layout box with a `node_id` that contains the point. Text runs
carry their source text node's id, so clicking link text resolves into the
anchor's subtree. `display: none` nodes produce no box and are never hit. The CLI
exposes this via `--hit-test X,Y`. There is **no** z-index, transform, clipping,
scrolling, or `pointer-events` handling.

## Not implemented

- No JavaScript event listeners (callbacks are Rust-only for now).
- No real window/OS input or event loop — events are dispatched programmatically.
- No pointer, touch, wheel, focus, input, composition, or drag/drop events.
- No accessibility event model.
- `passive` listeners are not supported.
- Link navigation is a **default-action result only**, not an interactive UI.
- Hit-testing ignores z-index, transforms, scrolling, and clipping.

## How JavaScript will connect later

A future milestone will expose `addEventListener`/`removeEventListener` and the
`Event` interface to scripts, with JS listeners wrapped as the same
`Box<dyn FnMut(&mut Event)>` callbacks this dispatcher already runs — so the
phase/propagation/cancelation semantics here are the foundation, unchanged.
