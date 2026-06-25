//! Selector matching against the DOM.
//!
//! Matching navigates the document tree by [`NodeId`] so it can resolve
//! combinators (descendant/child/next-sibling/subsequent-sibling) and structural
//! pseudo-classes (`:first-child`, `:nth-child`, `:not`, …), which need access to
//! an element's parent and siblings — not just a precomputed ancestor chain.
//!
//! A complex selector is matched right-to-left: the rightmost compound must match
//! the target element, then each combinator constrains where the next compound to
//! its left may match (an ancestor, the parent, a preceding sibling, …). Dynamic
//! pseudo-classes (`:hover`) and pseudo-elements parse to
//! [`PseudoClass::Inert`](mocha_css::PseudoClass::Inert) and never match.

use mocha_css::{AttributeMatch, AttributeSelector, Combinator, CompoundSelector, PseudoClass};
use mocha_css::{Selector, SimpleSelector};
use mocha_dom::{Document, ElementData, NodeId, NodeKind};
use mocha_error::MochaResult;

/// The match-relevant facts about one element (its own simple-selector inputs).
/// Structural pseudo-classes and combinators are resolved against the live tree,
/// not this descriptor.
#[derive(Debug, Clone)]
pub(crate) struct ElementDescriptor {
    tag: String,
    id: Option<String>,
    classes: Vec<String>,
    /// All attributes, with lowercased names (HTML attribute names are
    /// case-insensitive; values are matched case-sensitively).
    attributes: Vec<(String, String)>,
}

