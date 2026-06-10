//! Computed style for Mocha Browser: extract `<style>` CSS, match selectors,
//! run a small cascade with inheritance, and produce a styled tree.
//!
//! The cascade order is: user-agent defaults → author rules from `<style>` →
//! inline `style` attributes. Within author rules, higher specificity wins and
//! ties are broken by source order (later wins). The inherited properties are
//! `color`, `font-size`, and `font-weight`; everything else uses its initial
//! value when unset. This crate owns the default styles that `mocha_layout` used
//! to hard-code. It does **no layout geometry**.

mod matching;

pub use mocha_css::Color;
pub use mocha_dom::NodeId;

use std::collections::HashMap;

use matching::{selector_matches, ElementDescriptor};
use mocha_css::{
    parse_inline_style, parse_selector_list, parse_stylesheet, CssProperty, CssValue, Declaration,
    Selector, Specificity, Stylesheet,
};
use mocha_dom::{Document, ElementData, NodeKind};
use mocha_error::MochaResult;

/// The computed `display` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// Block-level.
    Block,
    /// Inline-level.
    Inline,
    /// Generates no box.
    None,
}

/// The computed `font-weight` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    /// Normal weight.
    Normal,
    /// Bold weight.
    Bold,
}

/// Per-side lengths used for `margin` and `padding`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeSizes {
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Left edge.
    pub left: f32,
}

/// The fully resolved style for one node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    /// `display`
    pub display: Display,
    /// `color` (inherited).
    pub color: Color,
    /// `background-color` (not inherited).
    pub background_color: Color,
    /// `font-size` in pixels (inherited).
    pub font_size: f32,
    /// `font-weight` (inherited).
    pub font_weight: FontWeight,
    /// `width` in pixels, or `None` for auto.
    pub width: Option<f32>,
    /// `height` in pixels, or `None` for auto.
    pub height: Option<f32>,
    /// Resolved margins.
    pub margin: EdgeSizes,
    /// Resolved paddings.
    pub padding: EdgeSizes,
    /// `border-width` in pixels.
    pub border_width: f32,
    /// `border-color` (defaults to the computed `color`).
    pub border_color: Color,
}

impl ComputedStyle {
    /// The initial style used for the document root and as the inheritance base.
    pub fn initial() -> ComputedStyle {
        ComputedStyle {
            display: Display::Block,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            font_size: 16.0,
            font_weight: FontWeight::Normal,
            width: None,
            height: None,
            margin: EdgeSizes::default(),
            padding: EdgeSizes::default(),
            border_width: 0.0,
            border_color: Color::BLACK,
        }
    }

    /// The computed style of a text node: inherited properties from its parent,
    /// initial values for the rest, and `display: inline`.
    fn for_text(parent: &ComputedStyle) -> ComputedStyle {
        ComputedStyle {
            display: Display::Inline,
            color: parent.color,
            background_color: Color::TRANSPARENT,
            font_size: parent.font_size,
            font_weight: parent.font_weight,
            width: None,
            height: None,
            margin: EdgeSizes::default(),
            padding: EdgeSizes::default(),
            border_width: 0.0,
            border_color: parent.color,
        }
    }

