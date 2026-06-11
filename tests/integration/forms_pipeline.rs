//! End-to-end tests for Milestone 10: form parsing, control state, JS-driven
//! state changes, control layout/paint (`DrawControl`), click default actions,
//! and the GET form-submission model. Uses the checked-in examples and local
//! files only; no public internet. Declared as a `[[test]]` of `mocha_shell`.

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use mocha_dom::{Document, NodeId};
use mocha_error::MochaError;
use mocha_events::{Event, EventDispatcher};
use mocha_forms::{
    build_form_state, build_submission, click_default_action, form_default_action_for_event,
    FormDefaultAction,
};
use mocha_shell::{hit_test_file, run_file, DisplayCommand};
use mocha_url::Url;

/// Build a path to a file under `examples/` relative to this crate.
fn example(rel: &str) -> String {
    format!("{}/../../examples/{}", env!("CARGO_MANIFEST_DIR"), rel)
}

fn parse_example(rel: &str) -> Document {
    let html = fs::read_to_string(example(rel)).expect("read example");
    mocha_html::parse_html(&html).expect("parse example")
}

fn by_id(document: &Document, id: &str) -> NodeId {
    document.get_element_by_id(id).unwrap().expect("id present")
}

fn find_tag(document: &Document, tag: &str) -> NodeId {
    document
        .traverse_depth_first(document.root_id())
        .unwrap()
        .into_iter()
        .find(|&id| document.tag_name(id).unwrap() == Some(tag))
        .expect("tag present")
}

/// All `DrawControl` commands as `(type, value, checked, disabled)`.
fn draw_controls(commands: &[DisplayCommand]) -> Vec<(String, Option<String>, Option<bool>, bool)> {
    commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::DrawControl {
                control_type,
                value,
                checked,
                disabled,
                ..
            } => Some((control_type.clone(), value.clone(), *checked, *disabled)),
            _ => None,
        })
        .collect()
}

#[test]
fn basic_form_example_renders_label_text_and_controls() {
    let commands = run_file(&example("forms/basic-form.html")).unwrap();
    let controls = draw_controls(&commands);
    assert_eq!(
        controls,
        vec![
            ("text".to_string(), Some("mocha".to_string()), None, false),
            (
                "submit".to_string(),
                Some("Search".to_string()),
                None,
                false
            ),
        ]
    );
    // The label still draws as text.
    assert!(commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::DrawText { text, .. } if text == "Search")));
}

#[test]
fn checkbox_radio_example_state_is_correct() {
    let controls = draw_controls(&run_file(&example("forms/checkbox-radio.html")).unwrap());
    assert_eq!(
        controls,
        vec![
            ("checkbox".to_string(), None, Some(true), false),
            ("radio".to_string(), None, Some(false), false),
            ("radio".to_string(), None, Some(true), false),
        ]
    );
}

#[test]
fn textarea_select_example_state_is_correct() {
    let controls = draw_controls(&run_file(&example("forms/textarea-select.html")).unwrap());
    assert_eq!(
        controls,
        vec![
            (
                "textarea".to_string(),
                Some("Hello Mocha".to_string()),
                None,
                false
            ),
            ("select".to_string(), Some("b".to_string()), None, false),
        ]
    );
}

#[test]
fn js_form_state_example_changes_draw_control_value_and_checked() {
    let controls = draw_controls(&run_file(&example("forms/js-form-state.html")).unwrap());
    assert_eq!(
        controls,
        vec![
            ("text".to_string(), Some("After".to_string()), None, false),
            ("checkbox".to_string(), None, Some(true), false),
        ]
    );
}

#[test]
fn form_submit_example_builds_the_expected_get_url() {
    let document = parse_example("forms/form-submit.html");
    let mut state = build_form_state(&document).unwrap();
    let form = by_id(&document, "search");
    let submitter = find_tag(&document, "input"); // first input is "q"; find the submit below
    assert_eq!(
        document.get_attribute(submitter, "name").unwrap(),
        Some("q")
    );
    let submit_button = document
        .traverse_depth_first(document.root_id())
        .unwrap()
        .into_iter()
        .find(|&id| document.get_attribute(id, "type").unwrap() == Some("submit"))
        .expect("submit input present");

    let base = Url::parse("http://forms.example/page.html").unwrap();
    let submission =
        build_submission(&document, &mut state, form, Some(submit_button), &base).unwrap();
    // The unnamed submit button contributes no field.
    assert_eq!(
        submission.action.normalized(),
        "http://forms.example/search?q=mocha&page=1"
    );
}

#[test]
fn click_dispatch_through_internal_events_toggles_checkbox_unless_prevented() {
    let document = parse_example("forms/checkbox-radio.html");
    let mut state = build_form_state(&document).unwrap();
    let checkbox = find_tag(&document, "input");
    let mut dispatcher = EventDispatcher::new();

    // Plain dispatch: the default action toggles the (initially checked) box off.
    let mut event = Event::click(checkbox, 0.0, 0.0);
    dispatcher.dispatch_event(&document, &mut event).unwrap();
    let action = form_default_action_for_event(&document, &mut state, &event).unwrap();
    assert_eq!(action, FormDefaultAction::ToggleCheckbox(checkbox));
    assert!(!state.control(checkbox).unwrap().checked);

    // A listener calling preventDefault suppresses the toggle.
    dispatcher.add_event_listener(
        checkbox,
        "click",
        mocha_events::EventListenerOptions::bubble(),
        Box::new(|event: &mut Event| event.prevent_default()),
    );
    let mut event = Event::click(checkbox, 0.0, 0.0);
    dispatcher.dispatch_event(&document, &mut event).unwrap();
    let action = form_default_action_for_event(&document, &mut state, &event).unwrap();
    assert_eq!(action, FormDefaultAction::None);
    assert!(!state.control(checkbox).unwrap().checked, "still unchecked");
}

