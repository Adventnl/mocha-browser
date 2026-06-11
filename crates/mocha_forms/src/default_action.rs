//! Form default actions for click events.
//!
//! Mirrors `mocha_nav::default_action_for_event` for the link default action:
//! after listeners have run, an un-prevented `click` on (or inside) a form
//! control toggles a checkbox, selects a radio group member, resets a form, or
//! requests a submission. Checkbox/radio/reset mutate the [`FormState`]
//! directly; submit only *identifies* the form and submitter — the caller
//! builds the [`crate::FormSubmission`] (URL resolution needs the document base
//! URL, which the event path does not carry).
//!
//! There is no focus, caret, or text editing: clicking a text control does
//! nothing. Label activation (`<label for>`) is not implemented.

use mocha_dom::{Document, NodeId};
use mocha_error::MochaResult;
use mocha_events::Event;

use crate::state::{is_control_tag, owner_form, reset_form, select_radio, ControlKind, FormState};

/// The form default action implied by a click after listeners have run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormDefaultAction {
    /// Nothing to do.
    None,
    /// The checkbox was toggled (the state has already been updated).
    ToggleCheckbox(NodeId),
    /// The radio was selected and its group unchecked (already applied).
    SelectRadio(NodeId),
    /// A submit button asked to submit its form. The caller decides whether to
    /// build a [`crate::FormSubmission`] (and with which base URL).
    Submit {
        /// The owning `<form>`.
        form: NodeId,
        /// The submit control that was clicked.
        submitter: NodeId,
    },
    /// The form was reset to its attribute-initialized state (already applied).
    Reset(NodeId),
}

/// Determine and apply the form default action for `event`.
///
/// Returns [`FormDefaultAction::None`] for non-click or prevented events,
/// clicks outside any control, and disabled controls.
pub fn form_default_action_for_event(
    document: &Document,
    state: &mut FormState,
    event: &Event,
) -> MochaResult<FormDefaultAction> {
    if event.event_type != "click" || event.default_prevented {
        return Ok(FormDefaultAction::None);
    }
    click_default_action(document, state, event.target)
}

/// Determine and apply the form default action for an un-prevented click at
/// `target` (the `mocha_js_dom` dispatch path, which reports prevention as a
/// boolean instead of carrying an [`Event`]).
///
/// The control is the click target itself or its nearest control ancestor, so
/// a click on the text inside a `<button>` activates the button.
pub fn click_default_action(
    document: &Document,
    state: &mut FormState,
    target: NodeId,
) -> MochaResult<FormDefaultAction> {
    let mut chain = vec![target];
    chain.extend(document.ancestors(target)?);

    for node in chain {
        let Some(tag) = document.tag_name(node)? else {
            continue;
        };
        if !is_control_tag(tag) {
            continue;
        }
        return apply_click(document, state, node);
    }
    Ok(FormDefaultAction::None)
}

/// Apply the default click behaviour of one control.
fn apply_click(
    document: &Document,
    state: &mut FormState,
    control: NodeId,
) -> MochaResult<FormDefaultAction> {
    let Some(control_state) = state.ensure_control(document, control)? else {
        return Ok(FormDefaultAction::None);
    };
    if control_state.disabled {
        return Ok(FormDefaultAction::None);
    }
    match control_state.kind {
        ControlKind::Checkbox => {
            control_state.checked = !control_state.checked;
            Ok(FormDefaultAction::ToggleCheckbox(control))
        }
        ControlKind::Radio => {
            select_radio(document, state, control)?;
            Ok(FormDefaultAction::SelectRadio(control))
        }
        ControlKind::Submit => match owner_form(document, control)? {
            Some(form) => Ok(FormDefaultAction::Submit {
                form,
                submitter: control,
            }),
            None => Ok(FormDefaultAction::None),
        },
        ControlKind::Reset => match owner_form(document, control)? {
            Some(form) => {
                reset_form(document, state, form)?;
                Ok(FormDefaultAction::Reset(form))
            }
            None => Ok(FormDefaultAction::None),
        },
        // Text controls would need focus/caret (out of scope); buttons of
        // type="button" and options have no default action.
        _ => Ok(FormDefaultAction::None),
    }
}