    /// Build a computed style from the winning specified values plus the parent
    /// (for inherited properties not explicitly set).
    fn from_values(
        values: &HashMap<CssProperty, CssValue>,
        parent: &ComputedStyle,
    ) -> ComputedStyle {
        let color = values
            .get(&CssProperty::Color)
            .and_then(as_color)
            .unwrap_or(parent.color);
        ComputedStyle {
            display: values
                .get(&CssProperty::Display)
                .and_then(as_display)
                .unwrap_or(Display::Inline),
            color,
            background_color: values
                .get(&CssProperty::BackgroundColor)
                .and_then(as_color)
                .unwrap_or(Color::TRANSPARENT),
            font_size: values
                .get(&CssProperty::FontSize)
                .and_then(as_length)
                .unwrap_or(parent.font_size),
            font_weight: values
                .get(&CssProperty::FontWeight)
                .and_then(as_font_weight)
                .unwrap_or(parent.font_weight),
            width: values.get(&CssProperty::Width).and_then(as_length),
            height: values.get(&CssProperty::Height).and_then(as_length),
            margin: EdgeSizes {
                top: edge(values, CssProperty::MarginTop),
                right: edge(values, CssProperty::MarginRight),
                bottom: edge(values, CssProperty::MarginBottom),
                left: edge(values, CssProperty::MarginLeft),
            },
            padding: EdgeSizes {
                top: edge(values, CssProperty::PaddingTop),
                right: edge(values, CssProperty::PaddingRight),
                bottom: edge(values, CssProperty::PaddingBottom),
                left: edge(values, CssProperty::PaddingLeft),
            },
            border_width: values
                .get(&CssProperty::BorderWidth)
                .and_then(as_length)
                .unwrap_or(0.0),
            // `border-color` defaults to the element's own color (like currentColor).
            border_color: values
                .get(&CssProperty::BorderColor)
                .and_then(as_color)
                .unwrap_or(color),
        }
    }
}

/// A decoded replaced element's final box: which image to paint and the resolved
/// content-box size (after applying CSS, then attributes, then intrinsic size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplacedBox {
    /// Index into the document's image store.
    pub image_id: usize,
    /// Final content width in pixels.
    pub width: f32,
    /// Final content height in pixels.
    pub height: f32,
}

/// A DOM node with its computed style and styled children.
#[derive(Debug, Clone, PartialEq)]
pub struct StyledNode {
    /// The source DOM node.
    pub node_id: NodeId,
    /// `Some(text)` for text nodes; `None` for elements and the document root.
    pub text: Option<String>,
    /// The computed style.
    pub style: ComputedStyle,
    /// For a successfully-loaded replaced element (`<img>`), its image box.
    /// `None` for every other node.
    pub replaced: Option<ReplacedBox>,
    /// Styled children (comments/doctypes are dropped).
    pub children: Vec<StyledNode>,
}

/// Collect and parse every `<style>` element's CSS, in document order.
pub fn collect_stylesheets(document: &Document) -> MochaResult<Vec<Stylesheet>> {
    let mut stylesheets = Vec::new();
    for id in document.traverse_depth_first(document.root_id())? {
        let NodeKind::Element(data) = &document.node(id)?.kind else {
            continue;
        };
        if data.tag_name != "style" {
            continue;
        }
        let mut css = String::new();
        for &child in document.children(id)? {
            if let NodeKind::Text(text) = &document.node(child)?.kind {
                css.push_str(&text.text);
                css.push(' ');
            }
        }
        stylesheets.push(parse_stylesheet(&css)?);
    }
    Ok(stylesheets)
}

/// The first element in `document` (in document order) matching `selector`.
///
/// Supports the same selector grammar as the cascade (type/class/id/universal/
/// compound/descendant). Unsupported selectors return a clear error from the CSS
/// parser. This reuses the cascade's matcher rather than introducing a second
/// selector engine.
pub fn query_selector(document: &Document, selector: &str) -> MochaResult<Option<NodeId>> {
    let selectors = parse_selector_list(selector)?;
    Ok(matching_elements(document, &selectors)?.into_iter().next())
}

/// All elements in `document` (in document order) matching `selector`.
pub fn query_selector_all(document: &Document, selector: &str) -> MochaResult<Vec<NodeId>> {
    let selectors = parse_selector_list(selector)?;
    matching_elements(document, &selectors)
}

fn matching_elements(document: &Document, selectors: &[Selector]) -> MochaResult<Vec<NodeId>> {
    let mut matches = Vec::new();
    collect_matches(document, document.root_id(), &[], selectors, &mut matches)?;
    Ok(matches)
}

