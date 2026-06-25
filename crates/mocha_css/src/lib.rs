//! Minimal CSS tokenizer, parser, and value model for Mocha Browser.
//!
//! **This is not a CSS-spec-compliant implementation**, but it covers the
//! selector grammar real pages rely on: type/class/id/universal selectors, the
//! descendant/child/next-sibling/subsequent-sibling combinators, attribute
//! selectors (`[a=v]`, `[a~=v]`, `[a^=v]`, …), and structural pseudo-classes
//! (`:first-child`, `:nth-child(an+b)`, `:not()`, `:root`, …). Dynamic
//! pseudo-classes (`:hover`) and pseudo-elements (`::before`) parse but never
//! match in a static render. It also supports a fixed property set, common
//! length units, and named/hex/`rgb()`/`hsl()` colors. This crate has **no DOM
//! access** and performs **no selector matching against a DOM** — that is
//! `mocha_style`'s job.
//!
//! [`MochaError`]: mocha_error::MochaError

mod parser;
pub mod tokenizer;

pub use parser::{parse_inline_style, parse_selector_list, parse_stylesheet};
pub use tokenizer::{tokenize, CssToken};

use std::fmt;

use mocha_error::{MochaError, MochaResult};

/// An sRGB color with 8-bit channels and alpha. `a == 0` means fully transparent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (255 = opaque, 0 = transparent).
    pub a: u8,
}

impl Color {
    /// An opaque color from RGB channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Opaque black, the initial value of `color`.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.a == 0 {
            write!(f, "transparent")
        } else {
            write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        }
    }
}

/// Look up one of the small set of named colors Mocha supports.
pub fn named_color(name: &str) -> Option<Color> {
    match name {
        "black" => Some(Color::rgb(0, 0, 0)),
        "white" => Some(Color::rgb(255, 255, 255)),
        "red" => Some(Color::rgb(255, 0, 0)),
        "green" => Some(Color::rgb(0, 128, 0)),
        "blue" => Some(Color::rgb(0, 0, 255)),
        "transparent" => Some(Color::TRANSPARENT),
        _ => None,
    }
}

/// A parsed CSS property value.
#[derive(Debug, Clone, PartialEq)]
pub enum CssValue {
    /// A bare keyword such as `block` or `bold`.
    Keyword(String),
    /// A length in pixels.
    LengthPx(f32),
    /// A length in `em` (relative to the element's font size).
    Em(f32),
    /// A length in `rem` (relative to the root font size).
    Rem(f32),
    /// A percentage (resolved against the containing block during layout).
    Percent(f32),
    /// A unitless number (e.g. a `line-height` multiplier).
    Number(f32),
    /// A color value.
    Color(Color),
}

/// The set of CSS properties Mocha understands. Shorthands (`margin`, `padding`)
/// are expanded into these longhands during parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CssProperty {
    /// `display`
    Display,
    /// `color`
    Color,
    /// `background-color`
    BackgroundColor,
    /// `font-size`
    FontSize,
    /// `font-weight`
    FontWeight,
    /// `width`
    Width,
    /// `height`
    Height,
    /// `margin-top`
    MarginTop,
    /// `margin-right`
    MarginRight,
    /// `margin-bottom`
    MarginBottom,
    /// `margin-left`
    MarginLeft,
    /// `padding-top`
    PaddingTop,
    /// `padding-right`
    PaddingRight,
    /// `padding-bottom`
    PaddingBottom,
    /// `padding-left`
    PaddingLeft,
    /// `border-width`
    BorderWidth,
    /// `border-color`
    BorderColor,
    /// `text-align`
    TextAlign,
    /// `line-height`
    LineHeight,
    /// `max-width`
    MaxWidth,
    /// `flex-direction`
    FlexDirection,
    /// `justify-content`
    JustifyContent,
    /// `align-items`
    AlignItems,
    /// `gap` (row/column gap, resolved to a single value)
    Gap,
    /// `flex-grow`
    FlexGrow,
    /// `border-radius`
    BorderRadius,
}

/// A single `property: value` pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// The property being set.
    pub property: CssProperty,
    /// The parsed value.
    pub value: CssValue,
    /// Whether the source declaration carried `!important`. Important
    /// declarations win over all normal ones in the cascade, regardless of
    /// selector specificity.
    pub important: bool,
}

