//! Bridge between Mocha's from-scratch JavaScript interpreter (`mocha_js`) and its
//! DOM (`mocha_dom`).
//!
//! It installs `window`/`document`/`console` globals backed by [`mocha_js::HostObject`]s,
//! exposes a small, real subset of the DOM API (read/mutate/query, events, a
//! deterministic timer queue), and runs inline `<script>`s against a shared
//! [`Document`]. This is **not** a browser DOM: the API surface is deliberately
//! tiny, there is no live `NodeList`, no real event loop, and no security model.
//! See `docs/architecture/dom-bindings.md`.
//!
//! ## Invalidation model
//!
//! Scripts mutate the shared document in place and set a coarse `dirty` flag. The
//! shell re-runs style/layout/paint once over the final document after all scripts
//! (and pending timers) have run — there is no incremental relayout.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mocha_dom::{Document, NodeId, NodeKind};
use mocha_error::{MochaError, MochaResult};
use mocha_forms::{ControlKind, FormState};
use mocha_js::{HostObject, Interpreter, JsValue};

/// Tags that `document.createElement` is allowed to create. Anything else is a
/// clear [`MochaError::UnsupportedFeature`] rather than a silently-broken element.
const CREATABLE_TAGS: &[&str] = &[
    "html", "body", "h1", "h2", "p", "div", "span", "a", "style", "script", "img", "link", "form",
    "input", "button", "label", "textarea", "select", "option",
];

/// Collect the source of every inline `<script>` in document order.
///
/// A `<script src=...>` (external script) is reported as
/// [`MochaError::UnsupportedFeature`]; rejecting it here (not in the parser) keeps
/// the HTML layer agnostic about execution.
pub fn collect_inline_scripts(document: &Document) -> MochaResult<Vec<String>> {
    let mut scripts = Vec::new();
    for id in document.traverse_depth_first(document.root_id())? {
        let NodeKind::Element(data) = &document.node(id)?.kind else {
            continue;
        };
        if data.tag_name != "script" {
            continue;
        }
        if data.attribute("src").is_some() {
            return Err(MochaError::UnsupportedFeature(
                "external scripts (<script src>) are not supported".to_string(),
            ));
        }
        let mut source = String::new();
        for &child in document.children(id)? {
            if let NodeKind::Text(text) = &document.node(child)?.kind {
                source.push_str(&text.text);
            }
        }
        scripts.push(source);
    }
    Ok(scripts)
}

// === shared bridge state ====================================================

/// Shared, interior-mutable state every host object holds a handle to.
struct DomBridge {
    doc: Rc<RefCell<Document>>,
    /// Dynamic form-control state (value/checked/selected/disabled), shared with
    /// the embedder so layout/paint/submission see JS changes.
    forms: Rc<RefCell<FormState>>,
    listeners: RefCell<Vec<JsListener>>,
    timers: RefCell<Vec<TimerTask>>,
    next_timer_id: Cell<u64>,
    dirty: Cell<bool>,
    /// The form a script asked to submit via `form.submit()`. The first request
    /// wins (a real browser would have navigated away); the embedder takes it
    /// after scripts run and builds the `FormSubmission`.
    pending_submission: RefCell<Option<NodeId>>,
}

impl DomBridge {
    fn mark_dirty(&self) {
        self.dirty.set(true);
    }

    /// Schedule a deterministic timer task, returning its numeric id.
    fn schedule_timer(&self, callback: JsValue) -> MochaResult<JsValue> {
        if !matches!(callback, JsValue::Function(_)) {
            return Err(MochaError::JavaScript(
                "setTimeout requires a function callback".to_string(),
            ));
        }
        let id = self.next_timer_id.get();
        self.next_timer_id.set(id + 1);
        self.timers.borrow_mut().push(TimerTask {
            id,
            callback,
            canceled: false,
        });
        Ok(JsValue::Number(id as f64))
    }

    /// Cancel a scheduled timer by its numeric id (ignored if `NaN`/unknown).
    fn cancel_timer(&self, id: f64) {
        if id.is_nan() {
            return;
        }
        let id = id as u64;
        for task in self.timers.borrow_mut().iter_mut() {
            if task.id == id {
                task.canceled = true;
            }
        }
    }
}

struct JsListener {
    node: NodeId,
    event_type: String,
    capture: bool,
    callback: JsValue,
}

struct TimerTask {
    id: u64,
    callback: JsValue,
    canceled: bool,
}

/// Mutable event state shared between an [`EventHost`] and the dispatcher.
struct EventState {
    event_type: String,
    target: NodeId,
    current_target: NodeId,
    default_prevented: bool,
    propagation_stopped: bool,
    immediate_stopped: bool,
}

// === the runtime ============================================================

/// Owns the interpreter and the DOM bridge for one document render.
pub struct DomRuntime {
    interp: Interpreter,
    bridge: Rc<DomBridge>,
}