impl ElementDescriptor {
    pub(crate) fn from_element(data: &ElementData) -> ElementDescriptor {
        ElementDescriptor {
            tag: data.tag_name.clone(),
            id: data.attribute("id").map(|value| value.to_string()),
            classes: data
                .attribute("class")
                .map(|value| value.split_whitespace().map(|c| c.to_string()).collect())
                .unwrap_or_default(),
            attributes: data
                .attributes
                .iter()
                .map(|attr| (attr.name.to_ascii_lowercase(), attr.value.clone()))
                .collect(),
        }
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(attr_name, _)| attr_name == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Does `selector` match the element at `node` in `document`?
pub(crate) fn selector_matches(
    document: &Document,
    node: NodeId,
    selector: &Selector,
) -> MochaResult<bool> {
    if selector.parts.is_empty() {
        return Ok(false);
    }
    matches_from(document, node, selector, selector.parts.len() - 1)
}

/// Does `parts[index]` — and everything to its left, via the combinators —
/// match `node`?
fn matches_from(
    document: &Document,
    node: NodeId,
    selector: &Selector,
    index: usize,
) -> MochaResult<bool> {
    if !compound_matches(document, node, &selector.parts[index])? {
        return Ok(false);
    }
    if index == 0 {
        return Ok(true);
    }
    let combinator = selector.combinators[index - 1];
    let next = index - 1;
    match combinator {
        Combinator::Descendant => {
            let mut current = element_parent(document, node)?;
            while let Some(ancestor) = current {
                if matches_from(document, ancestor, selector, next)? {
                    return Ok(true);
                }
                current = element_parent(document, ancestor)?;
            }
            Ok(false)
        }
        Combinator::Child => match element_parent(document, node)? {
            Some(parent) => matches_from(document, parent, selector, next),
            None => Ok(false),
        },
        Combinator::NextSibling => match previous_element_sibling(document, node)? {
            Some(sibling) => matches_from(document, sibling, selector, next),
            None => Ok(false),
        },
        Combinator::SubsequentSibling => {
            let mut current = previous_element_sibling(document, node)?;
            while let Some(sibling) = current {
                if matches_from(document, sibling, selector, next)? {
                    return Ok(true);
                }
                current = previous_element_sibling(document, sibling)?;
            }
            Ok(false)
        }
    }
}

fn compound_matches(
    document: &Document,
    node: NodeId,
    compound: &CompoundSelector,
) -> MochaResult<bool> {
    let NodeKind::Element(data) = &document.node(node)?.kind else {
        return Ok(false);
    };
    let descriptor = ElementDescriptor::from_element(data);
    for simple in &compound.simple_selectors {
        if !simple_matches(document, node, &descriptor, simple)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn simple_matches(
    document: &Document,
    node: NodeId,
    element: &ElementDescriptor,
    simple: &SimpleSelector,
) -> MochaResult<bool> {
    Ok(match simple {
        SimpleSelector::Universal => true,
        SimpleSelector::Type(name) => element.tag.eq_ignore_ascii_case(name),
        SimpleSelector::Class(name) => element.classes.iter().any(|c| c == name),
        SimpleSelector::Id(name) => element.id.as_deref() == Some(name.as_str()),
        SimpleSelector::Attribute(attribute) => attribute_matches(element, attribute),
        SimpleSelector::PseudoClass(pseudo) => pseudo_matches(document, node, element, pseudo)?,
    })
}

fn attribute_matches(element: &ElementDescriptor, attribute: &AttributeSelector) -> bool {
    let Some(value) = element.attribute(&attribute.name) else {
        return false;
    };
    match &attribute.matcher {
        AttributeMatch::Exists => true,
        AttributeMatch::Equals(expected) => value == expected,
        AttributeMatch::Includes(word) => {
            !word.is_empty() && value.split_whitespace().any(|w| w == word)
        }
        AttributeMatch::DashMatch(prefix) => {
            value == prefix || value.starts_with(&format!("{prefix}-"))
        }
        AttributeMatch::Prefix(prefix) => !prefix.is_empty() && value.starts_with(prefix.as_str()),
        AttributeMatch::Suffix(suffix) => !suffix.is_empty() && value.ends_with(suffix.as_str()),
        AttributeMatch::Substring(part) => !part.is_empty() && value.contains(part.as_str()),
    }
}

fn pseudo_matches(
    document: &Document,
    node: NodeId,
    element: &ElementDescriptor,
    pseudo: &PseudoClass,
) -> MochaResult<bool> {
    Ok(match pseudo {
        PseudoClass::Root => document.parent(node)? == Some(document.root_id()),
        PseudoClass::Empty => is_empty_element(document, node)?,
        PseudoClass::FirstChild => child_index(document, node)? == Some(1),
        PseudoClass::LastChild => {
            matches!(child_index(document, node)?, Some(i) if i == element_sibling_count(document, node)?)
        }
        PseudoClass::OnlyChild => element_sibling_count(document, node)? == 1,
        PseudoClass::FirstOfType => of_type_index(document, node, &element.tag)? == Some(1),
        PseudoClass::LastOfType => {
            matches!(of_type_index(document, node, &element.tag)?, Some(i) if i == of_type_count(document, node, &element.tag)?)
        }
        PseudoClass::OnlyOfType => of_type_count(document, node, &element.tag)? == 1,
        PseudoClass::NthChild(nth) => match child_index(document, node)? {
            Some(i) => nth.matches(i as i32),
            None => false,
        },
        PseudoClass::NthLastChild(nth) => match child_index(document, node)? {
            Some(i) => {
                let count = element_sibling_count(document, node)?;
                nth.matches((count - i + 1) as i32)
            }
            None => false,
        },
        PseudoClass::NthOfType(nth) => match of_type_index(document, node, &element.tag)? {
            Some(i) => nth.matches(i as i32),
            None => false,
        },
        PseudoClass::NthLastOfType(nth) => match of_type_index(document, node, &element.tag)? {
            Some(i) => {
                let count = of_type_count(document, node, &element.tag)?;
                nth.matches((count - i + 1) as i32)
            }
            None => false,
        },
        PseudoClass::Not(inner) => {
            let mut all = true;
            for simple in inner {
                if !simple_matches(document, node, element, simple)? {
                    all = false;
                    break;
                }
            }
            !all
        }
        // Dynamic pseudo-classes and pseudo-elements never match a static tree.
        PseudoClass::Inert(_) => false,
    })
}

/// The parent of `node`, if it is an element (not the document root or a
/// non-element node).
fn element_parent(document: &Document, node: NodeId) -> MochaResult<Option<NodeId>> {
    match document.parent(node)? {
        Some(parent) if matches!(document.node(parent)?.kind, NodeKind::Element(_)) => {
            Ok(Some(parent))
        }
        _ => Ok(None),
    }
}

/// The element children of `node`'s parent, in document order. An element with no
/// parent is treated as its own only sibling.
fn element_siblings(document: &Document, node: NodeId) -> MochaResult<Vec<NodeId>> {
    let Some(parent) = document.parent(node)? else {
        return Ok(vec![node]);
    };
    let mut siblings = Vec::new();
    for &child in document.children(parent)? {
        if matches!(document.node(child)?.kind, NodeKind::Element(_)) {
            siblings.push(child);
        }
    }
    Ok(siblings)
}

/// The immediately preceding element sibling of `node`, if any.
fn previous_element_sibling(document: &Document, node: NodeId) -> MochaResult<Option<NodeId>> {
    let siblings = element_siblings(document, node)?;
    let Some(position) = siblings.iter().position(|&s| s == node) else {
        return Ok(None);
    };
    Ok(position.checked_sub(1).map(|prev| siblings[prev]))
}

/// `node`'s 1-based index among its element siblings.
fn child_index(document: &Document, node: NodeId) -> MochaResult<Option<usize>> {
    let siblings = element_siblings(document, node)?;
    Ok(siblings.iter().position(|&s| s == node).map(|p| p + 1))
}

/// The number of element siblings (including `node`).
fn element_sibling_count(document: &Document, node: NodeId) -> MochaResult<usize> {
    Ok(element_siblings(document, node)?.len())
}

/// `node`'s 1-based index among its same-tag element siblings.
fn of_type_index(document: &Document, node: NodeId, tag: &str) -> MochaResult<Option<usize>> {
    let mut index = 0;
    for sibling in element_siblings(document, node)? {
        if tag_eq(document, sibling, tag)? {
            index += 1;
        }
        if sibling == node {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

/// The number of same-tag element siblings (including `node`).
fn of_type_count(document: &Document, node: NodeId, tag: &str) -> MochaResult<usize> {
    let mut count = 0;
    for sibling in element_siblings(document, node)? {
        if tag_eq(document, sibling, tag)? {
            count += 1;
        }
    }
    Ok(count)
}

fn tag_eq(document: &Document, node: NodeId, tag: &str) -> MochaResult<bool> {
    Ok(match &document.node(node)?.kind {
        NodeKind::Element(data) => data.tag_name.eq_ignore_ascii_case(tag),
        _ => false,
    })
}

/// Does `node` have no element children and no non-whitespace text?
fn is_empty_element(document: &Document, node: NodeId) -> MochaResult<bool> {
    for &child in document.children(node)? {
        match &document.node(child)?.kind {
            NodeKind::Element(_) => return Ok(false),
            NodeKind::Text(text) if !text.text.trim().is_empty() => return Ok(false),
            _ => {}
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_css::parse_selector_list;
    use mocha_html::parse_html;

    /// Parse `html`, then assert that exactly the nodes whose text content is in
    /// `expected` match `selector` (order-independent). Text is used as an easy
    /// stable label for each element in the fixtures below.
    fn assert_matches(html: &str, selector: &str, expected: &[&str]) {
        let document = parse_html(html).unwrap();
        let selectors = parse_selector_list(selector).unwrap();
        let mut got = Vec::new();
        for id in document.traverse_depth_first(document.root_id()).unwrap() {
            if matches!(document.node(id).unwrap().kind, NodeKind::Element(_))
                && selectors
                    .iter()
                    .any(|s| selector_matches(&document, id, s).unwrap())
            {
                got.push(document.text_content(id).unwrap());
            }
        }
        let mut got_sorted = got.clone();
        got_sorted.sort();
        let mut expected_sorted: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        expected_sorted.sort();
        assert_eq!(
            got_sorted, expected_sorted,
            "selector `{selector}` matched {got:?}"
        );
    }

    #[test]
    fn type_class_id_still_match() {
        let html = r#"<p id="a" class="note x">one</p><p>two</p>"#;
        assert_matches(html, "p", &["one", "two"]);
        assert_matches(html, ".note", &["one"]);
        assert_matches(html, "#a", &["one"]);
        assert_matches(html, "p.x#a", &["one"]);
    }

    #[test]
    fn child_combinator_requires_immediate_parent() {
        let html = "<div><p>direct</p><section><p>nested</p></section></div>";
        assert_matches(html, "div > p", &["direct"]);
        assert_matches(html, "div p", &["direct", "nested"]);
    }

    #[test]
    fn sibling_combinators() {
        let html = "<h1>h</h1><p>first</p><p>second</p><span>s</span>";
        // Adjacent: only the p immediately after the h1.
        assert_matches(html, "h1 + p", &["first"]);
        // Subsequent: every sibling p (and span) after the h1.
        assert_matches(html, "h1 ~ p", &["first", "second"]);
    }

    #[test]
    fn attribute_selectors() {
        let html = r#"<input type="text" value="x"><input type="checkbox"><a href="/foo" lang="en-US">l</a>"#;
        // Only the first input has a `value` attribute (inputs have no text).
        assert_matches(html, "[value]", &[""]);
        assert_matches(html, r#"[type="text"]"#, &[""]);
        assert_matches(html, r#"[href^="/"]"#, &["l"]);
        assert_matches(html, r#"[href$="foo"]"#, &["l"]);
        assert_matches(html, r#"[href*="o"]"#, &["l"]);
        assert_matches(html, r#"[lang|="en"]"#, &["l"]);
    }

    #[test]
    fn structural_pseudo_classes() {
        let html = "<ul><li>1</li><li>2</li><li>3</li><li>4</li></ul>";
        assert_matches(html, "li:first-child", &["1"]);
        assert_matches(html, "li:last-child", &["4"]);
        assert_matches(html, "li:nth-child(2)", &["2"]);
        assert_matches(html, "li:nth-child(odd)", &["1", "3"]);
        assert_matches(html, "li:nth-child(2n)", &["2", "4"]);
        assert_matches(html, "li:nth-last-child(1)", &["4"]);
    }

    #[test]
    fn not_and_root_and_inert() {
        let html = r#"<div class="keep">a</div><div class="drop">b</div>"#;
        assert_matches(html, "div:not(.drop)", &["a"]);
        // :hover is inert — it never matches in a static render.
        assert_matches(html, "div:hover", &[]);
    }
}
