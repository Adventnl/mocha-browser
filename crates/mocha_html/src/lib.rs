//! Minimal HTML parsing for Mocha Browser: tokenizer plus a stack-based tree
//! builder that produces a [`mocha_dom::Document`].
//!
//! **This is not the [HTML5 tree construction algorithm].** There is no error
//! recovery, no implied tags, and no foster parenting. `<style>` is accepted and
//! its CSS is captured as a text child (extracted later by `mocha_style`);
//! `<script>` is rejected. Only a small set of element names is accepted;
//! anything else is a clear [`MochaError`] rather than a silent skip.
//!
//! [HTML5 tree construction algorithm]: https://html.spec.whatwg.org/multipage/parsing.html#tree-construction

mod tokenizer;

pub use tokenizer::{tokenize, HtmlToken};

use mocha_dom::{Document, NodeId};
use mocha_error::{MochaError, MochaResult};

/// Element names the tree builder accepts.
///
/// `style` is accepted so its CSS text can be extracted later; its contents are
/// not laid out or painted. Encountering any other tag (start or end) is an
/// [`MochaError::UnsupportedFeature`] error, not a silent skip.
pub const SUPPORTED_TAGS: &[&str] = &["html", "body", "h1", "h2", "p", "div", "span", "a", "style"];

/// Parse an HTML source string into a [`Document`].
///
/// The pipeline is: [`tokenize`] then a stack-based tree builder. Mismatched or
/// unclosed tags, and unsupported tags, are reported as errors.
pub fn parse_html(input: &str) -> MochaResult<Document> {
    let tokens = tokenize(input)?;
    build_tree(tokens)
}

fn build_tree(tokens: Vec<HtmlToken>) -> MochaResult<Document> {
    let mut document = Document::new();
    let root = document.root_id();
    // Stack of currently-open elements, paired with their tag names so end tags
    // can be matched. The current insertion parent is the top of the stack, or
    // the document root when the stack is empty.
    let mut open: Vec<(NodeId, String)> = Vec::new();

    for token in tokens {
        match token {
            HtmlToken::Doctype(text) => {
                let node = document.create_doctype(text);
                document.append_child(current_parent(root, &open), node)?;
            }
            HtmlToken::Comment(text) => {
                let node = document.create_comment(text);
                document.append_child(current_parent(root, &open), node)?;
            }
            HtmlToken::Text(text) => {
                let node = document.create_text(text);
                document.append_child(current_parent(root, &open), node)?;
            }
            HtmlToken::StartTag {
                name,
                attributes,
                self_closing,
            } => {
                check_supported(&name)?;
                let element = document.create_element(name.clone(), attributes);
                document.append_child(current_parent(root, &open), element)?;
                if !self_closing {
                    open.push((element, name));
                }
            }
            HtmlToken::EndTag { name } => {
                check_supported(&name)?;
                match open.last() {
                    None => {
                        return Err(MochaError::Parse(format!(
                            "stray closing tag </{name}> with no open element"
                        )));
                    }
                    Some((_, open_name)) if *open_name != name => {
                        return Err(MochaError::Parse(format!(
                            "mismatched closing tag: expected </{open_name}> but found </{name}>"
                        )));
                    }
                    Some(_) => {
                        open.pop();
                    }
                }
            }
        }
    }

    if let Some((_, name)) = open.last() {
        return Err(MochaError::Parse(format!(
            "unclosed tag: <{name}> was never closed"
        )));
    }

    Ok(document)
}

fn current_parent(root: NodeId, open: &[(NodeId, String)]) -> NodeId {
    open.last().map(|(id, _)| *id).unwrap_or(root)
}

