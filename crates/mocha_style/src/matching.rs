//! Selector matching against DOM elements.
//!
//! Matching works on lightweight [`ElementDescriptor`]s (tag, id, classes)
//! rather than touching the DOM directly during the inner loop. Only the
//! supported selector grammar is matched: type, class, id, universal, and the
//! descendant combinator.

use mocha_css::{CompoundSelector, Selector, SimpleSelector};
use mocha_dom::ElementData;

/// The match-relevant facts about one element.
#[derive(Debug, Clone)]
pub(crate) struct ElementDescriptor {
    tag: String,
    id: Option<String>,
    classes: Vec<String>,
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
        }
    }
}

/// Does `selector` match `element`, given its `ancestors` (root first, parent
/// last)? The rightmost compound must match the element; each earlier compound
/// must match some ancestor, respecting order (descendant semantics).
pub(crate) fn selector_matches(
    selector: &Selector,
    element: &ElementDescriptor,
    ancestors: &[ElementDescriptor],
) -> bool {
    let parts = &selector.parts;
    let Some(target) = parts.last() else {
        return false;
    };
    if !compound_matches(target, element) {
        return false;
    }

    // Walk the remaining compounds right-to-left, consuming ancestors from the
    // closest parent upward.
    let mut ancestor_index = ancestors.len();
    for part in parts[..parts.len() - 1].iter().rev() {
        let mut found = false;
        while ancestor_index > 0 {
            ancestor_index -= 1;
            if compound_matches(part, &ancestors[ancestor_index]) {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

fn compound_matches(compound: &CompoundSelector, element: &ElementDescriptor) -> bool {
    compound
        .simple_selectors
        .iter()
        .all(|simple| simple_matches(simple, element))
}

fn simple_matches(simple: &SimpleSelector, element: &ElementDescriptor) -> bool {
    match simple {
        SimpleSelector::Universal => true,
        SimpleSelector::Type(name) => element.tag.eq_ignore_ascii_case(name),
        SimpleSelector::Class(name) => element.classes.iter().any(|c| c == name),
        SimpleSelector::Id(name) => element.id.as_deref() == Some(name.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_css::{CompoundSelector, Selector, SimpleSelector};
    use mocha_dom::Attribute;

    fn descriptor(tag: &str, id: Option<&str>, classes: &[&str]) -> ElementDescriptor {
        let mut attributes = Vec::new();
        if let Some(id) = id {
            attributes.push(Attribute {
                name: "id".into(),
                value: id.into(),
            });
        }
        if !classes.is_empty() {
            attributes.push(Attribute {
                name: "class".into(),
                value: classes.join(" "),
            });
        }
        ElementDescriptor::from_element(&ElementData {
            tag_name: tag.into(),
            attributes,
        })
    }

    fn type_sel(name: &str) -> Selector {
        Selector {
            parts: vec![CompoundSelector {
                simple_selectors: vec![SimpleSelector::Type(name.into())],
            }],
        }
    }

    #[test]
    fn type_selector_matches() {
        assert!(selector_matches(
            &type_sel("p"),
            &descriptor("p", None, &[]),
            &[]
        ));
        assert!(!selector_matches(
            &type_sel("p"),
            &descriptor("div", None, &[]),
            &[]
        ));
    }

    #[test]
    fn class_selector_matches_one_of_many() {
        let sel = Selector {
            parts: vec![CompoundSelector {
                simple_selectors: vec![SimpleSelector::Class("warning".into())],
            }],
        };
        assert!(selector_matches(
            &sel,
            &descriptor("p", None, &["note", "warning"]),
            &[]
        ));
    }

    #[test]
    fn id_selector_matches() {
        let sel = Selector {
            parts: vec![CompoundSelector {
                simple_selectors: vec![SimpleSelector::Id("hero".into())],
            }],
        };
        assert!(selector_matches(
            &sel,
            &descriptor("p", Some("hero"), &[]),
            &[]
        ));
        assert!(!selector_matches(
            &sel,
            &descriptor("p", Some("other"), &[]),
            &[]
        ));
    }

    #[test]
    fn descendant_selector_matches_non_contiguous_ancestor() {
        // div p, where p is nested inside section inside div.
        let sel = Selector {
            parts: vec![
                CompoundSelector {
                    simple_selectors: vec![SimpleSelector::Type("div".into())],
                },
                CompoundSelector {
                    simple_selectors: vec![SimpleSelector::Type("p".into())],
                },
            ],
        };
        let ancestors = vec![
            descriptor("div", None, &[]),
            descriptor("section", None, &[]),
        ];
        assert!(selector_matches(
            &sel,
            &descriptor("p", None, &[]),
            &ancestors
        ));
        // No div ancestor → no match.
        let ancestors = vec![descriptor("section", None, &[])];
        assert!(!selector_matches(
            &sel,
            &descriptor("p", None, &[]),
            &ancestors
        ));
    }
}