fn collect_matches(
    document: &Document,
    id: NodeId,
    ancestors: &[ElementDescriptor],
    selectors: &[Selector],
    out: &mut Vec<NodeId>,
) -> MochaResult<()> {
    if let NodeKind::Element(data) = &document.node(id)?.kind {
        let descriptor = ElementDescriptor::from_element(data);
        if selectors
            .iter()
            .any(|selector| selector_matches(selector, &descriptor, ancestors))
        {
            out.push(id);
        }
        let mut child_ancestors = ancestors.to_vec();
        child_ancestors.push(descriptor);
        for &child in document.children(id)? {
            collect_matches(document, child, &child_ancestors, selectors, out)?;
        }
    } else {
        // The document root and non-element nodes never match and do not extend
        // the ancestor chain, but their element descendants are still visited.
        for &child in document.children(id)? {
            collect_matches(document, child, ancestors, selectors, out)?;
        }
    }
    Ok(())
}

/// Build a styled tree for `document` using the given author `stylesheets`.
pub fn build_style_tree(
    document: &Document,
    stylesheets: &[Stylesheet],
) -> MochaResult<StyledNode> {
    let root_id = document.root_id();
    let root_style = ComputedStyle::initial();
    let children = build_children(document, root_id, stylesheets, &root_style, &[])?;
    Ok(StyledNode {
        node_id: root_id,
        text: None,
        style: root_style,
        replaced: None,
        children,
    })
}

fn build_children(
    document: &Document,
    parent_id: NodeId,
    stylesheets: &[Stylesheet],
    parent_style: &ComputedStyle,
    ancestors: &[ElementDescriptor],
) -> MochaResult<Vec<StyledNode>> {
    let mut styled_children = Vec::new();
    for &child in document.children(parent_id)? {
        if let Some(node) = build_node(document, child, stylesheets, parent_style, ancestors)? {
            styled_children.push(node);
        }
    }
    Ok(styled_children)
}

fn build_node(
    document: &Document,
    id: NodeId,
    stylesheets: &[Stylesheet],
    parent_style: &ComputedStyle,
    ancestors: &[ElementDescriptor],
) -> MochaResult<Option<StyledNode>> {
    match &document.node(id)?.kind {
        NodeKind::Element(data) => {
            let descriptor = ElementDescriptor::from_element(data);
            let values = specified_values(data, &descriptor, stylesheets, ancestors)?;
            let style = ComputedStyle::from_values(&values, parent_style);

            let mut child_ancestors = ancestors.to_vec();
            child_ancestors.push(descriptor);
            let children = build_children(document, id, stylesheets, &style, &child_ancestors)?;

            Ok(Some(StyledNode {
                node_id: id,
                text: None,
                style,
                replaced: None,
                children,
            }))
        }
        NodeKind::Text(text) => Ok(Some(StyledNode {
            node_id: id,
            text: Some(text.text.clone()),
            style: ComputedStyle::for_text(parent_style),
            replaced: None,
            children: Vec::new(),
        })),
        // Comments, doctypes, and the (already-handled) document root produce no
        // styled node.
        _ => Ok(None),
    }
}

/// Run the cascade for one element and return its specified property values.
fn specified_values(
    data: &ElementData,
    descriptor: &ElementDescriptor,
    stylesheets: &[Stylesheet],
    ancestors: &[ElementDescriptor],
) -> MochaResult<HashMap<CssProperty, CssValue>> {
    let mut values: HashMap<CssProperty, CssValue> = HashMap::new();

    // 1. User-agent defaults (lowest priority).
    for (property, value) in ua_defaults(&data.tag_name) {
        values.insert(property, value);
    }

    // 2. Author rules, applied in ascending cascade order so later-applied wins.
    let mut matched: Vec<(Specificity, usize, usize, &[Declaration])> = Vec::new();
    for (sheet_index, sheet) in stylesheets.iter().enumerate() {
        for rule in &sheet.rules {
            let mut best: Option<Specificity> = None;
            for selector in &rule.selectors {
                if selector_matches(selector, descriptor, ancestors) {
                    let specificity = selector.specificity();
                    if best.is_none_or(|current| specificity > current) {
                        best = Some(specificity);
                    }
                }
            }
            if let Some(specificity) = best {
                matched.push((
                    specificity,
                    sheet_index,
                    rule.source_order,
                    &rule.declarations,
                ));
            }
        }
    }
    matched.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, _, _, declarations) in matched {
        for declaration in declarations {
            values.insert(declaration.property, declaration.value.clone());
        }
    }

    // 3. Inline style attribute (highest priority).
    if let Some(inline) = data.attribute("style") {
        for declaration in parse_inline_style(inline)? {
            values.insert(declaration.property, declaration.value);
        }
    }

    Ok(values)
}