fn check_supported(name: &str) -> MochaResult<()> {
    if SUPPORTED_TAGS.contains(&name) {
        return Ok(());
    }
    // `<link rel="stylesheet">` is the natural way to reach for external CSS.
    // Report that specifically rather than as a generic unsupported tag, and
    // never pretend the stylesheet loads.
    if name == "link" {
        return Err(MochaError::UnsupportedFeature(
            "external stylesheets (<link>) are not supported in Milestone 2".to_string(),
        ));
    }
    Err(MochaError::UnsupportedFeature(format!(
        "tag <{name}> is not supported"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_dom::NodeKind;

    /// Collect the text of every [`NodeKind::Text`] node in document order.
    fn collect_text(document: &Document) -> Vec<String> {
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        order
            .into_iter()
            .filter_map(|id| match &document.node(id).unwrap().kind {
                NodeKind::Text(data) => Some(data.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Collect the tag name of every element in document order.
    fn collect_tags(document: &Document) -> Vec<String> {
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        order
            .into_iter()
            .filter_map(|id| match &document.node(id).unwrap().kind {
                NodeKind::Element(data) => Some(data.tag_name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_simple_html_body_h1() {
        let document = parse_html("<html><body><h1>Hello Mocha</h1></body></html>").unwrap();
        assert_eq!(collect_tags(&document), vec!["html", "body", "h1"]);
        assert_eq!(collect_text(&document), vec!["Hello Mocha"]);
    }

    #[test]
    fn parse_nested_div_span_text() {
        let document = parse_html("<div><span>inline</span></div>").unwrap();
        assert_eq!(collect_tags(&document), vec!["div", "span"]);
        assert_eq!(collect_text(&document), vec!["inline"]);

        // The span must be a child of the div, not a sibling.
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let div = order[1];
        let span = order[2];
        assert_eq!(document.parent(span).unwrap(), Some(div));
    }

    #[test]
    fn doctype_and_comment_become_nodes() {
        let document = parse_html("<!doctype html><!-- note --><body></body>").unwrap();
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let kinds: Vec<&NodeKind> = order
            .iter()
            .map(|&id| &document.node(id).unwrap().kind)
            .collect();
        assert!(matches!(kinds[1], NodeKind::Doctype(text) if text == "html"));
        assert!(matches!(kinds[2], NodeKind::Comment(text) if text == "note"));
    }

    #[test]
    fn reject_unsupported_tag() {
        let error = parse_html("<img>").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn reject_mismatched_closing_tag() {
        let error = parse_html("<p></div>").unwrap_err();
        match error {
            MochaError::Parse(message) => {
                assert!(message.contains("expected </p>"));
                assert!(message.contains("found </div>"));
            }
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn reject_unclosed_tag() {
        let error = parse_html("<p>text").unwrap_err();
        match error {
            MochaError::Parse(message) => assert!(message.contains("unclosed")),
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn reject_stray_end_tag() {
        let error = parse_html("</p>").unwrap_err();
        assert!(matches!(error, MochaError::Parse(_)));
    }

    #[test]
    fn parse_style_tag_stores_css_as_text() {
        let document = parse_html("<style>h1 { color: red; }</style>").unwrap();
        assert_eq!(collect_tags(&document), vec!["style"]);
        // The CSS lives as a text child, available for later extraction.
        assert_eq!(collect_text(&document), vec!["h1 { color: red; }"]);
    }

    #[test]
    fn parse_style_tag_with_angle_brackets_in_css() {
        // `<` inside a CSS comment must not break parsing or create elements.
        let document = parse_html("<style>/* <not-a-tag> */ p { color: red; }</style>").unwrap();
        assert_eq!(collect_tags(&document), vec!["style"]);
        assert_eq!(
            collect_text(&document),
            vec!["/* <not-a-tag> */ p { color: red; }"]
        );
    }

    #[test]
    fn unterminated_style_is_rejected() {
        let error = parse_html("<style>p { color: red; }").unwrap_err();
        assert!(matches!(error, MochaError::Parse(_)));
    }

    #[test]
    fn parse_class_and_id_and_inline_style_attributes() {
        let document = parse_html(r#"<p id="x" class="a b" style="color: red;"></p>"#).unwrap();
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        match &document.node(order[1]).unwrap().kind {
            NodeKind::Element(data) => {
                assert_eq!(data.attribute("id"), Some("x"));
                assert_eq!(data.attribute("class"), Some("a b"));
                assert_eq!(data.attribute("style"), Some("color: red;"));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn script_tag_is_rejected() {
        let error = parse_html("<script></script>").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn link_stylesheet_is_clearly_unsupported() {
        let error = parse_html(r#"<link rel="stylesheet">"#).unwrap_err();
        match error {
            MochaError::UnsupportedFeature(message) => {
                assert!(message.contains("external stylesheets"))
            }
            other => panic!("expected UnsupportedFeature, got {other:?}"),
        }
    }

    #[test]
    fn attributes_are_attached_to_elements() {
        let document = parse_html(r#"<div id="main"></div>"#).unwrap();
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        match &document.node(order[1]).unwrap().kind {
            NodeKind::Element(data) => assert_eq!(data.attribute("id"), Some("main")),
            other => panic!("expected element, got {other:?}"),
        }
    }
}