/// A simple selector: the smallest matching unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleSelector {
    /// `*`
    Universal,
    /// A type/tag selector such as `div`.
    Type(String),
    /// A class selector such as `.note` (stored without the dot).
    Class(String),
    /// An id selector such as `#hero` (stored without the hash).
    Id(String),
    /// An attribute selector such as `[type="text"]` or `[disabled]`.
    Attribute(AttributeSelector),
    /// A pseudo-class such as `:first-child` or `:nth-child(2n+1)`.
    PseudoClass(PseudoClass),
}

/// An attribute selector: a name plus a matching rule against its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeSelector {
    /// The attribute name (lowercased, e.g. `type`).
    pub name: String,
    /// How the attribute's value must match.
    pub matcher: AttributeMatch,
}

/// The matching rule of an [`AttributeSelector`]. Mirrors the CSS attribute
/// matchers; the contained string is the right-hand side (already unquoted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeMatch {
    /// `[attr]` — the attribute is present, with any value.
    Exists,
    /// `[attr=value]` — exact match.
    Equals(String),
    /// `[attr~=value]` — `value` is one of the space-separated words.
    Includes(String),
    /// `[attr|=value]` — equal to `value` or starts with `value-`.
    DashMatch(String),
    /// `[attr^=value]` — the value starts with `value`.
    Prefix(String),
    /// `[attr$=value]` — the value ends with `value`.
    Suffix(String),
    /// `[attr*=value]` — the value contains `value`.
    Substring(String),
}

/// A pseudo-class. Structural pseudo-classes match deterministically against the
/// DOM tree; dynamic ones (`:hover`, `:focus`, …) and pseudo-elements
/// (`::before`, …) parse to [`PseudoClass::Inert`] and never match — their rules
/// are *retained* rather than dropped, but their state is never faked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoClass {
    /// `:root` — the document element.
    Root,
    /// `:empty` — no element or (non-whitespace) text children.
    Empty,
    /// `:first-child`
    FirstChild,
    /// `:last-child`
    LastChild,
    /// `:only-child`
    OnlyChild,
    /// `:first-of-type`
    FirstOfType,
    /// `:last-of-type`
    LastOfType,
    /// `:only-of-type`
    OnlyOfType,
    /// `:nth-child(an+b)`
    NthChild(Nth),
    /// `:nth-last-child(an+b)`
    NthLastChild(Nth),
    /// `:nth-of-type(an+b)`
    NthOfType(Nth),
    /// `:nth-last-of-type(an+b)`
    NthLastOfType(Nth),
    /// `:not(<compound>)` — negation of a single compound selector.
    Not(Vec<SimpleSelector>),
    /// A dynamic pseudo-class or pseudo-element that is parsed but never matches
    /// in a static render (e.g. `hover`, `focus`, `::before`). The string is the
    /// pseudo's name, kept only for diagnostics.
    Inert(String),
}

/// The `an+b` coefficients of an `:nth-*` pseudo-class. A 1-based child index
/// `i` matches when there exists an integer `n >= 0` with `i == a*n + b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nth {
    /// The step (`a` in `an+b`).
    pub a: i32,
    /// The offset (`b` in `an+b`).
    pub b: i32,
}

impl Nth {
    /// Does a 1-based index match this `an+b`?
    pub fn matches(&self, index: i32) -> bool {
        if self.a == 0 {
            index == self.b
        } else {
            let diff = index - self.b;
            diff % self.a == 0 && diff / self.a >= 0
        }
    }
}

/// A combinator joining two compound selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// ` ` (whitespace) — the left compound matches some ancestor.
    Descendant,
    /// `>` — the left compound matches the immediate parent.
    Child,
    /// `+` — the left compound matches the immediately preceding element sibling.
    NextSibling,
    /// `~` — the left compound matches some preceding element sibling.
    SubsequentSibling,
}

/// A compound selector: simple selectors with no combinator between them, e.g.
/// `div.note#x`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundSelector {
    /// The simple selectors that must all match the same element.
    pub simple_selectors: Vec<SimpleSelector>,
}

/// A full (complex) selector. `parts` is ordered ancestor → target; `combinators`
/// has one entry per gap, so `combinators[i]` joins `parts[i]` (on the left) to
/// `parts[i + 1]`. A lone compound has an empty `combinators`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Compound selectors from the outermost ancestor to the target element.
    pub parts: Vec<CompoundSelector>,
    /// The combinators between consecutive parts (`parts.len() - 1` entries).
    pub combinators: Vec<Combinator>,
}

impl Selector {
    /// Build a descendant-combinator chain (the common case in tests/helpers).
    pub fn descendant_chain(parts: Vec<CompoundSelector>) -> Selector {
        let combinators = vec![Combinator::Descendant; parts.len().saturating_sub(1)];
        Selector { parts, combinators }
    }