/// The user-agent default declarations for a tag.
fn ua_defaults(tag: &str) -> Vec<(CssProperty, CssValue)> {
    let mut defaults = Vec::new();

    let display = match tag {
        "span" | "a" | "img" => "inline",
        "style" | "script" | "link" => "none",
        _ => "block",
    };
    defaults.push((CssProperty::Display, CssValue::Keyword(display.to_string())));

    // Links are blue by default (color inherits to their inline text).
    if tag == "a" {
        defaults.push((CssProperty::Color, CssValue::Color(Color::rgb(0, 0, 238))));
    }

    // Only headings carry a UA font-size; other elements inherit so author
    // `font-size` changes propagate to descendants.
    match tag {
        "h1" => defaults.push((CssProperty::FontSize, CssValue::LengthPx(32.0))),
        "h2" => defaults.push((CssProperty::FontSize, CssValue::LengthPx(24.0))),
        _ => {}
    }

    if tag == "body" {
        for property in [
            CssProperty::MarginTop,
            CssProperty::MarginRight,
            CssProperty::MarginBottom,
            CssProperty::MarginLeft,
        ] {
            defaults.push((property, CssValue::LengthPx(8.0)));
        }
    }

    defaults
}

fn edge(values: &HashMap<CssProperty, CssValue>, property: CssProperty) -> f32 {
    values.get(&property).and_then(as_length).unwrap_or(0.0)
}

fn as_color(value: &CssValue) -> Option<Color> {
    match value {
        CssValue::Color(color) => Some(*color),
        _ => None,
    }
}

fn as_length(value: &CssValue) -> Option<f32> {
    match value {
        CssValue::LengthPx(px) => Some(*px),
        _ => None,
    }
}

fn as_display(value: &CssValue) -> Option<Display> {
    match value {
        CssValue::Keyword(keyword) => match keyword.as_str() {
            "block" => Some(Display::Block),
            "inline" => Some(Display::Inline),
            "none" => Some(Display::None),
            _ => None,
        },
        _ => None,
    }
}

