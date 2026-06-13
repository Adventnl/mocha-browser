//! Minimal but **forgiving** HTML parsing for Mocha Browser: a tokenizer plus a
//! stack-based tree builder that produces a [`mocha_dom::Document`].
//!
//! **This is not the full [HTML5 tree construction algorithm]** — there are no
//! insertion modes and no foster parenting — but, unlike earlier milestones, it
//! now *recovers* from real-world markup instead of rejecting it:
//!
//! - **Any element name is accepted.** Unknown tags become ordinary elements
//!   (they style as `display: block` by default), so a page is never rejected
//!   for containing `<head>`, `<nav>`, `<table>`, `<article>`, … .
//! - **Mismatched and stray end tags recover.** An end tag closes to the nearest
//!   matching open ancestor (auto-closing any unclosed elements in between); an
//!   end tag with no matching open element is ignored.
//! - **A handful of implied end tags** are applied so common optional-tag markup
//!   nests correctly: a block-level start tag closes an open `<p>`, and a new
//!   `<li>`/`<option>`/`<dt>`/`<dd>`/`<tr>`/`<td>`/`<th>` closes the previous one.
//! - **Unclosed tags at end-of-input are auto-closed** rather than erroring.
//!
//! `<style>`/`<script>`/`<textarea>` raw text is captured as a text child
//! (`<style>` CSS is extracted by `mocha_style`; `<script>` JavaScript is run by
//! the engine via `mocha_js_dom`; `<textarea>` text is the control's value).
//! The only errors `parse_html` can still return are DOM-invariant violations.
//!
//! [HTML5 tree construction algorithm]: https://html.spec.whatwg.org/multipage/parsing.html#tree-construction

mod tokenizer;

pub use tokenizer::{tokenize, HtmlToken};

use mocha_dom::{Document, NodeId};
use mocha_error::MochaResult;

/// Void elements have no content and no end tag. They are appended but never
/// pushed onto the open-element stack. This is the full HTML void-element set.
pub const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Inline-level element names, used to decide whether a start tag is "block-like"
/// for implied `<p>` closing. Anything not listed here is treated as block-level.
const INLINE_TAGS: &[&str] = &[
    "a", "span", "em", "strong", "b", "i", "u", "s", "small", "code", "kbd", "samp", "var", "cite",
    "q", "abbr", "mark", "sub", "sup", "time", "img", "label", "input", "button", "textarea",
    "select", "br", "font", "big", "tt", "wbr",
];

/// Parse an HTML source string into a [`Document`].
///
/// The pipeline is [`tokenize`] then a stack-based tree builder. Malformed markup
/// is recovered (see the module docs); the result is always a usable document
/// unless a DOM-tree invariant is violated.
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
                close_implied(&mut open, &name);
                let element = document.create_element(name.clone(), attributes);
                document.append_child(current_parent(root, &open), element)?;
                if !self_closing && !VOID_TAGS.contains(&name.as_str()) {
                    open.push((element, name));
                }
            }
            HtmlToken::EndTag { name } => {
                if name.is_empty() {
                    continue;
                }
                // Close to the nearest matching open ancestor, auto-closing any
                // unclosed elements in between. A stray end tag is ignored.
                if let Some(pos) = open.iter().rposition(|(_, open_name)| *open_name == name) {
                    open.truncate(pos);
                }
            }
        }
    }

    // Any still-open elements at end-of-input are auto-closed (no error).
    Ok(document)
}

fn current_parent(root: NodeId, open: &[(NodeId, String)]) -> NodeId {
    open.last().map(|(id, _)| *id).unwrap_or(root)
}

/// Apply implied end tags before inserting a `new_tag` start tag: pop open
/// elements that HTML would implicitly close. Handles the common optional-tag
/// cases (`p`, list items, definition lists, table rows/cells) — not the full
/// spec — so real-world markup nests the way authors expect.
fn close_implied(open: &mut Vec<(NodeId, String)>, new_tag: &str) {
    while let Some((_, top)) = open.last() {
        let top = top.as_str();
        let implied = match new_tag {
            "li" => top == "li",
            "dt" | "dd" => top == "dt" || top == "dd",
            "option" => top == "option" || top == "optgroup",
            "optgroup" => top == "option" || top == "optgroup",
            "tr" => top == "tr" || top == "td" || top == "th",
            "td" | "th" => top == "td" || top == "th",
            "thead" | "tbody" | "tfoot" => top == "tr" || top == "td" || top == "th",
            _ => false,
        } || (top == "p" && is_block_level(new_tag));
        if implied {
            open.pop();
        } else {
            break;
        }
    }
}

