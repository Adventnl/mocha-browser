//! Form-control state and submission modelling for Mocha Browser (Milestone 10).
//!
//! This crate owns everything *semantic* about forms, separate from parsing
//! (`mocha_html`), styling (`mocha_style`), geometry (`mocha_layout`), and
//! painting (`mocha_paint`):
//!
//! - [`FormState`] — the dynamic state (value/checked/selected/disabled) of every
//!   control, keyed by DOM node, initialized from attributes and mutated by
//!   JavaScript or default actions. The DOM itself stays immutable-by-forms.
//! - form ownership ([`owner_form`]) — a control belongs to its nearest `<form>`
//!   ancestor.
//! - default actions ([`form_default_action_for_event`] /
//!   [`click_default_action`]) — checkbox toggle, radio group selection, form
//!   reset, and submit identification, honouring `preventDefault` and
//!   `disabled`.
//! - the submission model ([`build_submission`]) — successful-control
//!   collection and GET URL construction. **POST is a clear
//!   `UnsupportedFeature` error**, never a fake network submission.
//!
//! Supported controls: `<input>` (text, password, checkbox, radio, submit,
//! reset, hidden), `<button>` (submit, reset, button), `<textarea>`,
//! `<select>`/`<option>`. Any other `input`/`button` type is an
//! `UnsupportedFeature` error during form processing. There is no real
//! keyboard/focus/caret interaction anywhere in Mocha yet; state changes come
//! from JavaScript and programmatic event dispatch.

mod default_action;
mod state;
mod submission;

