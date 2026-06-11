//! The form-submission model: collect successful controls and build a GET
//! submission URL.
//!
//! Nothing here performs a navigation or a network request. A
//! [`FormSubmission`]'s `action` is a plain [`Url`] the embedder may pass to
//! `mocha_nav` if it chooses. Only `method="get"` is supported: a POST form is a
//! clear [`MochaError::UnsupportedFeature`], never a fake submission.

use mocha_dom::{Document, NodeId};
use mocha_error::{MochaError, MochaResult};
use mocha_url::Url;

use crate::state::{select_value, ControlKind, FormState};

/// The HTTP method of a form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormMethod {
    /// `method="get"` (the default): fields are serialized into the URL query.
    Get,
    /// `method="post"`: recognised but unsupported in Milestone 10.
    Post,
}

/// One successful control's submission entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    /// The control's `name`.
    pub name: String,
    /// The control's current value.
    pub value: String,
}

/// The outcome of submitting a form: the resolved action URL (with the
/// serialized query for GET), the method, and the collected fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSubmission {
    /// The resolved submission URL, query included.
    pub action: Url,
    /// The form method.
    pub method: FormMethod,
    /// The successful controls, in document order (the submitter last in its
    /// own document position).
    pub fields: Vec<FormField>,
}

/// Build the [`FormSubmission`] for `form`.
///
/// `submitter` is the submit button that triggered the submission, if any; a
/// *named* submitter contributes its own field. `base` is the document URL:
/// the `action` attribute is resolved against it, and an empty or missing
/// `action` submits to the document URL itself (with the form's query).
///
/// Errors: a non-`<form>` node, an unsupported method (`post` included), or an
/// unsupported control type inside the form.
pub fn build_submission(
    document: &Document,
    state: &mut FormState,
    form: NodeId,
    submitter: Option<NodeId>,
    base: &Url,
) -> MochaResult<FormSubmission> {
    if document.tag_name(form)? != Some("form") {
        return Err(MochaError::Dom(format!(
            "node {} is not a <form> element",
            form.0
        )));
    }

    let method = form_method(document, form)?;
    if method == FormMethod::Post {
        return Err(MochaError::UnsupportedFeature(
            "POST form submission is not supported in Milestone 10".to_string(),
        ));
    }

    let fields = collect_successful_controls(document, state, form, submitter)?;

    let mut action = match document.get_attribute(form, "action")? {
        Some(attribute) if !attribute.trim().is_empty() => base.join(attribute.trim())?,
        _ => base.clone(),
    };
    action.query = Some(encode_query(&fields));
    action.fragment = None;

    Ok(FormSubmission {
        action,
        method,
        fields,
    })
}

/// Parse a form's `method` attribute. Missing or empty means GET; anything
/// other than `get`/`post` is unsupported.
fn form_method(document: &Document, form: NodeId) -> MochaResult<FormMethod> {
    let attribute = document
        .get_attribute(form, "method")?
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match attribute.as_str() {
        "" | "get" => Ok(FormMethod::Get),
        "post" => Ok(FormMethod::Post),
        other => Err(MochaError::UnsupportedFeature(format!(
            "form method \"{other}\" is not supported"
        ))),
    }
}

/// Collect the successful controls of `form` in document order.
///
/// Included: enabled, named text/password/hidden inputs and textareas (their
/// value), checked checkboxes/radios (their value, default `"on"`), selects
/// with options (the selected option's value), and the named `submitter`.
/// Excluded: disabled or unnamed controls, unchecked checkboxes/radios,
/// non-submitter submit buttons, reset buttons, and `type="button"` buttons.
fn collect_successful_controls(
    document: &Document,
    state: &mut FormState,
    form: NodeId,
    submitter: Option<NodeId>,
) -> MochaResult<Vec<FormField>> {
    let mut fields = Vec::new();
    for node in document.traverse_depth_first(form)? {
        // Copy what we need out of the control state so `select_value` below can
        // borrow the state again.
        let Some((kind, name, value, checked)) =
            state.ensure_control(document, node)?.and_then(|control| {
                if control.disabled {
                    return None;
                }
                let name = control.name.clone().filter(|name| !name.is_empty())?;
                Some((control.kind, name, control.value.clone(), control.checked))
            })
        else {
            continue;
        };
        let value = match kind {
            ControlKind::Text
            | ControlKind::Password
            | ControlKind::Hidden
            | ControlKind::TextArea => Some(value),
            ControlKind::Checkbox | ControlKind::Radio => checked.then_some(value),
            ControlKind::Submit => (Some(node) == submitter).then_some(value),
            ControlKind::Reset | ControlKind::Button | ControlKind::Option => None,
            ControlKind::Select => select_value(document, state, node)?,
        };
        if let Some(value) = value {
            fields.push(FormField { name, value });
        }
    }
    Ok(fields)
}

/// Serialize fields as `application/x-www-form-urlencoded`: `name=value` pairs
/// joined by `&`, with each name and value percent-encoded (space as `+`).
fn encode_query(fields: &[FormField]) -> String {
    fields
        .iter()
        .map(|field| {
            format!(
                "{}={}",
                encode_component(&field.name),
                encode_component(&field.value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encode one form-urlencoded component. ASCII alphanumerics and
/// `*`, `-`, `.`, `_` pass through, space becomes `+`, and every other byte
/// (UTF-8 for non-ASCII) becomes `%XX`.
fn encode_component(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push('+'),
            other => {
                encoded.push('%');
                encoded.push(hex_digit(other >> 4));
                encoded.push(hex_digit(other & 0x0f));
            }
        }
    }
    encoded
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + value - 10) as char,
    }
}