    /// Compute this selector's specificity as (#id, #class, #type).
    pub fn specificity(&self) -> Specificity {
        let mut spec = Specificity {
            ids: 0,
            classes: 0,
            elements: 0,
        };
        for part in &self.parts {
            for simple in &part.simple_selectors {
                add_simple_specificity(simple, &mut spec);
            }
        }
        spec
    }
}

/// Accumulate one simple selector's contribution to a specificity tuple.
fn add_simple_specificity(simple: &SimpleSelector, spec: &mut Specificity) {
    match simple {
        SimpleSelector::Id(_) => spec.ids += 1,
        // Class, attribute, and (structural) pseudo-class selectors all count in
        // the "class" column.
        SimpleSelector::Class(_) | SimpleSelector::Attribute(_) => spec.classes += 1,
        SimpleSelector::PseudoClass(pseudo) => match pseudo {
            // `:not()` takes the specificity of its argument; pseudo-elements
            // (modelled as `Inert`) contribute nothing extra beyond a class-level
            // weight, matching how authors reason about them here.
            PseudoClass::Not(inner) => {
                for simple in inner {
                    add_simple_specificity(simple, spec);
                }
            }
            _ => spec.classes += 1,
        },
        SimpleSelector::Type(_) => spec.elements += 1,
        SimpleSelector::Universal => {}
    }
}

/// Selector specificity. Ordering compares ids, then classes, then elements,
/// which matches the CSS cascade's specificity precedence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    /// Number of id selectors.
    pub ids: u32,
    /// Number of class selectors.
    pub classes: u32,
    /// Number of type selectors.
    pub elements: u32,
}

/// A single style rule: one or more selectors and a block of declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRule {
    /// The selector list this rule applies to.
    pub selectors: Vec<Selector>,
    /// The declarations to apply when a selector matches.
    pub declarations: Vec<Declaration>,
    /// The rule's position within its stylesheet (0-based), used for the cascade
    /// tie-break "later rule wins".
    pub source_order: usize,
}

/// A parsed stylesheet: an ordered list of rules.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    /// The rules in source order.
    pub rules: Vec<StyleRule>,
    /// Human-readable notes about selectors, declarations, and at-rules that the
    /// forgiving parser skipped (surfaced as render diagnostics so unsupported
    /// features are reported, not silently faked).
    pub skipped: Vec<String>,
}

/// Parse a single color token's textual form (`#rgb`, `#rrggbb`, or a named
/// color). Shared by the parser; exposed for reuse by `mocha_style`'s tests.
pub fn parse_hex_color(hex: &str) -> MochaResult<Color> {
    let expanded = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => {
            return Err(MochaError::Parse(format!(
                "invalid hex color '#{hex}': expected 3 or 6 digits"
            )))
        }
    };
    let parse_channel = |slice: &str| {
        u8::from_str_radix(slice, 16)
            .map_err(|_| MochaError::Parse(format!("invalid hex color '#{hex}'")))
    };
    Ok(Color {
        r: parse_channel(&expanded[0..2])?,
        g: parse_channel(&expanded[2..4])?,
        b: parse_channel(&expanded[4..6])?,
        a: 255,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_displays_as_hex_or_transparent() {
        assert_eq!(Color::rgb(0x22, 0x22, 0x22).to_string(), "#222222");
        assert_eq!(Color::TRANSPARENT.to_string(), "transparent");
    }

    #[test]
    fn named_colors_resolve() {
        assert_eq!(named_color("red"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(named_color("green"), Some(Color::rgb(0, 128, 0)));
        assert_eq!(named_color("nope"), None);
    }

    #[test]
    fn specificity_orders_id_over_class_over_type() {
        let id = Selector::descendant_chain(vec![CompoundSelector {
            simple_selectors: vec![SimpleSelector::Id("a".into())],
        }]);
        let class = Selector::descendant_chain(vec![CompoundSelector {
            simple_selectors: vec![SimpleSelector::Class("a".into())],
        }]);
        let ty = Selector::descendant_chain(vec![CompoundSelector {
            simple_selectors: vec![SimpleSelector::Type("a".into())],
        }]);
        assert!(id.specificity() > class.specificity());
        assert!(class.specificity() > ty.specificity());
    }

    #[test]
    fn short_hex_expands() {
        assert_eq!(
            parse_hex_color("abc").unwrap(),
            Color::rgb(0xaa, 0xbb, 0xcc)
        );
    }
}