impl DomRuntime {
    /// Build a runtime over the shared `document`, installing `window`/`document`/
    /// `console` globals. The caller keeps its own clone of the `Rc<RefCell<…>>`
    /// to read the (mutated) document after scripts run.
    pub fn new(document: Rc<RefCell<Document>>) -> DomRuntime {
        let bridge = Rc::new(DomBridge {
            doc: document,
            forms: Rc::new(RefCell::new(FormState::new())),
            listeners: RefCell::new(Vec::new()),
            timers: RefCell::new(Vec::new()),
            next_timer_id: Cell::new(0),
            dirty: Cell::new(false),
            pending_submission: RefCell::new(None),
        });
        let mut interp = Interpreter::new();
        // `console` is already a global from the interpreter's built-ins; expose it
        // through `window.console` too.
        let console = interp.global_get("console").unwrap_or(JsValue::Undefined);
        let document = JsValue::Host(Rc::new(DocumentHost {
            bridge: bridge.clone(),
        }));
        let window = JsValue::Host(Rc::new(WindowHost {
            bridge: bridge.clone(),
            document: document.clone(),
            console,
        }));
        interp.define_global("document", document);
        interp.define_global("window", window);

        // `setTimeout`/`clearTimeout` are also bare globals (not just `window`
        // methods). They capture the bridge through a native closure.
        let timer_bridge = bridge.clone();
        interp.define_global(
            "setTimeout",
            JsValue::native_closure("setTimeout", move |_, args| {
                timer_bridge.schedule_timer(args.first().cloned().unwrap_or(JsValue::Undefined))
            }),
        );
        let clear_bridge = bridge.clone();
        interp.define_global(
            "clearTimeout",
            JsValue::native_closure("clearTimeout", move |_, args| {
                clear_bridge.cancel_timer(args.first().map(JsValue::to_number).unwrap_or(f64::NAN));
                Ok(JsValue::Undefined)
            }),
        );

        DomRuntime { interp, bridge }
    }

    /// Parse and run one inline script against the shared document. A parse or
    /// runtime error aborts with a clear [`MochaError`].
    pub fn run_script(&mut self, source: &str) -> MochaResult<()> {
        let program = mocha_js::parse(source)?;
        self.interp.run(&program)?;
        Ok(())
    }