#[test]
fn click_dispatch_through_js_runtime_respects_prevent_default() {
    let html = r#"<html><body><form>
        <input id="c" type="checkbox" name="agree">
        <input id="p" type="checkbox" name="blocked">
      </form>
      <script>
        document.getElementById("p").addEventListener("click", function (event) {
          event.preventDefault();
        });
      </script></body></html>"#;
    let doc = Rc::new(RefCell::new(mocha_html::parse_html(html).unwrap()));
    let scripts = mocha_js_dom::collect_inline_scripts(&doc.borrow()).unwrap();
    let mut runtime = mocha_js_dom::DomRuntime::new(doc.clone());
    runtime.init_form_state().unwrap();
    for source in &scripts {
        runtime.run_script(source).unwrap();
    }

    let plain = doc.borrow().get_element_by_id("c").unwrap().unwrap();
    let prevented = doc.borrow().get_element_by_id("p").unwrap().unwrap();
    let forms = runtime.form_state();

    // Un-prevented JS click: apply the form default action.
    let proceed = runtime.dispatch_event("click", plain).unwrap();
    assert!(proceed);
    click_default_action(&doc.borrow(), &mut forms.borrow_mut(), plain).unwrap();
    assert!(forms.borrow().control(plain).unwrap().checked);

    // preventDefault in a JS listener: the caller skips the default action.
    let proceed = runtime.dispatch_event("click", prevented).unwrap();
    assert!(!proceed);
    assert!(!forms.borrow().control(prevented).unwrap().checked);
}

#[test]
fn radio_click_unchecks_the_rest_of_its_group() {
    let document = parse_example("forms/checkbox-radio.html");
    let mut state = build_form_state(&document).unwrap();
    let inputs: Vec<NodeId> = document
        .traverse_depth_first(document.root_id())
        .unwrap()
        .into_iter()
        .filter(|&id| document.tag_name(id).unwrap() == Some("input"))
        .collect();
    let (small, large) = (inputs[1], inputs[2]);
    assert!(state.control(large).unwrap().checked);

    let action = click_default_action(&document, &mut state, small).unwrap();
    assert_eq!(action, FormDefaultAction::SelectRadio(small));
    assert!(state.control(small).unwrap().checked);
    assert!(!state.control(large).unwrap().checked);
}

#[test]
fn submit_click_produces_a_submission_and_post_is_unsupported() {
    let document = parse_example("forms/form-submit.html");
    let mut state = build_form_state(&document).unwrap();
    let submit = document
        .traverse_depth_first(document.root_id())
        .unwrap()
        .into_iter()
        .find(|&id| document.get_attribute(id, "type").unwrap() == Some("submit"))
        .unwrap();

    let action = click_default_action(&document, &mut state, submit).unwrap();
    let FormDefaultAction::Submit { form, submitter } = action else {
        panic!("expected submit, got {action:?}");
    };
    let base = Url::parse("http://forms.example/page.html").unwrap();
    let submission = build_submission(&document, &mut state, form, Some(submitter), &base).unwrap();
    assert_eq!(submission.action.query.as_deref(), Some("q=mocha&page=1"));

    // The same form with method="post" fails clearly.
    let post = mocha_html::parse_html(
        r#"<form method="post" action="/s"><input name="q" value="x"></form>"#,
    )
    .unwrap();
    let mut post_state = build_form_state(&post).unwrap();
    let post_form = find_tag(&post, "form");
    assert!(matches!(
        build_submission(&post, &mut post_state, post_form, None, &base),
        Err(MochaError::UnsupportedFeature(_))
    ));
}

#[test]
fn controls_are_hit_testable_through_the_full_pipeline() {
    // Find the text input's rect from the display list, then hit-test its center.
    let path = example("forms/basic-form.html");
    let commands = run_file(&path).unwrap();
    let (x, y, w, h) = commands
        .iter()
        .find_map(|c| match c {
            DisplayCommand::DrawControl {
                control_type,
                x,
                y,
                width,
                height,
                ..
            } if control_type == "text" => Some((*x, *y, *width, *height)),
            _ => None,
        })
        .expect("text control painted");
    let hit = hit_test_file(&path, x + w / 2.0, y + h / 2.0)
        .unwrap()
        .expect("hit a node");

    let document = parse_example("forms/basic-form.html");
    assert_eq!(hit, by_id(&document, "q"), "the input itself is hit");
}

#[test]
fn all_previous_examples_still_run() {
    for rel in [
        "basic/index.html",
        "styled/index.html",
        "layout/article.html",
        "layout/inline-wrap.html",
        "layout/box-model.html",
        "js/dom-basic.html",
        "js/dom-style-mutation.html",
        "js/event-listener.html",
        "resources/external-css.html",
        "images/basic-image.html",
        "images/inline-image.html",
        "images/sized-image.html",
    ] {
        run_file(&example(rel)).unwrap_or_else(|error| panic!("{rel} failed: {error}"));
    }
}
