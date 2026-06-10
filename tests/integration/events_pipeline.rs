//! Integration test for the Milestone 5 event pipeline: parse an anchor
//! document, lay it out, hit-test the link text, dispatch a click through the
//! event system, and resolve the navigation default action. No JavaScript and no
//! real window input are involved. Declared as a `[[test]]` of `mocha_shell`.

use mocha_dom::{Document, NodeId, NodeKind};
use mocha_events::{Event, EventDispatcher, EventListenerOptions};
use mocha_layout::{build_layout_tree, hit_test, LayoutBox, LayoutBoxKind, LayoutViewport, Rect};
use mocha_nav::{default_action_for_event, DefaultAction};
use mocha_url::Url;
use std::cell::RefCell;
use std::rc::Rc;

const HTML: &str =
    "<html><body><p>Go <a href=\"http://example.com/next\">here</a> now</p></body></html>";

fn document_and_layout() -> (Document, LayoutBox) {
    let document = mocha_html::parse_html(HTML).unwrap();
    let stylesheets = mocha_style::collect_stylesheets(&document).unwrap();
    let styled = mocha_style::build_style_tree(&document, &stylesheets).unwrap();
    let layout = build_layout_tree(&styled, LayoutViewport::default()).unwrap();
    (document, layout)
}

fn find_tag(document: &Document, tag: &str) -> NodeId {
    document
        .traverse_depth_first(document.root_id())
        .unwrap()
        .into_iter()
        .find(|&id| document.tag_name(id).unwrap() == Some(tag))
        .expect("tag present")
}

/// Find the rect of the text run whose text equals `needle`.
fn text_run_rect(root: &LayoutBox, needle: &str) -> Option<Rect> {
    if let LayoutBoxKind::TextRun(text) = &root.kind {
        if text == needle {
            return Some(root.rect);
        }
    }
    root.children.iter().find_map(|c| text_run_rect(c, needle))
}

#[test]
fn anchor_is_inline_and_link_text_hit_tests_into_the_anchor() {
    let (document, layout) = document_and_layout();
    let anchor = find_tag(&document, "a");

    // The link word "here" was laid out; hit its center.
    let rect = text_run_rect(&layout, "here").expect("link text laid out");
    let hit = hit_test(
        &layout,
        rect.x + rect.width / 2.0,
        rect.y + rect.height / 2.0,
    )
    .expect("hit a node");

    // The hit node is the anchor or a descendant text node of it.
    let is_anchor_or_descendant =
        hit == anchor || document.ancestors(hit).unwrap().contains(&anchor);
    assert!(
        is_anchor_or_descendant,
        "hit {hit:?} should be within the anchor"
    );
}

#[test]
fn click_dispatches_and_resolves_navigation_default_action() {
    let (document, _layout) = document_and_layout();
    let anchor = find_tag(&document, "a");

    // A click on a text node inside the anchor.
    let target = document
        .children(anchor)
        .unwrap()
        .iter()
        .copied()
        .find(|&id| matches!(document.node(id).unwrap().kind, NodeKind::Text(_)))
        .expect("anchor has text");

    // Register a bubble listener on the anchor to confirm dispatch reaches it.
    let seen = Rc::new(RefCell::new(false));
    let mut dispatcher = EventDispatcher::new();
    {
        let seen = Rc::clone(&seen);
        dispatcher.add_event_listener(
            anchor,
            "click",
            EventListenerOptions::bubble(),
            Box::new(move |_e: &mut Event| *seen.borrow_mut() = true),
        );
    }

    let mut event = Event::click(target, 0.0, 0.0);
    dispatcher.dispatch_event(&document, &mut event).unwrap();
    assert!(*seen.borrow(), "click bubbled to the anchor listener");

    // Default action resolves to navigation.
    let base = Url::parse("http://example.com/").unwrap();
    let action = default_action_for_event(&document, &event, Some(&base)).unwrap();
    assert_eq!(
        action,
        DefaultAction::Navigate(Url::parse("http://example.com/next").unwrap())
    );
}

#[test]
fn prevent_default_suppresses_navigation() {
    let (document, _layout) = document_and_layout();
    let anchor = find_tag(&document, "a");

    let mut dispatcher = EventDispatcher::new();
    dispatcher.add_event_listener(
        anchor,
        "click",
        EventListenerOptions::bubble(),
        Box::new(|event: &mut Event| event.prevent_default()),
    );

    let mut event = Event::click(anchor, 0.0, 0.0);
    let result = dispatcher.dispatch_event(&document, &mut event).unwrap();
    assert!(result.default_prevented);

    let action = default_action_for_event(&document, &event, None).unwrap();
    assert_eq!(action, DefaultAction::None);
}
