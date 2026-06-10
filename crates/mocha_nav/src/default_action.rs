//! Default-action interpretation for events.
//!
//! This lives in `mocha_nav` (not `mocha_events`) so the event core stays free of
//! URL/navigation knowledge. Only one default action is modelled in Milestone 5:
//! a `click` on (or inside) an `<a href>` produces a navigation.

use mocha_dom::Document;
use mocha_error::MochaResult;
use mocha_events::Event;
use mocha_url::Url;

/// The default action implied by an event after listeners have run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultAction {
    /// Nothing to do.
    None,
    /// Navigate to a URL (from clicking a link).
    Navigate(Url),
}

/// Determine the default action for `event`.
///
/// Returns [`DefaultAction::Navigate`] when an un-prevented `click` targets an
/// `<a href>` (or a descendant of one) and the href resolves to a URL. `href` is
/// resolved against `base_url` when given; without a base, only absolute hrefs
/// resolve (relative ones yield [`DefaultAction::None`]).
pub fn default_action_for_event(
    document: &Document,
    event: &Event,
    base_url: Option<&Url>,
) -> MochaResult<DefaultAction> {
    if event.event_type != "click" || event.default_prevented {
        return Ok(DefaultAction::None);
    }

    // The target itself, then each ancestor (nearest first).
    let mut chain = vec![event.target];
    chain.extend(document.ancestors(event.target)?);

    for node in chain {
        if document.tag_name(node)? != Some("a") {
            continue;
        }
        let Some(href) = document.get_attribute(node, "href")? else {
            continue;
        };
        let url = match base_url {
            Some(base) => base.join(href)?,
            None if href.contains("://") => Url::parse(href)?,
            None => return Ok(DefaultAction::None), // can't resolve a relative href
        };
        return Ok(DefaultAction::Navigate(url));
    }

    Ok(DefaultAction::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_dom::{Attribute, NodeId};

    /// Build root -> a(href) -> span -> text, returning (document, a, span).
    fn anchor_document(href: &str) -> (Document, NodeId, NodeId) {
        let mut document = Document::new();
        let root = document.root_id();
        let anchor = document.create_element(
            "a",
            vec![Attribute {
                name: "href".to_string(),
                value: href.to_string(),
            }],
        );
        let span = document.create_element("span", Vec::new());
        let text = document.create_text("link");
        document.append_child(root, anchor).unwrap();
        document.append_child(anchor, span).unwrap();
        document.append_child(span, text).unwrap();
        (document, anchor, span)
    }

    #[test]
    fn click_on_anchor_navigates() {
        let (document, anchor, _span) = anchor_document("http://example.com/p");
        let event = Event::click(anchor, 0.0, 0.0);
        let action = default_action_for_event(&document, &event, None).unwrap();
        assert_eq!(
            action,
            DefaultAction::Navigate(Url::parse("http://example.com/p").unwrap())
        );
    }

    #[test]
    fn click_inside_anchor_finds_nearest_anchor() {
        let (document, _anchor, span) = anchor_document("http://example.com/p");
        // Click targets the inner span, not the anchor itself.
        let event = Event::click(span, 0.0, 0.0);
        let action = default_action_for_event(&document, &event, None).unwrap();
        assert!(matches!(action, DefaultAction::Navigate(_)));
    }

    #[test]
    fn relative_href_resolves_against_base() {
        let (document, anchor, _span) = anchor_document("page2.html");
        let base = Url::parse("http://example.com/dir/page1.html").unwrap();
        let event = Event::click(anchor, 0.0, 0.0);
        let action = default_action_for_event(&document, &event, Some(&base)).unwrap();
        assert_eq!(
            action,
            DefaultAction::Navigate(Url::parse("http://example.com/dir/page2.html").unwrap())
        );
    }

    #[test]
    fn prevent_default_suppresses_navigation() {
        let (document, anchor, _span) = anchor_document("http://example.com/p");
        let mut event = Event::click(anchor, 0.0, 0.0);
        event.prevent_default();
        let action = default_action_for_event(&document, &event, None).unwrap();
        assert_eq!(action, DefaultAction::None);
    }

    #[test]
    fn click_on_non_anchor_is_none() {
        let mut document = Document::new();
        let root = document.root_id();
        let p = document.create_element("p", Vec::new());
        document.append_child(root, p).unwrap();
        let event = Event::click(p, 0.0, 0.0);
        assert_eq!(
            default_action_for_event(&document, &event, None).unwrap(),
            DefaultAction::None
        );
    }

    #[test]
    fn non_click_event_is_none() {
        let (document, anchor, _span) = anchor_document("http://example.com/p");
        let event = Event::new("mousedown", anchor);
        assert_eq!(
            default_action_for_event(&document, &event, None).unwrap(),
            DefaultAction::None
        );
    }
}
