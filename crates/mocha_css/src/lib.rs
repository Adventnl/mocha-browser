//! Minimal CSS tokenizer, parser, and value model for Mocha Browser.
//!
//! **This is not a CSS-spec-compliant implementation.** It supports a small,
//! explicitly documented subset: type/class/id/universal/descendant selectors,
//! a fixed property set, `px` lengths, named and hex colors, and a few keywords.
//! Unknown properties, unsupported units, and unsupported syntax (`!important`,
//! combinators, pseudo-classes, …) return a clear [`MochaError`] rather than
//! being silently dropped. This crate has **no DOM access** and performs **no
//! selector matching against a DOM** — that is `mocha_style`'s job.
//!
//! [`MochaError`]: mocha_error::MochaError

mod parser;
pub mod tokenizer;

pub use parser::{parse_inline_style, parse_stylesheet};
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
    /// A length in pixels (the only supported unit).
    LengthPx(f32),
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
}

/// A single `property: value` pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// The property being set.
    pub property: CssProperty,
    /// The parsed value.
    pub value: CssValue,
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
}

/// A compound selector: simple selectors with no combinator between them, e.g.
/// `div.note#x`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundSelector {
    /// The simple selectors that must all match the same element.
    pub simple_selectors: Vec<SimpleSelector>,
}

/// A full selector. `parts` is ordered ancestor → descendant; multiple parts
/// represent a descendant combinator (`div p span`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Compound selectors from the outermost ancestor to the target element.
    pub parts: Vec<CompoundSelector>,
}

impl Selector {
    /// Compute this selector's specificity as (#id, #class, #type).
    pub fn specificity(&self) -> Specificity {
        let mut spec = Specificity {
            ids: 0,
            classes: 0,
            elements: 0,
        };
        for part in &self.parts {
            for simple in &part.simple_selectors {
                match simple {
                    SimpleSelector::Id(_) => spec.ids += 1,
                    SimpleSelector::Class(_) => spec.classes += 1,
                    SimpleSelector::Type(_) => spec.elements += 1,
                    SimpleSelector::Universal => {}
                }
            }
        }
        spec
    }
}

/// Selector specificity. Ordering compares ids, then classes, then elements,
/// which matches the CSS cascade's specificity precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
        let id = Selector {
            parts: vec![CompoundSelector {
                simple_selectors: vec![SimpleSelector::Id("a".into())],
            }],
        };
        let class = Selector {
            parts: vec![CompoundSelector {
                simple_selectors: vec![SimpleSelector::Class("a".into())],
            }],
        };
        let ty = Selector {
            parts: vec![CompoundSelector {
                simple_selectors: vec![SimpleSelector::Type("a".into())],
            }],
        };
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
