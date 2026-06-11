//! The form-control state model.
//!
//! Dynamic control state (current value, checked, selected, disabled) lives here,
//! **outside** the DOM: attributes on `mocha_dom` nodes stay as parsed, and a
//! [`FormState`] keyed by [`NodeId`] carries the values that JavaScript and user
//! actions can change. Attributes initialize the state once; later attribute
//! reads do not see state changes and vice versa.

use std::collections::HashMap;

use mocha_dom::{Document, NodeId, NodeKind};
use mocha_error::{MochaError, MochaResult};

/// The normalized kind of a form control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// `<input type="text">` (also the default for a missing `type`).
    Text,
    /// `<input type="password">`.
    Password,
    /// `<input type="checkbox">`.
    Checkbox,
    /// `<input type="radio">`.
    Radio,
    /// `<input type="submit">` or `<button type="submit">` (the button default).
    Submit,
    /// `<input type="reset">` or `<button type="reset">`.
    Reset,
    /// `<button type="button">`.
    Button,
    /// `<input type="hidden">`.
    Hidden,
    /// `<textarea>`.
    TextArea,
    /// `<select>`.
    Select,
    /// `<option>`.
    Option,
}

impl ControlKind {
    /// The normalized type string (what `input.type` returns in JavaScript).
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlKind::Text => "text",
            ControlKind::Password => "password",
            ControlKind::Checkbox => "checkbox",
            ControlKind::Radio => "radio",
            ControlKind::Submit => "submit",
            ControlKind::Reset => "reset",
            ControlKind::Button => "button",
            ControlKind::Hidden => "hidden",
            ControlKind::TextArea => "textarea",
            ControlKind::Select => "select",
            ControlKind::Option => "option",
        }
    }
}

/// The dynamic state of one form control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlState {
    /// The normalized control kind.
    pub kind: ControlKind,
    /// The `name` attribute, or `None` when absent (unnamed controls never
    /// submit).
    pub name: Option<String>,
    /// The current value. For checkboxes/radios with no `value` attribute this
    /// is `"on"` (the HTML default submission value).
    pub value: String,
    /// Whether a checkbox/radio is currently checked.
    pub checked: bool,
    /// Whether an option is currently selected.
    pub selected: bool,
    /// Whether the control is disabled.
    pub disabled: bool,
}

/// Dynamic state for every form control in a document, keyed by node id.
#[derive(Debug, Clone, Default)]
pub struct FormState {
    controls: HashMap<NodeId, ControlState>,
}

impl FormState {
    /// An empty state; controls are added by [`build_form_state`] or lazily by
    /// [`FormState::ensure_control`].
    pub fn new() -> FormState {
        FormState::default()
    }

    /// The state of a control, if it has been initialized.
    pub fn control(&self, node: NodeId) -> Option<&ControlState> {
        self.controls.get(&node)
    }

    /// Mutable access to an already-initialized control's state.
    pub fn control_mut(&mut self, node: NodeId) -> Option<&mut ControlState> {
        self.controls.get_mut(&node)
    }

    /// The number of tracked controls.
    pub fn len(&self) -> usize {
        self.controls.len()
    }

    /// Whether no controls are tracked.
    pub fn is_empty(&self) -> bool {
        self.controls.is_empty()
    }

    /// Ensure `node`'s control state exists, initializing it from the DOM
    /// attributes on first access (so controls inserted after the initial build,
    /// e.g. via `innerHTML`, behave like parsed ones).
    ///
    /// Returns `Ok(None)` when `node` is not a form-control element, and an
    /// [`MochaError::UnsupportedFeature`] error for unsupported `input`/`button`
    /// types.
    pub fn ensure_control(
        &mut self,
        document: &Document,
        node: NodeId,
    ) -> MochaResult<Option<&mut ControlState>> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.controls.entry(node) {
            let Some(initial) = initial_control_state(document, node)? else {
                return Ok(None);
            };
            entry.insert(initial);
        }
        Ok(self.controls.get_mut(&node))
    }

    /// Ensure state for every control currently in `document` (in document
    /// order) and normalize every `<select>`'s option selection: the last
    /// `selected` option wins; with none marked, the first option is selected
    /// (matching browser behaviour for single-select dropdowns).
    pub fn ensure_document(&mut self, document: &Document) -> MochaResult<()> {
        for node in document.traverse_depth_first(document.root_id())? {
            self.ensure_control(document, node)?;
            if document.tag_name(node)? == Some("select") {
                normalize_select(document, self, node)?;
            }
        }
        Ok(())
    }
}