pub use default_action::{click_default_action, form_default_action_for_event, FormDefaultAction};
pub use state::{
    build_form_state, owner_form, reset_form, select_radio, select_value, selected_index,
    set_select_value, set_selected_index, ControlKind, ControlState, FormState,
};
pub use submission::{build_submission, FormField, FormMethod, FormSubmission};

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_dom::{Document, NodeId};
    use mocha_error::MochaError;
    use mocha_events::Event;
    use mocha_url::Url;

    fn parse(html: &str) -> Document {
        mocha_html::parse_html(html).unwrap()
    }

    fn find_tag(document: &Document, tag: &str) -> NodeId {
        find_tag_nth(document, tag, 0)
    }

    fn find_tag_nth(document: &Document, tag: &str, nth: usize) -> NodeId {
        document
            .traverse_depth_first(document.root_id())
            .unwrap()
            .into_iter()
            .filter(|&id| document.tag_name(id).unwrap() == Some(tag))
            .nth(nth)
            .expect("tag present")
    }

    fn by_id(document: &Document, id: &str) -> NodeId {
        document.get_element_by_id(id).unwrap().expect("id present")
    }

    fn base() -> Url {
        Url::parse("http://example.com/dir/page.html").unwrap()
    }

    // --- state initialization ------------------------------------------------

    #[test]
    fn input_value_attribute_initializes_state() {
        let document = parse(r#"<input name="q" value="initial">"#);
        let state = build_form_state(&document).unwrap();
        let control = state.control(find_tag(&document, "input")).unwrap();
        assert_eq!(control.kind, ControlKind::Text);
        assert_eq!(control.name.as_deref(), Some("q"));
        assert_eq!(control.value, "initial");
        assert!(!control.checked && !control.disabled);
    }

    #[test]
    fn missing_type_defaults_to_text_and_password_is_recognised() {
        let document = parse(r#"<input name="a"><input type="password" name="b">"#);
        let state = build_form_state(&document).unwrap();
        assert_eq!(
            state
                .control(find_tag_nth(&document, "input", 0))
                .unwrap()
                .kind,
            ControlKind::Text
        );
        assert_eq!(
            state
                .control(find_tag_nth(&document, "input", 1))
                .unwrap()
                .kind,
            ControlKind::Password
        );
    }

    #[test]
    fn checkbox_and_radio_checked_attributes_initialize_state() {
        let document = parse(
            r#"<input id="c" type="checkbox" name="agree" checked>
               <input id="r" type="radio" name="size" value="large">"#,
        );
        let state = build_form_state(&document).unwrap();
        let checkbox = state.control(by_id(&document, "c")).unwrap();
        assert!(checkbox.checked);
        assert_eq!(checkbox.value, "on", "missing value defaults to \"on\"");
        let radio = state.control(by_id(&document, "r")).unwrap();
        assert!(!radio.checked);
        assert_eq!(radio.value, "large");
    }

    #[test]
    fn disabled_attribute_initializes_state() {
        let document = parse(r#"<input name="q" disabled>"#);
        let state = build_form_state(&document).unwrap();
        assert!(
            state
                .control(find_tag(&document, "input"))
                .unwrap()
                .disabled
        );
    }

    #[test]
    fn textarea_text_content_initializes_value() {
        let document = parse("<textarea name=\"m\">Hello Mocha</textarea>");
        let state = build_form_state(&document).unwrap();
        let control = state.control(find_tag(&document, "textarea")).unwrap();
        assert_eq!(control.kind, ControlKind::TextArea);
        assert_eq!(control.value, "Hello Mocha");
    }

    #[test]
    fn select_selected_option_initializes_state() {
        let document = parse(
            r#"<select name="choice"><option value="a">Alpha</option><option value="b" selected>Beta</option></select>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        let select = find_tag(&document, "select");
        assert_eq!(
            selected_index(&document, &mut state, select).unwrap(),
            Some(1)
        );
        assert_eq!(
            select_value(&document, &mut state, select)
                .unwrap()
                .as_deref(),
            Some("b")
        );
    }

    #[test]
    fn select_with_no_selected_option_defaults_to_first() {
        let document = parse(
            r#"<select name="c"><option value="a">A</option><option value="b">B</option></select>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        let select = find_tag(&document, "select");
        assert_eq!(
            selected_index(&document, &mut state, select).unwrap(),
            Some(0)
        );
        // An option-less select has no selection at all.
        let empty = parse(r#"<select name="e"></select>"#);
        let mut empty_state = build_form_state(&empty).unwrap();
        let empty_select = find_tag(&empty, "select");
        assert_eq!(
            selected_index(&empty, &mut empty_state, empty_select).unwrap(),
            None
        );
    }

    #[test]
    fn option_text_is_the_value_fallback() {
        let document = parse(r#"<select><option>Alpha</option></select>"#);
        let state = build_form_state(&document).unwrap();
        assert_eq!(
            state.control(find_tag(&document, "option")).unwrap().value,
            "Alpha"
        );
    }

    #[test]
    fn button_defaults_to_submit_kind() {
        let document = parse(r#"<button name="go" value="1">Go</button>"#);
        let state = build_form_state(&document).unwrap();
        assert_eq!(
            state.control(find_tag(&document, "button")).unwrap().kind,
            ControlKind::Submit
        );
    }

    #[test]
    fn unsupported_input_type_errors_clearly() {
        let document = parse(r#"<input type="date" name="when">"#);
        let error = build_form_state(&document).unwrap_err();
        match error {
            MochaError::UnsupportedFeature(message) => assert!(message.contains("date")),
            other => panic!("expected UnsupportedFeature, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_button_type_errors_clearly() {
        let document = parse(r#"<button type="menu">M</button>"#);
        assert!(matches!(
            build_form_state(&document).unwrap_err(),
            MochaError::UnsupportedFeature(_)
        ));
    }

    #[test]
    fn non_control_elements_have_no_state() {
        let document = parse(r#"<form action="/x"><p>hi</p></form>"#);
        let state = build_form_state(&document).unwrap();
        assert!(state.is_empty());
    }

    // --- form ownership -------------------------------------------------------

    #[test]
    fn owner_form_is_the_nearest_form_ancestor() {
        let document = parse(
            r#"<form id="f"><div><input id="i" name="q"></div></form><input id="o" name="x">"#,
        );
        let form = by_id(&document, "f");
        assert_eq!(
            owner_form(&document, by_id(&document, "i")).unwrap(),
            Some(form)
        );
        assert_eq!(owner_form(&document, by_id(&document, "o")).unwrap(), None);
    }

    // --- default actions -------------------------------------------------------

    #[test]
    fn click_toggles_checkbox() {
        let document = parse(r#"<input id="c" type="checkbox" name="agree">"#);
        let mut state = build_form_state(&document).unwrap();
        let checkbox = by_id(&document, "c");

        let action = click_default_action(&document, &mut state, checkbox).unwrap();
        assert_eq!(action, FormDefaultAction::ToggleCheckbox(checkbox));
        assert!(state.control(checkbox).unwrap().checked);

        click_default_action(&document, &mut state, checkbox).unwrap();
        assert!(!state.control(checkbox).unwrap().checked);
    }

    #[test]
    fn prevent_default_suppresses_checkbox_toggle() {
        let document = parse(r#"<input id="c" type="checkbox" name="agree">"#);
        let mut state = build_form_state(&document).unwrap();
        let checkbox = by_id(&document, "c");
        let mut event = Event::click(checkbox, 0.0, 0.0);
        event.prevent_default();

        let action = form_default_action_for_event(&document, &mut state, &event).unwrap();
        assert_eq!(action, FormDefaultAction::None);
        assert!(!state.control(checkbox).unwrap().checked);
    }

    #[test]
    fn click_selects_radio_and_unchecks_same_name_group() {
        let document = parse(
            r#"<form><input id="s" type="radio" name="size" value="small">
               <input id="l" type="radio" name="size" value="large" checked>
               <input id="other" type="radio" name="color" value="red" checked></form>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        let small = by_id(&document, "s");
        let large = by_id(&document, "l");

        let action = click_default_action(&document, &mut state, small).unwrap();
        assert_eq!(action, FormDefaultAction::SelectRadio(small));
        assert!(state.control(small).unwrap().checked);
        assert!(
            !state.control(large).unwrap().checked,
            "same-name radio unchecked"
        );
        assert!(
            state.control(by_id(&document, "other")).unwrap().checked,
            "different-name radio untouched"
        );
    }

    #[test]
    fn radios_with_the_same_name_in_different_forms_are_separate_groups() {
        let document = parse(
            r#"<form><input id="a" type="radio" name="x" checked></form>
               <form><input id="b" type="radio" name="x"></form>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        click_default_action(&document, &mut state, by_id(&document, "b")).unwrap();
        assert!(state.control(by_id(&document, "a")).unwrap().checked);
        assert!(state.control(by_id(&document, "b")).unwrap().checked);
    }

    #[test]
    fn click_submit_identifies_form_and_submitter() {
        let document =
            parse(r#"<form id="f" action="/go"><input id="s" type="submit" value="Go"></form>"#);
        let mut state = build_form_state(&document).unwrap();
        let action = click_default_action(&document, &mut state, by_id(&document, "s")).unwrap();
        assert_eq!(
            action,
            FormDefaultAction::Submit {
                form: by_id(&document, "f"),
                submitter: by_id(&document, "s"),
            }
        );
    }

    #[test]
    fn click_on_text_inside_button_activates_the_button() {
        let document =
            parse(r#"<form id="f"><button id="b"><span id="inner">Go</span></button></form>"#);
        let mut state = build_form_state(&document).unwrap();
        let action =
            click_default_action(&document, &mut state, by_id(&document, "inner")).unwrap();
        assert!(matches!(action, FormDefaultAction::Submit { .. }));
    }

    #[test]
    fn prevent_default_suppresses_submit() {
        let document = parse(r#"<form><input id="s" type="submit" value="Go"></form>"#);
        let mut state = build_form_state(&document).unwrap();
        let mut event = Event::click(by_id(&document, "s"), 0.0, 0.0);
        event.prevent_default();
        let action = form_default_action_for_event(&document, &mut state, &event).unwrap();
        assert_eq!(action, FormDefaultAction::None);
    }

    #[test]
    fn disabled_control_has_no_default_action() {
        let document = parse(
            r#"<form><input id="c" type="checkbox" disabled><input id="s" type="submit" disabled></form>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        assert_eq!(
            click_default_action(&document, &mut state, by_id(&document, "c")).unwrap(),
            FormDefaultAction::None
        );
        assert!(!state.control(by_id(&document, "c")).unwrap().checked);
        assert_eq!(
            click_default_action(&document, &mut state, by_id(&document, "s")).unwrap(),
            FormDefaultAction::None
        );
    }

    #[test]
    fn submit_outside_a_form_does_nothing() {
        let document = parse(r#"<input id="s" type="submit" value="Go">"#);
        let mut state = build_form_state(&document).unwrap();
        assert_eq!(
            click_default_action(&document, &mut state, by_id(&document, "s")).unwrap(),
            FormDefaultAction::None
        );
    }

    #[test]
    fn reset_restores_attribute_state() {
        let document = parse(
            r#"<form><input id="q" name="q" value="initial">
               <input id="c" type="checkbox" name="agree" checked>
               <input id="r" type="reset"></form>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        // Mutate, then reset.
        state.control_mut(by_id(&document, "q")).unwrap().value = "changed".to_string();
        state.control_mut(by_id(&document, "c")).unwrap().checked = false;
        let action = click_default_action(&document, &mut state, by_id(&document, "r")).unwrap();
        assert!(matches!(action, FormDefaultAction::Reset(_)));
        assert_eq!(
            state.control(by_id(&document, "q")).unwrap().value,
            "initial"
        );
        assert!(state.control(by_id(&document, "c")).unwrap().checked);
    }

    #[test]
    fn non_click_event_has_no_form_default_action() {
        let document = parse(r#"<input id="c" type="checkbox">"#);
        let mut state = build_form_state(&document).unwrap();
        let event = Event::new("mousedown", by_id(&document, "c"));
        assert_eq!(
            form_default_action_for_event(&document, &mut state, &event).unwrap(),
            FormDefaultAction::None
        );
    }

    // --- select state mutation --------------------------------------------------

    #[test]
    fn set_select_value_and_selected_index_update_options() {
        let document = parse(
            r#"<select name="c"><option value="a">A</option><option value="b" selected>B</option></select>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        let select = find_tag(&document, "select");

        set_select_value(&document, &mut state, select, "a").unwrap();
        assert_eq!(
            selected_index(&document, &mut state, select).unwrap(),
            Some(0)
        );

        set_selected_index(&document, &mut state, select, Some(1)).unwrap();
        assert_eq!(
            select_value(&document, &mut state, select)
                .unwrap()
                .as_deref(),
            Some("b")
        );
    }

    // --- submission ---------------------------------------------------------------

    /// The submission of the first form in `html`, with `submitter_id` (an `id`
    /// attribute) optionally naming the clicked submit control.
    fn submit(html: &str, submitter_id: Option<&str>) -> Result<FormSubmission, MochaError> {
        let document = parse(html);
        let mut state = build_form_state(&document)?;
        let form = find_tag(&document, "form");
        let submitter = submitter_id.map(|id| by_id(&document, id));
        build_submission(&document, &mut state, form, submitter, &base())
    }

    fn names(submission: &FormSubmission) -> Vec<&str> {
        submission.fields.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn get_submission_collects_text_inputs_into_query_url() {
        let submission = submit(
            r#"<form action="/search" method="get"><input name="q" value="mocha"><input name="page" value="1"></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(submission.method, FormMethod::Get);
        assert_eq!(
            submission.action.normalized(),
            "http://example.com/search?q=mocha&page=1"
        );
    }

    #[test]
    fn relative_action_resolves_against_base() {
        let submission = submit(
            r#"<form action="results.html"><input name="q" value="x"></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(submission.action.path, "/dir/results.html");
    }

    #[test]
    fn empty_or_missing_action_uses_the_document_url() {
        for html in [
            r#"<form><input name="q" value="x"></form>"#,
            r#"<form action=""><input name="q" value="x"></form>"#,
        ] {
            let submission = submit(html, None).unwrap();
            assert_eq!(submission.action.path, "/dir/page.html");
            assert_eq!(submission.action.query.as_deref(), Some("q=x"));
        }
    }

    #[test]
    fn disabled_and_unnamed_fields_are_excluded() {
        let submission = submit(
            r#"<form><input name="ok" value="1"><input name="off" value="2" disabled><input value="3"></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(names(&submission), vec!["ok"]);
    }

    #[test]
    fn checkbox_and_radio_inclusion_follows_checked_state() {
        let submission = submit(
            r#"<form><input type="checkbox" name="a" checked>
               <input type="checkbox" name="b">
               <input type="radio" name="size" value="small">
               <input type="radio" name="size" value="large" checked></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(
            submission.fields,
            vec![
                FormField {
                    name: "a".to_string(),
                    value: "on".to_string()
                },
                FormField {
                    name: "size".to_string(),
                    value: "large".to_string()
                },
            ]
        );
    }

    #[test]
    fn textarea_and_select_are_included() {
        let submission = submit(
            r#"<form><textarea name="m">Hi</textarea>
               <select name="c"><option value="a">A</option><option value="b" selected>B</option></select></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(
            submission.fields,
            vec![
                FormField {
                    name: "m".to_string(),
                    value: "Hi".to_string()
                },
                FormField {
                    name: "c".to_string(),
                    value: "b".to_string()
                },
            ]
        );
    }

    #[test]
    fn submit_button_is_included_only_as_the_submitter() {
        let html = r#"<form><input name="q" value="x">
            <input id="go" type="submit" name="go" value="Go">
            <button id="alt" name="alt" value="Alt">Alt</button>
            <button type="button" name="never" value="n">N</button></form>"#;
        // Not submitting via a button: no submit fields at all.
        assert_eq!(names(&submit(html, None).unwrap()), vec!["q"]);
        // Submitting via "go": only "go" joins.
        assert_eq!(names(&submit(html, Some("go")).unwrap()), vec!["q", "go"]);
        // Submitting via the (named) button element.
        assert_eq!(names(&submit(html, Some("alt")).unwrap()), vec!["q", "alt"]);
    }

    #[test]
    fn hidden_input_is_included() {
        let submission = submit(
            r#"<form><input type="hidden" name="token" value="abc123"></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(
            submission.fields,
            vec![FormField {
                name: "token".to_string(),
                value: "abc123".to_string()
            }]
        );
    }

    #[test]
    fn query_values_are_form_urlencoded() {
        let submission = submit(
            r#"<form action="/s"><input name="q" value="a b&c=d+e"></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(
            submission.action.query.as_deref(),
            Some("q=a+b%26c%3Dd%2Be")
        );
        // Non-ASCII goes through UTF-8 percent-encoding.
        let unicode = submit(
            r#"<form action="/s"><input name="q" value="café"></form>"#,
            None,
        )
        .unwrap();
        assert_eq!(unicode.action.query.as_deref(), Some("q=caf%C3%A9"));
    }

    #[test]
    fn post_form_is_clearly_unsupported() {
        let error = submit(
            r#"<form action="/s" method="post"><input name="q" value="x"></form>"#,
            None,
        )
        .unwrap_err();
        match error {
            MochaError::UnsupportedFeature(message) => {
                assert!(message.contains("POST"), "message: {message}")
            }
            other => panic!("expected UnsupportedFeature, got {other:?}"),
        }
    }

    #[test]
    fn unknown_method_is_clearly_unsupported() {
        assert!(matches!(
            submit(
                r#"<form method="dialog"><input name="q" value="x"></form>"#,
                None
            ),
            Err(MochaError::UnsupportedFeature(_))
        ));
    }

    #[test]
    fn submitting_a_non_form_node_errors() {
        let document = parse(r#"<div id="d"></div>"#);
        let mut state = build_form_state(&document).unwrap();
        assert!(matches!(
            build_submission(&document, &mut state, by_id(&document, "d"), None, &base()),
            Err(MochaError::Dom(_))
        ));
    }

    #[test]
    fn js_style_state_changes_are_reflected_in_submission() {
        let document = parse(
            r#"<form action="/s"><input id="q" name="q" value="before">
               <input id="c" type="checkbox" name="agree"></form>"#,
        );
        let mut state = build_form_state(&document).unwrap();
        state.control_mut(by_id(&document, "q")).unwrap().value = "after".to_string();
        state.control_mut(by_id(&document, "c")).unwrap().checked = true;
        let form = find_tag(&document, "form");
        let submission = build_submission(&document, &mut state, form, None, &base()).unwrap();
        assert_eq!(submission.action.query.as_deref(), Some("q=after&agree=on"));
    }
}