/// Whether a start tag is block-level for the purpose of implied `<p>` closing.
fn is_block_level(tag: &str) -> bool {
    !INLINE_TAGS.contains(&tag)
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
    fn any_tag_is_accepted() {
        // Tags outside the old allow-list now parse as ordinary elements.
        let document = parse_html(
            "<html><head><title>T</title></head><body><nav><ul><li>x</li></ul></nav></body></html>",
        )
        .unwrap();
        let tags = collect_tags(&document);
        for expected in ["html", "head", "title", "body", "nav", "ul", "li"] {
            assert!(tags.contains(&expected.to_string()), "missing <{expected}>");
        }
        assert_eq!(collect_text(&document), vec!["T", "x"]);
    }

    #[test]
    fn img_parses_as_a_void_element_with_attributes() {
        let document = parse_html(
            r#"<html><body><img src="cat.png" alt="A cat" width="100" height="80"><p>after</p></body></html>"#,
        )
        .unwrap();
        assert_eq!(collect_tags(&document), vec!["html", "body", "img", "p"]);
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let img = order
            .iter()
            .find(|&&id| document.tag_name(id).unwrap() == Some("img"))
            .copied()
            .unwrap();
        assert_eq!(document.get_attribute(img, "src").unwrap(), Some("cat.png"));
        assert_eq!(document.get_attribute(img, "alt").unwrap(), Some("A cat"));
        assert_eq!(document.get_attribute(img, "width").unwrap(), Some("100"));
        assert!(document.children(img).unwrap().is_empty());
    }

    #[test]
    fn mismatched_closing_tag_recovers() {
        // `</div>` has no matching open <div>, so it is ignored; the <p> is then
        // auto-closed at end-of-input. No error.
        let document = parse_html("<p>hi</div>").unwrap();
        assert_eq!(collect_tags(&document), vec!["p"]);
        assert_eq!(collect_text(&document), vec!["hi"]);
    }

    #[test]
    fn end_tag_auto_closes_intervening_open_elements() {
        // </div> closes the <span> and <b> opened inside it.
        let document = parse_html("<div><span><b>x</div>after").unwrap();
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let div = order[1];
        // "after" is a sibling of the div (the div closed), not nested inside it.
        let after = order
            .iter()
            .find(|&&id| matches!(&document.node(id).unwrap().kind, NodeKind::Text(t) if t.text == "after"))
            .copied()
            .unwrap();
        assert_eq!(document.parent(after).unwrap(), Some(document.root_id()));
        assert_ne!(document.parent(after).unwrap(), Some(div));
    }

    #[test]
    fn unclosed_tag_is_auto_closed() {
        let document = parse_html("<p>text").unwrap();
        assert_eq!(collect_tags(&document), vec!["p"]);
        assert_eq!(collect_text(&document), vec!["text"]);
    }

    #[test]
    fn stray_end_tag_is_ignored() {
        let document = parse_html("</p>hello").unwrap();
        assert_eq!(collect_tags(&document), Vec::<String>::new());
        assert_eq!(collect_text(&document), vec!["hello"]);
    }

    #[test]
    fn block_start_tag_closes_open_paragraph() {
        // `<p>a<p>b` and `<p>a<div>b` — the second block start closes the first <p>.
        let document = parse_html("<p>a<p>b</p>").unwrap();
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let paragraphs: Vec<_> = order
            .iter()
            .filter(|&&id| document.tag_name(id).unwrap() == Some("p"))
            .collect();
        assert_eq!(paragraphs.len(), 2);
        // The two <p> elements are siblings (the first did not contain the second).
        assert_eq!(
            document.parent(*paragraphs[1]).unwrap(),
            document.parent(*paragraphs[0]).unwrap()
        );
    }

    #[test]
    fn list_item_auto_closes_previous_sibling() {
        let document = parse_html("<ul><li>one<li>two</ul>").unwrap();
        let ul = document
            .traverse_depth_first(document.root_id())
            .unwrap()
            .into_iter()
            .find(|&id| document.tag_name(id).unwrap() == Some("ul"))
            .unwrap();
        // Two <li> children, not nested.
        assert_eq!(document.children(ul).unwrap().len(), 2);
    }

    #[test]
    fn full_void_set_does_not_capture_following_siblings() {
        let document =
            parse_html("<body><br><hr><meta charset=\"utf-8\"><p>after</p></body>").unwrap();
        assert_eq!(
            collect_tags(&document),
            vec!["body", "br", "hr", "meta", "p"]
        );
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
    fn unterminated_style_recovers() {
        // No `</style>`: the CSS is captured to EOF as the style's text child.
        let document = parse_html("<style>p { color: red; }").unwrap();
        assert_eq!(collect_tags(&document), vec!["style"]);
        assert_eq!(collect_text(&document), vec!["p { color: red; }"]);
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
    fn script_tag_is_captured_as_raw_text() {
        // `<` inside script source must survive and not start an HTML tag.
        let document = parse_html("<script>if (1 < 2) { x; }</script>").unwrap();
        assert_eq!(collect_tags(&document), vec!["script"]);
        assert_eq!(collect_text(&document), vec!["if (1 < 2) { x; }"]);
    }

    #[test]
    fn unterminated_script_recovers() {
        let document = parse_html("<script>doStuff();").unwrap();
        assert_eq!(collect_tags(&document), vec!["script"]);
        assert_eq!(collect_text(&document), vec!["doStuff();"]);
    }

    #[test]
    fn script_with_src_attribute_parses_as_element() {
        // The parser accepts `<script src=...>`; rejecting external scripts is the
        // job of script collection in the execution pipeline, not the parser.
        let document = parse_html(r#"<script src="app.js"></script>"#).unwrap();
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        assert_eq!(
            document.get_attribute(order[1], "src").unwrap(),
            Some("app.js")
        );
    }

    #[test]
    fn link_parses_as_a_void_element_without_a_close_tag() {
        // No `</link>` is required; the link sits at body level with siblings.
        let document = parse_html(
            r#"<html><body><link rel="stylesheet" href="a.css"><p>after</p></body></html>"#,
        )
        .unwrap();
        assert_eq!(collect_tags(&document), vec!["html", "body", "link", "p"]);
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let link = order
            .iter()
            .find(|&&id| document.tag_name(id).unwrap() == Some("link"))
            .copied()
            .unwrap();
        assert_eq!(
            document.get_attribute(link, "rel").unwrap(),
            Some("stylesheet")
        );
        assert_eq!(document.get_attribute(link, "href").unwrap(), Some("a.css"));
        // The link did not capture the following <p> as a child (it is void).
        assert!(document.children(link).unwrap().is_empty());
    }

    #[test]
    fn parse_form_with_action_and_method() {
        let document =
            parse_html(r#"<form action="/search" method="get"><input name="q"></form>"#).unwrap();
        assert_eq!(collect_tags(&document), vec!["form", "input"]);
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let form = order[1];
        assert_eq!(
            document.get_attribute(form, "action").unwrap(),
            Some("/search")
        );
        assert_eq!(document.get_attribute(form, "method").unwrap(), Some("get"));
    }

    #[test]
    fn input_parses_as_a_void_element() {
        // No </input> is required; the following <p> is a sibling, not a child.
        let document =
            parse_html(r#"<form><input type="text" name="q" value="mocha"><p>after</p></form>"#)
                .unwrap();
        assert_eq!(collect_tags(&document), vec!["form", "input", "p"]);
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let input = order[2];
        assert_eq!(document.tag_name(input).unwrap(), Some("input"));
        assert!(document.children(input).unwrap().is_empty());
        assert_eq!(
            document.get_attribute(input, "value").unwrap(),
            Some("mocha")
        );
    }

    #[test]
    fn checkbox_checked_attribute_parses_as_valueless() {
        let document = parse_html(r#"<input type="checkbox" name="agree" checked>"#).unwrap();
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let input = order[1];
        assert_eq!(document.get_attribute(input, "checked").unwrap(), Some(""));
        assert_eq!(
            document.get_attribute(input, "type").unwrap(),
            Some("checkbox")
        );
    }

    #[test]
    fn button_and_label_parse_with_attributes() {
        let document = parse_html(
            r#"<label for="q">Search</label><button type="submit" name="go">Go</button>"#,
        )
        .unwrap();
        assert_eq!(collect_tags(&document), vec!["label", "button"]);
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let label = order[1];
        assert_eq!(document.get_attribute(label, "for").unwrap(), Some("q"));
        let button = order[3];
        assert_eq!(
            document.get_attribute(button, "type").unwrap(),
            Some("submit")
        );
        assert_eq!(document.text_content(button).unwrap(), "Go");
    }

    #[test]
    fn textarea_content_is_raw_text_with_whitespace_preserved() {
        // The body must not be tokenized as HTML and must keep its whitespace
        // verbatim — it becomes the control's initial value.
        let document =
            parse_html("<textarea name=\"m\">Hello  <world>\n  line2</textarea>").unwrap();
        assert_eq!(collect_tags(&document), vec!["textarea"]);
        assert_eq!(collect_text(&document), vec!["Hello  <world>\n  line2"]);
    }

    #[test]
    fn unterminated_textarea_recovers() {
        let document = parse_html("<textarea>oops").unwrap();
        assert_eq!(collect_tags(&document), vec!["textarea"]);
        assert_eq!(collect_text(&document), vec!["oops"]);
    }

    #[test]
    fn select_and_options_parse_as_children() {
        let document = parse_html(
            r#"<select name="choice"><option value="a">Alpha</option><option value="b" selected>Beta</option></select>"#,
        )
        .unwrap();
        assert_eq!(collect_tags(&document), vec!["select", "option", "option"]);
        let order = document.traverse_depth_first(document.root_id()).unwrap();
        let select = order[1];
        let options: Vec<_> = document.children(select).unwrap().to_vec();
        assert_eq!(options.len(), 2);
        assert_eq!(
            document.get_attribute(options[1], "selected").unwrap(),
            Some("")
        );
        assert_eq!(document.text_content(options[0]).unwrap(), "Alpha");
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
