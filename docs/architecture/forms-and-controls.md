# Forms and Basic Input Controls (Milestone 10)

Milestone 10 gives Mocha a small, honest forms foundation: form controls parse,
carry dynamic state, lay out, paint, respond to programmatic clicks, and model
GET submission. There is **no interactive window**: state changes come from
JavaScript and programmatic event dispatch, never from a real keyboard or mouse.

## Supported tags and types

| Element      | Notes                                                                  |
| ------------ | ---------------------------------------------------------------------- |
| `<form>`     | `action`, `method` (`get` only; `post` recognised but unsupported)     |
| `<input>`    | void element; types `text` (default), `password`, `checkbox`, `radio`, `submit`, `reset`, `hidden` |
| `<button>`   | types `submit` (default), `button`, `reset`                            |
| `<label>`    | parses and lays out inline; `for` is stored but clicks do **not** activate the control |
| `<textarea>` | raw-text content becomes the initial value; `rows`/`cols` size the box |
| `<select>`   | single-select only                                                      |
| `<option>`   | `value` (falls back to its text), `selected`, `disabled`                |

Any other `input`/`button` type (`date`, `file`, `range`, …) is a clear
`UnsupportedFeature` error during form processing — never a silent fallback.
Unknown `method` values (including `dialog`) are unsupported.

## The form-state model (`mocha_forms`)

Dynamic control state lives **outside** the DOM in a `FormState` keyed by
`NodeId`:

```rust
pub struct ControlState {
    pub kind: ControlKind,    // Text | Password | Checkbox | Radio | Submit |
                              // Reset | Button | Hidden | TextArea | Select | Option
    pub name: Option<String>,
    pub value: String,
    pub checked: bool,
    pub selected: bool,
    pub disabled: bool,
}
```

Attributes initialize the state exactly once: `value` → value (checkbox/radio
default to `"on"` when absent), `checked` → checked, `selected` → selected,
`disabled` → disabled, a textarea's text content → value, and an option's
`value` attribute (or its text) → value. After initialization the DOM
attributes and the state are independent — like a real browser's
attribute/property split. Controls added later (e.g. via `innerHTML`) are
initialized lazily on first access.

Select normalization: when several options carry `selected`, the last wins;
when none does, the **first option** is selected (browser behaviour for
single-select dropdowns). `selectedIndex` is `-1` only for an option-less
select.

Form ownership is the nearest `<form>` ancestor; the HTML `form` attribute is
not supported.

## JavaScript bindings (`mocha_js_dom`)

Controls expose `value`, `checked`, `disabled`, `type` (normalized), and
`name`; selects add `selectedIndex` (getter/setter) and derive `value` from the
selected option (setting `value` selects the matching option); options expose
`value`/`selected`/`disabled`. Setting `checked = true` on a radio unchecks its
group. All reads and writes go through the shared `FormState`, so script
changes are visible to layout, paint, and submission.

`form.submit()` records a pending submission request (first call wins) instead
of navigating; the embedder takes it via
`DomRuntime::take_pending_submission()`. The shell notes the request on stderr
and does not navigate.

## Layout and paint

Default display: `form` is block, `label` is inline, `input`/`button`/
`textarea`/`select` are inline replaced items (Mocha has no `inline-block`),
and `option` generates no box. The shell resolves each control into a
`ControlBox` (the forms counterpart of the image `ReplacedBox`) with these
default content sizes, overridable by CSS `width`/`height`:

| Control          | Default size                                              |
| ---------------- | --------------------------------------------------------- |
| text / password  | 160 × 24                                                  |
| checkbox / radio | 13 × 13                                                   |
| button / submit / reset | label estimate (`chars × font × 0.6 + 16`, min 40) × 26 |
| textarea         | `cols × 8` × `rows × 18`, defaulting to 200 × 80          |
| select           | 160 × 24                                                  |

Buttons display their label (a `<button>`'s text content, an `<input>`'s
value, or `Submit`/`Reset`); selects display the selected option's **value**.
`type="hidden"` produces no box at all. Controls participate in inline
formatting — they share lines with text, wrap, and raise the line height — and
a `display: block` control lays out like a block replaced element. Control
boxes carry their DOM node, so `--hit-test` resolves clicks onto them.

Paint emits one `DrawControl` per visible control:

```text
DrawControl type=text x=66 y=8 width=160 height=24 value="mocha" disabled=false
DrawControl type=checkbox x=8 y=8 width=13 height=13 checked=true disabled=false
```

Nothing is rasterized: like `DrawImage`, the command carries everything a
future real surface needs. A button's label and a select's options are **not**
painted as separate text.

## Default actions

After event listeners run, an un-prevented `click` on (or inside) a control
triggers, via `mocha_forms::form_default_action_for_event` (internal events) or
`mocha_forms::click_default_action` (the JS dispatch path):

- **checkbox** — toggles `checked`;
- **radio** — checks it and unchecks same-name radios in the same form (or
  among formless radios);
- **submit** (`<input type=submit>` / `<button type=submit>`) — identifies the
  form and submitter; the caller decides whether to build a `FormSubmission`;
- **reset** — restores every control in the form to its attribute-initialized
  state.

`preventDefault` suppresses all of these; disabled controls do nothing. Text
controls have no click action (no focus/caret exists), and label clicks do not
activate their control.

## Form submission (GET only)

`mocha_forms::build_submission(document, state, form, submitter, base)`
collects the **successful controls** in document order — enabled, named
text/password/hidden/textarea values, checked checkboxes/radios, the selected
option of each select, and the named submitter — and excludes disabled or
unnamed controls, unchecked checkboxes/radios, non-submitter submit buttons,
reset buttons, and `type=button` buttons.

The `action` attribute is resolved against the document base URL; an empty or
missing `action` submits to the document URL itself. Fields are serialized as
`application/x-www-form-urlencoded` (space as `+`, UTF-8 percent-encoding) into
the URL query, and the fragment is dropped:

```text
<form action="/search" method="get"> + q=mocha, page=1
  → http://example.com/search?q=mocha&page=1
```

Nothing navigates automatically. The resulting `action` is a plain `Url` an
embedder may pass to `mocha_nav` explicitly. **POST returns
`UnsupportedFeature("POST form submission is not supported in Milestone 10")`**
— Mocha never fakes a network submission.

## Shell support

`--dump-form-state <doc>` prints one line per form and control (after inline
scripts have run):

```text
form node=#2 action="/search" method="get"
text node=#4 name="q" value="mocha" disabled=false
submit node=#6 name="" value="Search" disabled=false
```

There is no interactive CLI form input — Mocha does not pretend to have a
browser window.

## Limitations

No keyboard text editing, focus, caret, selection, or IME. No validation or
validation messages. No POST bodies, `multipart/form-data`, or cookies. No
file/date/color/range/number inputs. No `:checked`/`:disabled`/`:focus`
pseudo-classes. No `<optgroup>`/`<fieldset>`/`<legend>`, no multiple-select,
no `form` attribute, no label activation, no autofill or password manager.
Controls are display-list commands, not real rendered widgets.