/// Whether `tag` names a form-control element tracked by [`FormState`].
pub(crate) fn is_control_tag(tag: &str) -> bool {
    matches!(tag, "input" | "button" | "textarea" | "select" | "option")
}

/// Build the form state for every control in `document`, in document order
/// (see [`FormState::ensure_document`]).
pub fn build_form_state(document: &Document) -> MochaResult<FormState> {
    let mut state = FormState::new();
    state.ensure_document(document)?;
    Ok(state)
}

/// The nearest `<form>` ancestor of `node`, if any (form ownership; the HTML
/// `form` attribute is not supported).
pub fn owner_form(document: &Document, node: NodeId) -> MochaResult<Option<NodeId>> {
    for ancestor in document.ancestors(node)? {
        if document.tag_name(ancestor)? == Some("form") {
            return Ok(Some(ancestor));
        }
    }
    Ok(None)
}

/// The `<option>` children of a `<select>`, in document order. Only direct
/// children are considered (`<optgroup>` is unsupported and rejected by the
/// parser).
pub(crate) fn select_options(document: &Document, select: NodeId) -> MochaResult<Vec<NodeId>> {
    let mut options = Vec::new();
    for &child in document.children(select)? {
        if document.tag_name(child)? == Some("option") {
            options.push(child);
        }
    }
    Ok(options)
}

/// The index of the selected option of `select`, or `None` when the select has
/// no options. A select whose options are all unselected reports its first
/// option (the browser default for single-select dropdowns).
pub fn selected_index(
    document: &Document,
    state: &mut FormState,
    select: NodeId,
) -> MochaResult<Option<usize>> {
    let options = select_options(document, select)?;
    if options.is_empty() {
        return Ok(None);
    }
    for (index, &option) in options.iter().enumerate() {
        let selected = state
            .ensure_control(document, option)?
            .is_some_and(|control| control.selected);
        if selected {
            return Ok(Some(index));
        }
    }
    Ok(Some(0))
}

/// The current value of `select`: its selected option's value, or `None` when
/// the select has no options.
pub fn select_value(
    document: &Document,
    state: &mut FormState,
    select: NodeId,
) -> MochaResult<Option<String>> {
    let options = select_options(document, select)?;
    match selected_index(document, state, select)? {
        Some(index) => {
            let control = state.ensure_control(document, options[index])?;
            Ok(control.map(|c| c.value.clone()))
        }
        None => Ok(None),
    }
}

/// Select the option at `index` (clearing the others). An out-of-range index
/// deselects every option (like setting `selectedIndex = -1`).
pub fn set_selected_index(
    document: &Document,
    state: &mut FormState,
    select: NodeId,
    index: Option<usize>,
) -> MochaResult<()> {
    let options = select_options(document, select)?;
    for (position, &option) in options.iter().enumerate() {
        if let Some(control) = state.ensure_control(document, option)? {
            control.selected = Some(position) == index;
        }
    }
    Ok(())
}

/// Select the first option whose current value equals `value`. With no match,
/// every option is deselected (mirroring the browser `select.value` setter).
pub fn set_select_value(
    document: &Document,
    state: &mut FormState,
    select: NodeId,
    value: &str,
) -> MochaResult<()> {
    let options = select_options(document, select)?;
    let mut matched = None;
    for (index, &option) in options.iter().enumerate() {
        let is_match = state
            .ensure_control(document, option)?
            .is_some_and(|control| control.value == value);
        if is_match {
            matched = Some(index);
            break;
        }
    }
    set_selected_index(document, state, select, matched)
}

/// Check `radio` and uncheck every other radio in its group: same (present)
/// `name`, same owner form (or both formless), anywhere in the document. An
/// unnamed radio forms a group of one.
pub fn select_radio(document: &Document, state: &mut FormState, radio: NodeId) -> MochaResult<()> {
    let (group_name, group_form) = {
        let Some(control) = state.ensure_control(document, radio)? else {
            return Err(MochaError::Dom(format!(
                "node {} is not a form control",
                radio.0
            )));
        };
        if control.kind != ControlKind::Radio {
            return Err(MochaError::Dom(format!(
                "node {} is not a radio input",
                radio.0
            )));
        }
        (control.name.clone(), owner_form(document, radio)?)
    };

    if let Some(name) = group_name {
        for node in document.traverse_depth_first(document.root_id())? {
            if node == radio || document.tag_name(node)? != Some("input") {
                continue;
            }
            if owner_form(document, node)? != group_form {
                continue;
            }
            if let Some(control) = state.ensure_control(document, node)? {
                if control.kind == ControlKind::Radio && control.name.as_deref() == Some(&name) {
                    control.checked = false;
                }
            }
        }
    }

    if let Some(control) = state.control_mut(radio) {
        control.checked = true;
    }
    Ok(())
}

