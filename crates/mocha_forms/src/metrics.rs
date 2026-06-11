//! Form-control sizing and paint metrics.
//!
//! This is the single home for control sizing rules, shared by every embedder
//! (the terminal shell and the desktop shell both call
//! [`resolve_control_metrics`] rather than duplicating the defaults). It maps a
//! control's [`FormState`] entry plus its CSS `width`/`height` to a
//! [`ControlMetrics`]: the data a `DrawControl` display command needs. It owns
//! **no** layout geometry beyond the control's own box size.

use mocha_dom::{Document, NodeId};
use mocha_error::MochaResult;

use crate::state::{select_value, ControlKind, FormState};

/// Default content sizes (in CSS px) for each control, overridable by CSS
/// `width`/`height`. Buttons size to their label; textareas to `rows`/`cols`.
const TEXT_WIDTH: f32 = 160.0;
const TEXT_HEIGHT: f32 = 24.0;
const TOGGLE_SIZE: f32 = 13.0;
const BUTTON_HEIGHT: f32 = 26.0;
const BUTTON_MIN_WIDTH: f32 = 40.0;
const BUTTON_PADDING: f32 = 16.0;
const SELECT_WIDTH: f32 = 160.0;
const SELECT_HEIGHT: f32 = 24.0;
const TEXTAREA_FALLBACK_WIDTH: f32 = 200.0;
const TEXTAREA_FALLBACK_HEIGHT: f32 = 80.0;
const TEXTAREA_COL_WIDTH: f32 = 8.0;
const TEXTAREA_ROW_HEIGHT: f32 = 18.0;

/// The resolved size and paint data of one form control.
///
/// Field-for-field this matches `mocha_style::ControlBox`; the embedder maps one
/// to the other. Keeping this type in `mocha_forms` lets the sizing rules live
/// with the form model while `mocha_style`/`mocha_layout`/`mocha_paint` stay
/// free of form semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlMetrics {
    /// The normalized control type painted (`"text"`, `"checkbox"`, `"button"`, …).
    pub control_type: String,
    /// Final content width in pixels.
    pub width: f32,
    /// Final content height in pixels.
    pub height: f32,
    /// The current value (text controls / select) or visible label (buttons).
    pub value: Option<String>,
    /// The checked state for checkboxes/radios; `None` otherwise.
    pub checked: Option<bool>,
    /// Whether the control is disabled.
    pub disabled: bool,
}