    /// Drain the deterministic timer queue (set via `setTimeout`) in insertion
    /// order, skipping cancelled tasks. There is no real clock; this is intended
    /// to be called once after all inline scripts have run.
    pub fn run_pending_timers(&mut self) -> MochaResult<()> {
        let mut ran = 0usize;
        loop {
            let task = {
                let mut timers = self.bridge.timers.borrow_mut();
                timers
                    .iter()
                    .position(|t| !t.canceled)
                    .map(|pos| timers.remove(pos))
            };
            let Some(task) = task else { break };
            self.interp.call_function(task.callback, Vec::new())?;
            ran += 1;
            if ran > 100_000 {
                return Err(MochaError::JavaScript(
                    "timer queue did not drain (too many scheduled tasks)".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Dispatch a JavaScript event of `event_type` at `target` through the DOM
    /// flow (capturing → at-target → bubbling), invoking matching JS listeners.
    /// Returns `true` when the default action should proceed (i.e. it was not
    /// prevented).
    pub fn dispatch_event(&mut self, event_type: &str, target: NodeId) -> MochaResult<bool> {
        let state = Rc::new(RefCell::new(EventState {
            event_type: event_type.to_string(),
            target,
            current_target: target,
            default_prevented: false,
            propagation_stopped: false,
            immediate_stopped: false,
        }));
        let event_value = JsValue::Host(Rc::new(EventHost {
            state: state.clone(),
            bridge: self.bridge.clone(),
        }));

        let ancestors = self.bridge.doc.borrow().ancestors(target)?; // parent → root

        // Capturing: root → parent.
        for &node in ancestors.iter().rev() {
            if state.borrow().propagation_stopped {
                break;
            }
            self.invoke_listeners(node, true, &state, &event_value)?;
        }

        // At target: capture-registered then bubble-registered listeners.
        if !state.borrow().propagation_stopped {
            self.invoke_listeners(target, true, &state, &event_value)?;
            if !state.borrow().immediate_stopped {
                self.invoke_listeners(target, false, &state, &event_value)?;
            }
        }

        // Bubbling: parent → root.
        for &node in ancestors.iter() {
            if state.borrow().propagation_stopped {
                break;
            }
            self.invoke_listeners(node, false, &state, &event_value)?;
        }

        let prevented = state.borrow().default_prevented;
        Ok(!prevented)
    }

    fn invoke_listeners(
        &mut self,
        node: NodeId,
        capture: bool,
        state: &Rc<RefCell<EventState>>,
        event_value: &JsValue,
    ) -> MochaResult<()> {
        let event_type = state.borrow().event_type.clone();
        // Snapshot matching callbacks so listeners added/removed during a callback
        // do not change this dispatch's set.
        let callbacks: Vec<JsValue> = self
            .bridge
            .listeners
            .borrow()
            .iter()
            .filter(|l| l.node == node && l.capture == capture && l.event_type == event_type)
            .map(|l| l.callback.clone())
            .collect();
        for callback in callbacks {
            if state.borrow().immediate_stopped {
                break;
            }
            state.borrow_mut().current_target = node;
            self.interp
                .call_function(callback, vec![event_value.clone()])?;
        }
        Ok(())
    }

    /// Take (and clear) `console.log` output captured during script execution.
    pub fn take_console_output(&mut self) -> Vec<String> {
        self.interp.take_console_output()
    }

    /// Whether any script mutated the DOM (coarse invalidation signal).
    pub fn is_dirty(&self) -> bool {
        self.bridge.dirty.get()
    }

    /// The shared form-control state. JS property reads/writes on controls go
    /// through this state; the embedder uses the same handle for layout, paint,
    /// and submission.
    pub fn form_state(&self) -> Rc<RefCell<FormState>> {
        self.bridge.forms.clone()
    }

    /// Eagerly initialize form state for every control currently in the
    /// document. Unsupported control types (e.g. `<input type="date">`) error
    /// here, before any script runs. Controls created later (e.g. via
    /// `innerHTML`) are initialized lazily on first access.
    pub fn init_form_state(&self) -> MochaResult<()> {
        let doc = self.bridge.doc.borrow();
        self.bridge.forms.borrow_mut().ensure_document(&doc)
    }

    /// Take (and clear) the form a script requested to submit via
    /// `form.submit()`, if any. The first request wins — in a real browser the
    /// first submission would have navigated away.
    pub fn take_pending_submission(&mut self) -> Option<NodeId> {
        self.bridge.pending_submission.borrow_mut().take()
    }
}

// === host objects ===========================================================

/// `window`.
struct WindowHost {
    bridge: Rc<DomBridge>,
    document: JsValue,
    console: JsValue,
}

impl HostObject for WindowHost {
    fn class_name(&self) -> &str {
        "Window"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, _: &mut Interpreter, name: &str) -> MochaResult<JsValue> {
        Ok(match name {
            "document" => self.document.clone(),
            "console" => self.console.clone(),
            _ => JsValue::Undefined,
        })
    }
    fn set(&self, _: &mut Interpreter, _: &str, _: JsValue) -> MochaResult<()> {
        Ok(())
    }
    fn call(&self, _: &mut Interpreter, name: &str, args: Vec<JsValue>) -> MochaResult<JsValue> {
        match name {
            "setTimeout" => self
                .bridge
                .schedule_timer(args.first().cloned().unwrap_or(JsValue::Undefined)),
            "clearTimeout" => {
                self.bridge
                    .cancel_timer(args.first().map(JsValue::to_number).unwrap_or(f64::NAN));
                Ok(JsValue::Undefined)
            }
            other => Err(MochaError::JavaScript(format!(
                "window has no method '{other}'"
            ))),
        }
    }
}

/// `document`.
struct DocumentHost {
    bridge: Rc<DomBridge>,
}

impl HostObject for DocumentHost {
    fn class_name(&self) -> &str {
        "Document"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, _: &mut Interpreter, name: &str) -> MochaResult<JsValue> {
        let found = match name {
            "body" => mocha_style::query_selector(&self.bridge.doc.borrow(), "body")?,
            "documentElement" => mocha_style::query_selector(&self.bridge.doc.borrow(), "html")?,
            _ => return Ok(JsValue::Undefined),
        };
        Ok(found
            .map(|node| node_host(&self.bridge, node))
            .unwrap_or(JsValue::Null))
    }
    fn set(&self, _: &mut Interpreter, _: &str, _: JsValue) -> MochaResult<()> {
        Ok(())
    }
    fn call(&self, _: &mut Interpreter, name: &str, args: Vec<JsValue>) -> MochaResult<JsValue> {
        match name {
            "getElementById" => {
                let id = arg_str(&args, 0);
                let found = self.bridge.doc.borrow().get_element_by_id(&id)?;
                Ok(found
                    .map(|node| node_host(&self.bridge, node))
                    .unwrap_or(JsValue::Null))
            }
            "querySelector" => {
                let selector = arg_str(&args, 0);
                let found = mocha_style::query_selector(&self.bridge.doc.borrow(), &selector)?;
                Ok(found
                    .map(|node| node_host(&self.bridge, node))
                    .unwrap_or(JsValue::Null))
            }
            "querySelectorAll" => {
                let selector = arg_str(&args, 0);
                let nodes = mocha_style::query_selector_all(&self.bridge.doc.borrow(), &selector)?;
                Ok(JsValue::array(
                    nodes
                        .into_iter()
                        .map(|node| node_host(&self.bridge, node))
                        .collect(),
                ))
            }
            "createElement" => {
                let tag = arg_str(&args, 0).to_ascii_lowercase();
                if !CREATABLE_TAGS.contains(&tag.as_str()) {
                    return Err(MochaError::UnsupportedFeature(format!(
                        "document.createElement('{tag}') is not supported"
                    )));
                }
                let node = self.bridge.doc.borrow_mut().create_element(tag, Vec::new());
                Ok(node_host(&self.bridge, node))
            }
            "createTextNode" => {
                let text = arg_str(&args, 0);
                let node = self.bridge.doc.borrow_mut().create_text(text);
                Ok(node_host(&self.bridge, node))
            }
            other => Err(MochaError::JavaScript(format!(
                "document has no method '{other}'"
            ))),
        }
    }
}

/// A DOM node (`Element` or `Text`) exposed to JavaScript.
struct NodeHost {
    bridge: Rc<DomBridge>,
    node: NodeId,
}

impl NodeHost {
    /// Read a form-control property (`value`, `checked`, `selected`, `disabled`,
    /// `type`, `name`, `selectedIndex`) from the shared [`FormState`]. Returns
    /// `Ok(None)` when `name` is not a form property or this node is not a form
    /// control, so the caller falls through to the generic node properties.
    fn form_get(&self, name: &str) -> MochaResult<Option<JsValue>> {
        if !matches!(
            name,
            "value" | "checked" | "selected" | "disabled" | "type" | "name" | "selectedIndex"
        ) {
            return Ok(None);
        }
        let doc = self.bridge.doc.borrow();
        let mut forms = self.bridge.forms.borrow_mut();

        // A select's value/selectedIndex are derived from its options.
        if doc.tag_name(self.node)? == Some("select") {
            match name {
                "value" => {
                    let value = mocha_forms::select_value(&doc, &mut forms, self.node)?;
                    return Ok(Some(JsValue::Str(value.unwrap_or_default())));
                }
                "selectedIndex" => {
                    let index = mocha_forms::selected_index(&doc, &mut forms, self.node)?;
                    return Ok(Some(JsValue::Number(
                        index.map(|i| i as f64).unwrap_or(-1.0),
                    )));
                }
                _ => {}
            }
        }

        let Some(control) = forms.ensure_control(&doc, self.node)? else {
            return Ok(None);
        };
        Ok(match name {
            "value" => Some(JsValue::Str(control.value.clone())),
            "checked" => Some(JsValue::Bool(control.checked)),
            "selected" => Some(JsValue::Bool(control.selected)),
            "disabled" => Some(JsValue::Bool(control.disabled)),
            "type" => Some(JsValue::Str(control.kind.as_str().to_string())),
            "name" => Some(JsValue::Str(control.name.clone().unwrap_or_default())),
            // selectedIndex is only meaningful on a select.
            _ => None,
        })
    }

    /// Write a form-control property to the shared [`FormState`]. Returns
    /// `Ok(true)` when the write was handled (the caller marks the document
    /// dirty), `Ok(false)` to fall through.
    fn form_set(&self, name: &str, value: &JsValue) -> MochaResult<bool> {
        if !matches!(
            name,
            "value" | "checked" | "selected" | "disabled" | "selectedIndex"
        ) {
            return Ok(false);
        }
        let doc = self.bridge.doc.borrow();
        let mut forms = self.bridge.forms.borrow_mut();

        if doc.tag_name(self.node)? == Some("select") {
            match name {
                "value" => {
                    mocha_forms::set_select_value(&doc, &mut forms, self.node, &value.stringify())?;
                    return Ok(true);
                }
                "selectedIndex" => {
                    let number = value.to_number();
                    let index = if number.is_finite() && number >= 0.0 {
                        Some(number as usize)
                    } else {
                        None
                    };
                    mocha_forms::set_selected_index(&doc, &mut forms, self.node, index)?;
                    return Ok(true);
                }
                _ => {}
            }
        }

        // Setting `checked = true` on a radio also unchecks its group.
        if name == "checked" {
            let Some(kind) = forms.ensure_control(&doc, self.node)?.map(|c| c.kind) else {
                return Ok(false);
            };
            let truthy = value.is_truthy();
            if kind == ControlKind::Radio && truthy {
                mocha_forms::select_radio(&doc, &mut forms, self.node)?;
            } else if let Some(control) = forms.control_mut(self.node) {
                control.checked = truthy;
            }
            return Ok(true);
        }

        let Some(control) = forms.ensure_control(&doc, self.node)? else {
            return Ok(false);
        };
        match name {
            "value" => control.value = value.stringify(),
            "selected" => control.selected = value.is_truthy(),
            "disabled" => control.disabled = value.is_truthy(),
            // selectedIndex is only meaningful on a select.
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn set_inner_html(&self, html: &str) -> MochaResult<()> {
        let fragment = mocha_html::parse_html(html)?;
        let mut doc = self.bridge.doc.borrow_mut();
        let existing: Vec<NodeId> = doc.children(self.node)?.to_vec();
        for child in existing {
            doc.remove_child(self.node, child)?;
        }
        let frag_children: Vec<NodeId> = fragment.children(fragment.root_id())?.to_vec();
        for child in frag_children {
            let imported = import_node(&mut doc, &fragment, child)?;
            doc.append_child(self.node, imported)?;
        }
        Ok(())
    }
}

impl HostObject for NodeHost {
    fn class_name(&self) -> &str {
        "Node"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, _: &mut Interpreter, name: &str) -> MochaResult<JsValue> {
        if let Some(value) = self.form_get(name)? {
            return Ok(value);
        }
        let doc = self.bridge.doc.borrow();
        Ok(match name {
            "textContent" => JsValue::Str(doc.text_content(self.node)?),
            "innerHTML" => JsValue::Str(serialize_children(&doc, self.node)?),
            "id" => JsValue::Str(
                doc.get_attribute_owned(self.node, "id")?
                    .unwrap_or_default(),
            ),
            "className" => JsValue::Str(
                doc.get_attribute_owned(self.node, "class")?
                    .unwrap_or_default(),
            ),
            "tagName" | "nodeName" => doc
                .tag_name(self.node)?
                .map(|tag| JsValue::Str(tag.to_ascii_uppercase()))
                .unwrap_or(JsValue::Undefined),
            _ => JsValue::Undefined,
        })
    }
    fn set(&self, _: &mut Interpreter, name: &str, value: JsValue) -> MochaResult<()> {
        if self.form_set(name, &value)? {
            // A control's painted value/checked/disabled state changed.
            self.bridge.mark_dirty();
            return Ok(());
        }
        match name {
            "textContent" => {
                self.bridge
                    .doc
                    .borrow_mut()
                    .set_text_content(self.node, value.stringify())?;
                self.bridge.mark_dirty();
            }
            "innerHTML" => {
                self.set_inner_html(&value.stringify())?;
                self.bridge.mark_dirty();
            }
            "id" => {
                self.bridge
                    .doc
                    .borrow_mut()
                    .set_attribute(self.node, "id", value.stringify())?;
                self.bridge.mark_dirty();
            }
            "className" => {
                self.bridge.doc.borrow_mut().set_attribute(
                    self.node,
                    "class",
                    value.stringify(),
                )?;
                self.bridge.mark_dirty();
            }
            // Unknown ("expando") properties are not persisted onto the DOM.
            _ => {}
        }
        Ok(())
    }
    fn call(&self, _: &mut Interpreter, name: &str, args: Vec<JsValue>) -> MochaResult<JsValue> {
        match name {
            "submit" => {
                if self.bridge.doc.borrow().tag_name(self.node)? != Some("form") {
                    return Err(MochaError::JavaScript(
                        "submit() is only supported on <form> elements".to_string(),
                    ));
                }
                let mut pending = self.bridge.pending_submission.borrow_mut();
                if pending.is_none() {
                    *pending = Some(self.node);
                }
                Ok(JsValue::Undefined)
            }
            "getAttribute" => {
                let attr = arg_str(&args, 0);
                let value = self
                    .bridge
                    .doc
                    .borrow()
                    .get_attribute_owned(self.node, &attr)?;
                Ok(value.map(JsValue::Str).unwrap_or(JsValue::Null))
            }
            "setAttribute" => {
                let attr = arg_str(&args, 0);
                let value = arg_str(&args, 1);
                self.bridge
                    .doc
                    .borrow_mut()
                    .set_attribute(self.node, attr, value)?;
                self.bridge.mark_dirty();
                Ok(JsValue::Undefined)
            }
            "appendChild" => {
                let child_value = args.first().cloned().unwrap_or(JsValue::Undefined);
                let child = node_id_of(&child_value).ok_or_else(|| {
                    MochaError::JavaScript("appendChild expects a DOM node".to_string())
                })?;
                {
                    let mut doc = self.bridge.doc.borrow_mut();
                    if let Some(parent) = doc.parent(child)? {
                        doc.remove_child(parent, child)?;
                    }
                    doc.append_child(self.node, child)?;
                }
                self.bridge.mark_dirty();
                Ok(child_value)
            }
            "removeChild" => {
                let child_value = args.first().cloned().unwrap_or(JsValue::Undefined);
                let child = node_id_of(&child_value).ok_or_else(|| {
                    MochaError::JavaScript("removeChild expects a DOM node".to_string())
                })?;
                self.bridge
                    .doc
                    .borrow_mut()
                    .remove_child(self.node, child)?;
                self.bridge.mark_dirty();
                Ok(child_value)
            }
            "addEventListener" => {
                let event_type = arg_str(&args, 0);
                let callback = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !matches!(callback, JsValue::Function(_)) {
                    return Err(MochaError::JavaScript(
                        "addEventListener requires a function listener".to_string(),
                    ));
                }
                let capture = args.get(2).map(JsValue::is_truthy).unwrap_or(false);
                self.bridge.listeners.borrow_mut().push(JsListener {
                    node: self.node,
                    event_type,
                    capture,
                    callback,
                });
                Ok(JsValue::Undefined)
            }
            "removeEventListener" => {
                let event_type = arg_str(&args, 0);
                let callback = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let capture = args.get(2).map(JsValue::is_truthy).unwrap_or(false);
                let mut listeners = self.bridge.listeners.borrow_mut();
                if let Some(pos) = listeners.iter().position(|l| {
                    l.node == self.node
                        && l.capture == capture
                        && l.event_type == event_type
                        && l.callback.strict_equals(&callback)
                }) {
                    listeners.remove(pos);
                }
                Ok(JsValue::Undefined)
            }
            other => Err(MochaError::JavaScript(format!(
                "node has no method '{other}'"
            ))),
        }
    }
}

/// The `event` object passed to a JavaScript listener during dispatch.
struct EventHost {
    state: Rc<RefCell<EventState>>,
    bridge: Rc<DomBridge>,
}

impl HostObject for EventHost {
    fn class_name(&self) -> &str {
        "Event"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, _: &mut Interpreter, name: &str) -> MochaResult<JsValue> {
        let state = self.state.borrow();
        Ok(match name {
            "type" => JsValue::Str(state.event_type.clone()),
            "target" => node_host(&self.bridge, state.target),
            "currentTarget" => node_host(&self.bridge, state.current_target),
            "defaultPrevented" => JsValue::Bool(state.default_prevented),
            _ => JsValue::Undefined,
        })
    }
    fn set(&self, _: &mut Interpreter, _: &str, _: JsValue) -> MochaResult<()> {
        Ok(())
    }
    fn call(&self, _: &mut Interpreter, name: &str, _: Vec<JsValue>) -> MochaResult<JsValue> {
        let mut state = self.state.borrow_mut();
        match name {
            "preventDefault" => state.default_prevented = true,
            "stopPropagation" => state.propagation_stopped = true,
            "stopImmediatePropagation" => {
                state.propagation_stopped = true;
                state.immediate_stopped = true;
            }
            other => {
                return Err(MochaError::JavaScript(format!(
                    "event has no method '{other}'"
                )))
            }
        }
        Ok(JsValue::Undefined)
    }
}

// === helpers ================================================================

fn node_host(bridge: &Rc<DomBridge>, node: NodeId) -> JsValue {
    JsValue::Host(Rc::new(NodeHost {
        bridge: bridge.clone(),
        node,
    }))
}

fn node_id_of(value: &JsValue) -> Option<NodeId> {
    match value {
        JsValue::Host(host) => host.as_any().downcast_ref::<NodeHost>().map(|n| n.node),
        _ => None,
    }
}

fn arg_str(args: &[JsValue], index: usize) -> String {
    args.get(index).map(JsValue::stringify).unwrap_or_default()
}

/// Deep-copy a node (and its subtree) from `src` into `dst`, returning the new id.
/// Used by `innerHTML` to graft a parsed fragment into the live document arena.
fn import_node(dst: &mut Document, src: &Document, src_id: NodeId) -> MochaResult<NodeId> {
    let new_id = match src.node(src_id)?.kind.clone() {
        NodeKind::Element(data) => dst.create_element(data.tag_name, data.attributes),
        NodeKind::Text(text) => dst.create_text(text.text),
        NodeKind::Comment(text) => dst.create_comment(text),
        NodeKind::Doctype(text) => dst.create_doctype(text),
        NodeKind::Document => {
            return Err(MochaError::Dom("cannot import a document node".to_string()))
        }
    };
    let children: Vec<NodeId> = src.children(src_id)?.to_vec();
    for child in children {
        let imported = import_node(dst, src, child)?;
        dst.append_child(new_id, imported)?;
    }
    Ok(new_id)
}

/// A minimal serialization of an element's children (for the `innerHTML` getter).
fn serialize_children(doc: &Document, node: NodeId) -> MochaResult<String> {
    let mut out = String::new();
    for &child in doc.children(node)? {
        serialize_node(doc, child, &mut out)?;
    }
    Ok(out)
}

fn serialize_node(doc: &Document, node: NodeId, out: &mut String) -> MochaResult<()> {
    match &doc.node(node)?.kind {
        NodeKind::Element(data) => {
            out.push('<');
            out.push_str(&data.tag_name);
            for attribute in &data.attributes {
                out.push(' ');
                out.push_str(&attribute.name);
                out.push_str("=\"");
                out.push_str(&attribute.value);
                out.push('"');
            }
            out.push('>');
            let children: Vec<NodeId> = doc.children(node)?.to_vec();
            for child in children {
                serialize_node(doc, child, out)?;
            }
            out.push_str("</");
            out.push_str(&data.tag_name);
            out.push('>');
        }
        NodeKind::Text(text) => out.push_str(&text.text),
        NodeKind::Comment(text) => {
            out.push_str("<!--");
            out.push_str(text);
            out.push_str("-->");
        }
        NodeKind::Doctype(_) | NodeKind::Document => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_from(html: &str) -> Rc<RefCell<Document>> {
        Rc::new(RefCell::new(mocha_html::parse_html(html).unwrap()))
    }

    /// Parse, run all inline scripts and pending timers, returning the shared
    /// document and the (still-alive) runtime for further inspection/dispatch.
    fn run(html: &str) -> (Rc<RefCell<Document>>, DomRuntime) {
        let doc = doc_from(html);
        let scripts = collect_inline_scripts(&doc.borrow()).unwrap();
        let mut runtime = DomRuntime::new(doc.clone());
        for source in &scripts {
            runtime.run_script(source).unwrap();
        }
        runtime.run_pending_timers().unwrap();
        (doc, runtime)
    }

    fn text_of(doc: &Rc<RefCell<Document>>, id: &str) -> String {
        let doc = doc.borrow();
        let node = doc.get_element_by_id(id).unwrap().unwrap();
        doc.text_content(node).unwrap()
    }

    #[test]
    fn inline_script_changes_text_content() {
        let (doc, _rt) = run(r#"<html><body><h1 id="t">Before</h1>
               <script>document.getElementById("t").textContent = "After";</script>
               </body></html>"#);
        assert_eq!(text_of(&doc, "t"), "After");
    }

    #[test]
    fn script_creates_and_appends_element() {
        let (doc, _rt) = run(r#"<html><body id="b">
               <script>
                 let p = document.createElement("p");
                 p.textContent = "Created";
                 document.body.appendChild(p);
               </script></body></html>"#);
        let doc = doc.borrow();
        let body = doc.get_element_by_id("b").unwrap().unwrap();
        let last = *doc.children(body).unwrap().last().unwrap();
        assert_eq!(doc.tag_name(last).unwrap(), Some("p"));
        assert_eq!(doc.text_content(last).unwrap(), "Created");
    }

    #[test]
    fn set_attribute_and_class_and_id_are_observable() {
        let (doc, _rt) = run(r#"<html><body><p id="n">x</p>
               <script>
                 let n = document.getElementById("n");
                 n.setAttribute("style", "color: red;");
                 n.className = "note";
               </script></body></html>"#);
        let doc = doc.borrow();
        let n = doc.get_element_by_id("n").unwrap().unwrap();
        assert_eq!(
            doc.get_attribute_owned(n, "style").unwrap().as_deref(),
            Some("color: red;")
        );
        assert_eq!(
            doc.get_attribute_owned(n, "class").unwrap().as_deref(),
            Some("note")
        );
    }

    #[test]
    fn get_attribute_query_selector_and_create_text_node() {
        let (doc, _rt) = run(
            r#"<html><body><a id="l" href="/next">L</a><p class="c">one</p><p class="c">two</p>
               <script>
                 let result = document.getElementById("l").getAttribute("href");
                 let all = document.querySelectorAll("p.c");
                 let first = document.querySelector(".c");
                 first.appendChild(document.createTextNode(" + " + result + " + " + all.length));
               </script></body></html>"#,
        );
        let doc = doc.borrow();
        let first = mocha_style::query_selector(&doc, ".c").unwrap().unwrap();
        assert_eq!(doc.text_content(first).unwrap(), "one + /next + 2");
    }

    #[test]
    fn remove_child_detaches_node() {
        let (doc, _rt) = run(r#"<html><body id="b"><p id="gone">x</p>
               <script>
                 let p = document.getElementById("gone");
                 document.body.removeChild(p);
               </script></body></html>"#);
        let doc = doc.borrow();
        let body = doc.get_element_by_id("b").unwrap().unwrap();
        // The body still holds its <script>, but the removed <p> is detached and
        // is no longer among the body's children.
        assert_eq!(doc.get_element_by_id("gone").unwrap(), None);
        for &child in doc.children(body).unwrap() {
            assert_ne!(doc.tag_name(child).unwrap(), Some("p"));
        }
    }

    #[test]
    fn inner_html_setter_and_getter_round_trip() {
        let (doc, _rt) = run(r#"<html><body><div id="d">old</div>
               <script>document.getElementById("d").innerHTML = "<span>hi</span>";</script>
               </body></html>"#);
        let doc = doc.borrow();
        let div = doc.get_element_by_id("d").unwrap().unwrap();
        let child = doc.children(div).unwrap()[0];
        assert_eq!(doc.tag_name(child).unwrap(), Some("span"));
        assert_eq!(serialize_children(&doc, div).unwrap(), "<span>hi</span>");
    }

    #[test]
    fn js_event_listener_runs_and_prevent_default_is_reported() {
        let doc = doc_from(
            r#"<html><body><a id="link" href="/next">Click</a>
               <script>
                 let link = document.getElementById("link");
                 link.addEventListener("click", function (event) {
                   event.preventDefault();
                   link.textContent = "Clicked";
                 });
               </script></body></html>"#,
        );
        let scripts = collect_inline_scripts(&doc.borrow()).unwrap();
        let mut runtime = DomRuntime::new(doc.clone());
        for source in &scripts {
            runtime.run_script(source).unwrap();
        }
        let link = doc.borrow().get_element_by_id("link").unwrap().unwrap();
        let proceed = runtime.dispatch_event("click", link).unwrap();
        assert!(
            !proceed,
            "preventDefault should suppress the default action"
        );
        assert_eq!(doc.borrow().text_content(link).unwrap(), "Clicked");
    }

    #[test]
    fn remove_event_listener_stops_future_dispatch() {
        let doc = doc_from(r#"<html><body><a id="x">b</a></body></html>"#);
        let mut runtime = DomRuntime::new(doc.clone());
        runtime
            .run_script(
                r#"let x = document.getElementById("x");
                   function handler() { x.textContent = "fired"; }
                   x.addEventListener("click", handler);
                   x.removeEventListener("click", handler);"#,
            )
            .unwrap();
        let x = doc.borrow().get_element_by_id("x").unwrap().unwrap();
        runtime.dispatch_event("click", x).unwrap();
        assert_eq!(doc.borrow().text_content(x).unwrap(), "b");
    }

    #[test]
    fn set_timeout_mutates_dom_and_clear_timeout_prevents_it() {
        let (doc, _rt) = run(r#"<html><body><p id="p">start</p>
               <script>
                 let p = document.getElementById("p");
                 let keep = setTimeout(function () { p.textContent = "ran"; }, 0);
                 let drop = setTimeout(function () { p.textContent = "should not run"; }, 0);
                 clearTimeout(drop);
               </script></body></html>"#);
        assert_eq!(text_of(&doc, "p"), "ran");
    }

    #[test]
    fn timers_run_in_insertion_order() {
        let doc = doc_from(r#"<html><body><p id="p"></p></body></html>"#);
        let mut runtime = DomRuntime::new(doc.clone());
        runtime
            .run_script(
                r#"let p = document.getElementById("p");
                   setTimeout(function () { p.textContent = p.textContent + "a"; }, 0);
                   setTimeout(function () { p.textContent = p.textContent + "b"; }, 0);
                   setTimeout(function () { p.textContent = p.textContent + "c"; }, 0);"#,
            )
            .unwrap();
        runtime.run_pending_timers().unwrap();
        let p = doc.borrow().get_element_by_id("p").unwrap().unwrap();
        assert_eq!(doc.borrow().text_content(p).unwrap(), "abc");
    }

    #[test]
    fn window_document_identity_and_console_capture() {
        let doc = doc_from(r#"<html><body><p id="p">x</p></body></html>"#);
        let mut runtime = DomRuntime::new(doc.clone());
        runtime
            .run_script(
                r#"console.log("hi", 1);
                   let answer = "no";
                   if (window.document === document) { answer = "yes"; }
                   document.getElementById("p").textContent = answer;"#,
            )
            .unwrap();
        assert_eq!(runtime.take_console_output(), vec!["hi 1".to_string()]);
        let p = doc.borrow().get_element_by_id("p").unwrap().unwrap();
        assert_eq!(doc.borrow().text_content(p).unwrap(), "yes");
    }

    // --- Milestone 10: form bindings -----------------------------------------

    /// Like `run`, but initializes form state eagerly first (the shell pipeline
    /// order) and returns the runtime for state inspection.
    fn run_with_forms(html: &str) -> (Rc<RefCell<Document>>, DomRuntime) {
        let doc = doc_from(html);
        let scripts = collect_inline_scripts(&doc.borrow()).unwrap();
        let mut runtime = DomRuntime::new(doc.clone());
        runtime.init_form_state().unwrap();
        for source in &scripts {
            runtime.run_script(source).unwrap();
        }
        runtime.run_pending_timers().unwrap();
        (doc, runtime)
    }

    #[test]
    fn js_reads_input_value_type_and_name() {
        let (doc, _rt) = run_with_forms(
            r#"<html><body><form><input id="q" name="q" value="mocha"></form><p id="out"></p>
               <script>
                 let q = document.getElementById("q");
                 document.getElementById("out").textContent = q.value + "/" + q.type + "/" + q.name;
               </script></body></html>"#,
        );
        assert_eq!(text_of(&doc, "out"), "mocha/text/q");
    }

    #[test]
    fn js_sets_input_value_in_form_state() {
        let (doc, rt) = run_with_forms(
            r#"<html><body><form><input id="q" name="q" value="Before"></form>
               <script>document.getElementById("q").value = "After";</script></body></html>"#,
        );
        let node = doc.borrow().get_element_by_id("q").unwrap().unwrap();
        let forms = rt.form_state();
        let forms = forms.borrow();
        assert_eq!(forms.control(node).unwrap().value, "After");
        // The DOM attribute is untouched; only the dynamic state changed.
        assert_eq!(
            doc.borrow()
                .get_attribute_owned(node, "value")
                .unwrap()
                .as_deref(),
            Some("Before")
        );
    }

    #[test]
    fn js_toggles_checkbox_checked() {
        let (doc, rt) = run_with_forms(
            r#"<html><body><form><input id="c" type="checkbox" name="agree"></form>
               <script>
                 let c = document.getElementById("c");
                 c.checked = !c.checked;
               </script></body></html>"#,
        );
        let node = doc.borrow().get_element_by_id("c").unwrap().unwrap();
        assert!(rt.form_state().borrow().control(node).unwrap().checked);
    }

    #[test]
    fn js_radio_checked_setter_unchecks_the_group() {
        let (doc, rt) = run_with_forms(
            r#"<html><body><form>
                 <input id="s" type="radio" name="size" value="small">
                 <input id="l" type="radio" name="size" value="large" checked>
               </form>
               <script>document.getElementById("s").checked = true;</script></body></html>"#,
        );
        let small = doc.borrow().get_element_by_id("s").unwrap().unwrap();
        let large = doc.borrow().get_element_by_id("l").unwrap().unwrap();
        let forms = rt.form_state();
        let forms = forms.borrow();
        assert!(forms.control(small).unwrap().checked);
        assert!(!forms.control(large).unwrap().checked);
    }

    #[test]
    fn js_reads_and_sets_textarea_value() {
        let (doc, rt) = run_with_forms(
            r#"<html><body><form><textarea id="m" name="m">Hello Mocha</textarea></form><p id="out"></p>
               <script>
                 let m = document.getElementById("m");
                 document.getElementById("out").textContent = m.value;
                 m.value = "Edited";
               </script></body></html>"#,
        );
        assert_eq!(text_of(&doc, "out"), "Hello Mocha");
        let node = doc.borrow().get_element_by_id("m").unwrap().unwrap();
        assert_eq!(
            rt.form_state().borrow().control(node).unwrap().value,
            "Edited"
        );
    }

    #[test]
    fn js_select_value_and_selected_index() {
        let (doc, _rt) = run_with_forms(
            r#"<html><body><form><select id="s" name="c">
                 <option value="a">Alpha</option>
                 <option value="b" selected>Beta</option>
               </select></form><p id="out"></p>
               <script>
                 let s = document.getElementById("s");
                 let before = s.value + "/" + s.selectedIndex;
                 s.value = "a";
                 let after = s.value + "/" + s.selectedIndex;
                 s.selectedIndex = 1;
                 document.getElementById("out").textContent = before + " " + after + " " + s.value;
               </script></body></html>"#,
        );
        assert_eq!(text_of(&doc, "out"), "b/1 a/0 b");
    }

    #[test]
    fn js_reads_and_sets_disabled() {
        let (doc, rt) = run_with_forms(
            r#"<html><body><form><input id="a" name="a" disabled><input id="b" name="b"></form><p id="out"></p>
               <script>
                 document.getElementById("out").textContent = "" + document.getElementById("a").disabled;
                 document.getElementById("b").disabled = true;
               </script></body></html>"#,
        );
        assert_eq!(text_of(&doc, "out"), "true");
        let node = doc.borrow().get_element_by_id("b").unwrap().unwrap();
        assert!(rt.form_state().borrow().control(node).unwrap().disabled);
    }

    #[test]
    fn form_submit_records_a_pending_submission() {
        let (doc, mut rt) = run_with_forms(
            r#"<html><body><form id="f" action="/go"><input name="q" value="x"></form>
               <script>document.getElementById("f").submit();</script></body></html>"#,
        );
        let form = doc.borrow().get_element_by_id("f").unwrap().unwrap();
        assert_eq!(rt.take_pending_submission(), Some(form));
        assert_eq!(
            rt.take_pending_submission(),
            None,
            "take clears the request"
        );
    }

    #[test]
    fn submit_on_a_non_form_node_errors() {
        let doc = doc_from(r#"<html><body><p id="p">x</p></body></html>"#);
        let mut runtime = DomRuntime::new(doc);
        let error = runtime
            .run_script(r#"document.getElementById("p").submit();"#)
            .unwrap_err();
        assert!(matches!(error, MochaError::JavaScript(_)));
    }

    #[test]
    fn form_properties_on_non_controls_are_undefined() {
        let (doc, _rt) = run_with_forms(
            r#"<html><body><p id="p">x</p><p id="out"></p>
               <script>
                 let p = document.getElementById("p");
                 document.getElementById("out").textContent = "" + p.value + "/" + p.checked;
               </script></body></html>"#,
        );
        assert_eq!(text_of(&doc, "out"), "undefined/undefined");
    }

    #[test]
    fn unsupported_input_type_fails_form_state_init() {
        let doc = doc_from(r#"<html><body><input type="date" name="d"></body></html>"#);
        let runtime = DomRuntime::new(doc);
        assert!(matches!(
            runtime.init_form_state().unwrap_err(),
            MochaError::UnsupportedFeature(_)
        ));
    }

    #[test]
    fn script_runtime_error_is_reported_clearly() {
        let doc = doc_from(r#"<html><body></body></html>"#);
        let mut runtime = DomRuntime::new(doc);
        let error = runtime.run_script("undefinedThing.foo();").unwrap_err();
        assert!(matches!(error, MochaError::JavaScript(_)));
    }

    #[test]
    fn external_script_is_unsupported_and_create_unknown_tag_errors() {
        let doc = doc_from(r#"<html><body><script src="app.js"></script></body></html>"#);
        let error = collect_inline_scripts(&doc.borrow()).unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));

        let doc = doc_from(r#"<html><body></body></html>"#);
        let mut runtime = DomRuntime::new(doc);
        let error = runtime
            .run_script(r#"document.createElement("marquee");"#)
            .unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }
}