fn as_font_weight(value: &CssValue) -> Option<FontWeight> {
    match value {
        CssValue::Keyword(keyword) => match keyword.as_str() {
            "bold" => Some(FontWeight::Bold),
            "normal" => Some(FontWeight::Normal),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_dom::{Attribute, Document};

    /// Build a document and style tree from HTML-like construction, returning the
    /// styled node for the element with the given id attribute.
    fn find_by_id<'a>(
        node: &'a StyledNode,
        document: &Document,
        id: &str,
    ) -> Option<&'a StyledNode> {
        if let Ok(n) = document.node(node.node_id) {
            if let NodeKind::Element(data) = &n.kind {
                if data.attribute("id") == Some(id) {
                    return Some(node);
                }
            }
        }
        node.children
            .iter()
            .find_map(|child| find_by_id(child, document, id))
    }

    fn attr(name: &str, value: &str) -> Attribute {
        Attribute {
            name: name.into(),
            value: value.into(),
        }
    }

    #[test]
    fn specificity_id_beats_class_and_class_beats_type() {
        let mut document = Document::new();
        let root = document.root_id();
        let p = document.create_element("p", vec![attr("id", "intro"), attr("class", "note")]);
        document.append_child(root, p).unwrap();

        let sheets = vec![parse_stylesheet(
            "p { color: black; } .note { color: blue; } #intro { color: red; }",
        )
        .unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        let styled = find_by_id(&tree, &document, "intro").unwrap();
        assert_eq!(styled.style.color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn later_rule_wins_on_equal_specificity() {
        let mut document = Document::new();
        let root = document.root_id();
        let p = document.create_element("p", Vec::new());
        document.append_child(root, p).unwrap();

        let sheets = vec![parse_stylesheet("p { color: red; } p { color: green; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(tree.children[0].style.color, Color::rgb(0, 128, 0));
    }

    #[test]
    fn inline_style_beats_stylesheet() {
        let mut document = Document::new();
        let root = document.root_id();
        let p = document.create_element("p", vec![attr("style", "color: red;")]);
        document.append_child(root, p).unwrap();

        let sheets = vec![parse_stylesheet("p { color: blue; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(tree.children[0].style.color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn color_and_font_size_inherit_but_margin_does_not() {
        let mut document = Document::new();
        let root = document.root_id();
        let div = document.create_element("div", Vec::new());
        let p = document.create_element("p", Vec::new());
        document.append_child(root, div).unwrap();
        document.append_child(div, p).unwrap();

        // Author sets color/font-size/margin on the div only.
        let sheets =
            vec![parse_stylesheet("div { color: blue; font-size: 20px; margin: 10px; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        let div_node = &tree.children[0];
        let p_node = &div_node.children[0];

        // Inherited:
        assert_eq!(p_node.style.color, Color::rgb(0, 0, 255));
        assert_eq!(p_node.style.font_size, 20.0);
        // Not inherited:
        assert_eq!(div_node.style.margin.top, 10.0);
        assert_eq!(p_node.style.margin.top, 0.0);
    }

    #[test]
    fn font_weight_inherits() {
        let mut document = Document::new();
        let root = document.root_id();
        let div = document.create_element("div", Vec::new());
        let p = document.create_element("p", Vec::new());
        document.append_child(root, div).unwrap();
        document.append_child(div, p).unwrap();

        let sheets = vec![parse_stylesheet("div { font-weight: bold; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(
            tree.children[0].children[0].style.font_weight,
            FontWeight::Bold
        );
    }

    #[test]
    fn padding_and_border_do_not_inherit() {
        let mut document = Document::new();
        let root = document.root_id();
        let div = document.create_element("div", Vec::new());
        let p = document.create_element("p", Vec::new());
        document.append_child(root, div).unwrap();
        document.append_child(div, p).unwrap();

        let sheets = vec![parse_stylesheet("div { padding: 10px; border-width: 5px; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        let p_node = &tree.children[0].children[0];
        assert_eq!(p_node.style.padding.top, 0.0);
        assert_eq!(p_node.style.border_width, 0.0);
    }

    #[test]
    fn compound_selector_applies_in_pipeline() {
        let mut document = Document::new();
        let root = document.root_id();
        let p = document.create_element("p", vec![attr("class", "note")]);
        document.append_child(root, p).unwrap();

        // `p.note` must match (both parts true); `p.other` must not.
        let sheets =
            vec![parse_stylesheet("p.note { color: blue; } p.other { color: red; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(tree.children[0].style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn selector_list_uses_best_matching_specificity() {
        let mut document = Document::new();
        let root = document.root_id();
        let h1 = document.create_element("h1", vec![attr("class", "x")]);
        document.append_child(root, h1).unwrap();

        // Rule A matches via type (0,0,1). Rule B matches via its `.x` selector
        // (0,1,0), which is higher, so B should win even though it lists `h1` too.
        let sheets = vec![parse_stylesheet("h1 { color: red; } .x, h1 { color: blue; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(tree.children[0].style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn background_color_does_not_inherit() {
        let mut document = Document::new();
        let root = document.root_id();
        let div = document.create_element("div", Vec::new());
        let p = document.create_element("p", Vec::new());
        document.append_child(root, div).unwrap();
        document.append_child(div, p).unwrap();

        let sheets = vec![parse_stylesheet("div { background-color: blue; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(
            tree.children[0].style.background_color,
            Color::rgb(0, 0, 255)
        );
        assert_eq!(
            tree.children[0].children[0].style.background_color,
            Color::TRANSPARENT
        );
    }

    #[test]
    fn display_none_appears_in_style_tree() {
        let mut document = Document::new();
        let root = document.root_id();
        let div = document.create_element("div", Vec::new());
        document.append_child(root, div).unwrap();

        let sheets = vec![parse_stylesheet("div { display: none; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(tree.children[0].style.display, Display::None);
    }

    #[test]
    fn default_h1_font_size_is_larger_than_p() {
        let mut document = Document::new();
        let root = document.root_id();
        let h1 = document.create_element("h1", Vec::new());
        let p = document.create_element("p", Vec::new());
        document.append_child(root, h1).unwrap();
        document.append_child(root, p).unwrap();

        let tree = build_style_tree(&document, &[]).unwrap();
        assert!(tree.children[0].style.font_size > tree.children[1].style.font_size);
        assert_eq!(tree.children[0].style.font_size, 32.0);
        assert_eq!(tree.children[1].style.font_size, 16.0);
    }

    #[test]
    fn descendant_selector_applies_through_style_tree() {
        let mut document = Document::new();
        let root = document.root_id();
        let div = document.create_element("div", Vec::new());
        let p = document.create_element("p", Vec::new());
        document.append_child(root, div).unwrap();
        document.append_child(div, p).unwrap();

        let sheets = vec![parse_stylesheet("div p { color: green; }").unwrap()];
        let tree = build_style_tree(&document, &sheets).unwrap();
        assert_eq!(
            tree.children[0].children[0].style.color,
            Color::rgb(0, 128, 0)
        );
    }

    #[test]
    fn query_selector_by_type_class_id_and_descendant() {
        // div#wrap > p.intro ; plus a stray span to ensure it is skipped.
        let mut document = Document::new();
        let root = document.root_id();
        let div = document.create_element("div", vec![attr("id", "wrap")]);
        let p = document.create_element("p", vec![attr("class", "intro")]);
        let span = document.create_element("span", Vec::new());
        document.append_child(root, div).unwrap();
        document.append_child(div, p).unwrap();
        document.append_child(div, span).unwrap();

        assert_eq!(query_selector(&document, "p").unwrap(), Some(p));
        assert_eq!(query_selector(&document, ".intro").unwrap(), Some(p));
        assert_eq!(query_selector(&document, "#wrap").unwrap(), Some(div));
        assert_eq!(query_selector(&document, "div p").unwrap(), Some(p));
        assert_eq!(query_selector(&document, "p.intro").unwrap(), Some(p));
        assert_eq!(query_selector(&document, "section p").unwrap(), None);
    }

    #[test]
    fn query_selector_all_returns_document_order() {
        let mut document = Document::new();
        let root = document.root_id();
        let p1 = document.create_element("p", Vec::new());
        let div = document.create_element("div", Vec::new());
        let p2 = document.create_element("p", Vec::new());
        document.append_child(root, p1).unwrap();
        document.append_child(root, div).unwrap();
        document.append_child(div, p2).unwrap();
        assert_eq!(query_selector_all(&document, "p").unwrap(), vec![p1, p2]);
    }

    #[test]
    fn query_selector_rejects_unsupported_selector() {
        let document = Document::new();
        assert!(query_selector(&document, "p:hover").is_err());
        assert!(query_selector(&document, "div > p").is_err());
    }

    #[test]
    fn collect_stylesheets_preserves_order() {
        let mut document = Document::new();
        let root = document.root_id();
        let style_a = document.create_element("style", Vec::new());
        let text_a = document.create_text("p { color: red; }");
        let style_b = document.create_element("style", Vec::new());
        let text_b = document.create_text("p { color: blue; }");
        document.append_child(root, style_a).unwrap();
        document.append_child(style_a, text_a).unwrap();
        document.append_child(root, style_b).unwrap();
        document.append_child(style_b, text_b).unwrap();

        let sheets = collect_stylesheets(&document).unwrap();
        assert_eq!(sheets.len(), 2);
        // Second sheet (later) should win the cascade for a bare `p`.
        let p = document.create_element("p", Vec::new());
        document.append_child(root, p).unwrap();
        let tree = build_style_tree(&document, &sheets).unwrap();
        let styled_p = tree.children.iter().find(|c| c.node_id == p).unwrap();
        assert_eq!(styled_p.style.color, Color::rgb(0, 0, 255));
    }
}