/// Resolve the [`ControlMetrics`] for `node`, or `None` when it generates no
/// visible control box (`<input type="hidden">`, `<option>`, or a non-control
/// element).
///
/// `css_width`/`css_height` are the element's computed CSS lengths (they
/// override the control defaults); `font_size` is the computed font size, used
/// to estimate a button's label width with the same metric layout uses for text
/// (`chars * font * 0.6`).
pub fn resolve_control_metrics(
    document: &Document,
    state: &mut FormState,
    node: NodeId,
    css_width: Option<f32>,
    css_height: Option<f32>,
    font_size: f32,
) -> MochaResult<Option<ControlMetrics>> {
    // Only the box-generating control elements; options render inside their
    // select, and labels/forms are ordinary flow content.
    let is_button_element = document.tag_name(node)? == Some("button");
    if !matches!(
        document.tag_name(node)?,
        Some("input" | "button" | "textarea" | "select")
    ) {
        return Ok(None);
    }
    let Some(control) = state.ensure_control(document, node)? else {
        return Ok(None);
    };
    let (kind, value, checked, disabled) = (
        control.kind,
        control.value.clone(),
        control.checked,
        control.disabled,
    );

    let (control_type, display_value, checked_state, default_width, default_height) = match kind {
        ControlKind::Hidden | ControlKind::Option => return Ok(None),
        ControlKind::Text | ControlKind::Password => {
            (kind.as_str(), Some(value), None, TEXT_WIDTH, TEXT_HEIGHT)
        }
        ControlKind::Checkbox | ControlKind::Radio => {
            (kind.as_str(), None, Some(checked), TOGGLE_SIZE, TOGGLE_SIZE)
        }
        ControlKind::Submit | ControlKind::Reset | ControlKind::Button => {
            // The visible label: a <button>'s text content, an <input>'s value,
            // or the UA default. Width estimates the label with the same metric
            // layout uses for text (chars * font * 0.6) plus padding.
            let label = if is_button_element {
                document.text_content(node)?.trim().to_string()
            } else {
                value
            };
            let label = if label.is_empty() {
                match kind {
                    ControlKind::Reset => "Reset".to_string(),
                    _ => "Submit".to_string(),
                }
            } else {
                label
            };
            let width = (label.chars().count() as f32 * font_size * 0.6).round() + BUTTON_PADDING;
            // A <button> always paints as "button"; an <input type=submit/reset>
            // keeps its own type.
            let control_type = if is_button_element {
                "button"
            } else {
                kind.as_str()
            };
            (
                control_type,
                Some(label),
                None,
                width.max(BUTTON_MIN_WIDTH),
                BUTTON_HEIGHT,
            )
        }
        ControlKind::TextArea => {
            let dimension = |name: &str| {
                document
                    .get_attribute(node, name)
                    .ok()
                    .flatten()
                    .and_then(|attr| attr.trim().parse::<f32>().ok())
                    .filter(|parsed| *parsed > 0.0)
            };
            let width = dimension("cols")
                .map(|cols| cols * TEXTAREA_COL_WIDTH)
                .unwrap_or(TEXTAREA_FALLBACK_WIDTH);
            let height = dimension("rows")
                .map(|rows| rows * TEXTAREA_ROW_HEIGHT)
                .unwrap_or(TEXTAREA_FALLBACK_HEIGHT);
            ("textarea", Some(value), None, width, height)
        }
        ControlKind::Select => {
            let value = select_value(document, state, node)?;
            ("select", value, None, SELECT_WIDTH, SELECT_HEIGHT)
        }
    };

    Ok(Some(ControlMetrics {
        control_type: control_type.to_string(),
        width: css_width.unwrap_or(default_width),
        height: css_height.unwrap_or(default_height),
        value: display_value,
        checked: checked_state,
        disabled,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_form_state;

    fn parse(html: &str) -> Document {
        mocha_html::parse_html(html).unwrap()
    }

    fn find_tag(document: &Document, tag: &str) -> NodeId {
        document
            .traverse_depth_first(document.root_id())
            .unwrap()
            .into_iter()
            .find(|&id| document.tag_name(id).unwrap() == Some(tag))
            .expect("tag present")
    }

    /// Resolve the first control of `tag` with default CSS (no overrides) at the
    /// default font size.
    fn metrics(html: &str, tag: &str) -> Option<ControlMetrics> {
        let document = parse(html);
        let mut state = build_form_state(&document).unwrap();
        let node = find_tag(&document, tag);
        resolve_control_metrics(&document, &mut state, node, None, None, 16.0).unwrap()
    }

    #[test]
    fn text_input_metrics_are_160_by_24() {
        let m = metrics(r#"<input name="q" value="hi">"#, "input").unwrap();
        assert_eq!(m.control_type, "text");
        assert_eq!((m.width, m.height), (160.0, 24.0));
        assert_eq!(m.value.as_deref(), Some("hi"));
        assert_eq!(m.checked, None);
    }

    #[test]
    fn password_matches_text_size() {
        let m = metrics(r#"<input type="password" name="p">"#, "input").unwrap();
        assert_eq!(m.control_type, "password");
        assert_eq!((m.width, m.height), (160.0, 24.0));
    }

    #[test]
    fn checkbox_and_radio_metrics_are_13_square_with_checked_state() {
        let cb = metrics(r#"<input type="checkbox" checked>"#, "input").unwrap();
        assert_eq!(cb.control_type, "checkbox");
        assert_eq!((cb.width, cb.height), (13.0, 13.0));
        assert_eq!(cb.checked, Some(true));

        let radio = metrics(r#"<input type="radio">"#, "input").unwrap();
        assert_eq!((radio.width, radio.height), (13.0, 13.0));
        assert_eq!(radio.checked, Some(false));
    }

    #[test]
    fn button_width_grows_with_label_and_height_is_26() {
        let short = metrics(r#"<button>Go</button>"#, "button").unwrap();
        let long = metrics(r#"<button>A longer label here</button>"#, "button").unwrap();
        assert_eq!(short.control_type, "button");
        assert_eq!(short.height, 26.0);
        assert_eq!(short.value.as_deref(), Some("Go"));
        assert!(long.width > short.width);
    }

    #[test]
    fn submit_input_without_value_uses_submit_label() {
        let m = metrics(r#"<input type="submit">"#, "input").unwrap();
        assert_eq!(m.control_type, "submit");
        assert_eq!(m.value.as_deref(), Some("Submit"));
        assert!(m.width >= BUTTON_MIN_WIDTH);
    }

    #[test]
    fn textarea_uses_rows_cols_then_falls_back() {
        let sized = metrics(r#"<textarea rows="4" cols="20">x</textarea>"#, "textarea").unwrap();
        assert_eq!((sized.width, sized.height), (160.0, 72.0));
        let bare = metrics(r#"<textarea>x</textarea>"#, "textarea").unwrap();
        assert_eq!((bare.width, bare.height), (200.0, 80.0));
    }

    #[test]
    fn select_metrics_carry_selected_value() {
        let m = metrics(
            r#"<select><option value="a">A</option><option value="b" selected>B</option></select>"#,
            "select",
        )
        .unwrap();
        assert_eq!(m.control_type, "select");
        assert_eq!((m.width, m.height), (160.0, 24.0));
        assert_eq!(m.value.as_deref(), Some("b"));
    }

    #[test]
    fn css_width_and_height_override_defaults() {
        let document = parse(r#"<input name="q">"#);
        let mut state = build_form_state(&document).unwrap();
        let node = find_tag(&document, "input");
        let m = resolve_control_metrics(&document, &mut state, node, Some(300.0), Some(40.0), 16.0)
            .unwrap()
            .unwrap();
        assert_eq!((m.width, m.height), (300.0, 40.0));
    }

    #[test]
    fn hidden_input_and_non_controls_have_no_metrics() {
        assert!(metrics(r#"<input type="hidden" name="t" value="x">"#, "input").is_none());
        let document = parse(r#"<form><p>hi</p></form>"#);
        let mut state = build_form_state(&document).unwrap();
        let p = find_tag(&document, "p");
        assert!(
            resolve_control_metrics(&document, &mut state, p, None, None, 16.0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn disabled_state_is_carried() {
        let m = metrics(r#"<input name="q" disabled>"#, "input").unwrap();
        assert!(m.disabled);
    }
}