/// Reset every control inside `form` to its attribute-initialized state and
/// re-normalize its selects (the `<input type="reset">` default action).
pub fn reset_form(document: &Document, state: &mut FormState, form: NodeId) -> MochaResult<()> {
    for node in document.traverse_depth_first(form)? {
        if let Some(initial) = initial_control_state(document, node)? {
            state.controls.insert(node, initial);
        }
        if document.tag_name(node)? == Some("select") {
            normalize_select(document, state, node)?;
        }
    }
    Ok(())
}

/// Make a select's option states consistent: the last `selected` option wins;
/// with none marked, the first option is selected.
fn normalize_select(document: &Document, state: &mut FormState, select: NodeId) -> MochaResult<()> {
    let options = select_options(document, select)?;
    if options.is_empty() {
        return Ok(());
    }
    let mut chosen = 0;
    for (index, &option) in options.iter().enumerate() {
        let selected = state
            .ensure_control(document, option)?
            .is_some_and(|control| control.selected);
        if selected {
            chosen = index;
        }
    }
    set_selected_index(document, state, select, Some(chosen))
}

/// Build the initial [`ControlState`] for a node from its DOM attributes, or
/// `None` when the node is not a form-control element.
fn initial_control_state(document: &Document, node: NodeId) -> MochaResult<Option<ControlState>> {
    let NodeKind::Element(data) = &document.node(node)?.kind else {
        return Ok(None);
    };
    if !is_control_tag(&data.tag_name) {
        return Ok(None);
    }

    let attribute = |name: &str| data.attribute(name).map(str::to_string);
    let disabled = data.attribute("disabled").is_some();
    let name = attribute("name");

    let (kind, value, checked, selected) = match data.tag_name.as_str() {
        "input" => {
            let kind = input_kind(data.attribute("type"))?;
            let value = attribute("value").unwrap_or_else(|| {
                // Checkboxes and radios submit "on" when no value is given.
                match kind {
                    ControlKind::Checkbox | ControlKind::Radio => "on".to_string(),
                    _ => String::new(),
                }
            });
            let checked = data.attribute("checked").is_some();
            (kind, value, checked, false)
        }
        "button" => {
            let kind = button_kind(data.attribute("type"))?;
            (kind, attribute("value").unwrap_or_default(), false, false)
        }
        "textarea" => {
            // The element's raw text content is the initial value.
            (
                ControlKind::TextArea,
                document.text_content(node)?,
                false,
                false,
            )
        }
        "select" => (ControlKind::Select, String::new(), false, false),
        "option" => {
            // The value attribute, falling back to the option's text.
            let value = match attribute("value") {
                Some(value) => value,
                None => document.text_content(node)?.trim().to_string(),
            };
            let selected = data.attribute("selected").is_some();
            (ControlKind::Option, value, false, selected)
        }
        other => {
            return Err(MochaError::Dom(format!(
                "is_control_tag and initial_control_state disagree on <{other}>"
            )))
        }
    };

    Ok(Some(ControlState {
        kind,
        name,
        value,
        checked,
        selected,
        disabled,
    }))
}

/// Normalize an `<input type>` attribute to a [`ControlKind`]. Unsupported
/// types are a clear error, not a silent fallback to text.
fn input_kind(type_attribute: Option<&str>) -> MochaResult<ControlKind> {
    let normalized = type_attribute.unwrap_or("text").trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "text" => Ok(ControlKind::Text),
        "password" => Ok(ControlKind::Password),
        "checkbox" => Ok(ControlKind::Checkbox),
        "radio" => Ok(ControlKind::Radio),
        "submit" => Ok(ControlKind::Submit),
        "reset" => Ok(ControlKind::Reset),
        "hidden" => Ok(ControlKind::Hidden),
        other => Err(MochaError::UnsupportedFeature(format!(
            "<input type=\"{other}\"> is not supported in Milestone 10"
        ))),
    }
}

/// Normalize a `<button type>` attribute to a [`ControlKind`] (default: submit).
fn button_kind(type_attribute: Option<&str>) -> MochaResult<ControlKind> {
    let normalized = type_attribute
        .unwrap_or("submit")
        .trim()
        .to_ascii_lowercase();
    match normalized.as_str() {
        "" | "submit" => Ok(ControlKind::Submit),
        "button" => Ok(ControlKind::Button),
        "reset" => Ok(ControlKind::Reset),
        other => Err(MochaError::UnsupportedFeature(format!(
            "<button type=\"{other}\"> is not supported in Milestone 10"
        ))),
    }
}
