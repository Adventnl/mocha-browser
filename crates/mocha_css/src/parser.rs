//! A small recursive CSS parser built on [`crate::tokenizer`].
//!
//! It parses the supported selector grammar and declaration grammar, expanding
//! `margin`/`padding` shorthands into longhands. Unknown properties and
//! unsupported values/syntax are reported as errors, never dropped.

use mocha_error::{MochaError, MochaResult};

use crate::tokenizer::{tokenize, CssToken};
use crate::{
    named_color, parse_hex_color, CompoundSelector, CssProperty, CssValue, Declaration, Selector,
    SimpleSelector, StyleRule, Stylesheet,
};

/// Parse a complete stylesheet (`selector { decls } …`).
pub fn parse_stylesheet(input: &str) -> MochaResult<Stylesheet> {
    let mut parser = Parser::new(tokenize(input)?);
    let mut rules = Vec::new();
    let mut source_order = 0;
    loop {
        parser.skip_whitespace();
        if parser.at_end() {
            break;
        }
        rules.push(parser.parse_rule(source_order)?);
        source_order += 1;
    }
    Ok(Stylesheet { rules })
}

/// Parse the body of a `style="…"` attribute into declarations.
pub fn parse_inline_style(input: &str) -> MochaResult<Vec<Declaration>> {
    let mut parser = Parser::new(tokenize(input)?);
    let mut declarations = Vec::new();
    loop {
        parser.skip_whitespace();
        if parser.at_end() {
            break;
        }
        declarations.extend(parser.parse_declaration()?);
    }
    Ok(declarations)
}

struct Parser {
    tokens: Vec<CssToken>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<CssToken>) -> Parser {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&CssToken> {
        self.tokens.get(self.pos)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(CssToken::Whitespace)) {
            self.pos += 1;
        }
    }

    fn parse_rule(&mut self, source_order: usize) -> MochaResult<StyleRule> {
        let selectors = self.parse_selector_list()?;
        match self.peek() {
            Some(CssToken::LeftBrace) => self.pos += 1,
            other => {
                return Err(MochaError::Parse(format!(
                    "expected '{{' after selector, found {other:?}"
                )))
            }
        }
        let declarations = self.parse_declaration_block()?;
        Ok(StyleRule {
            selectors,
            declarations,
            source_order,
        })
    }

    fn parse_selector_list(&mut self) -> MochaResult<Vec<Selector>> {
        let mut selectors = vec![self.parse_selector()?];
        loop {
            self.skip_whitespace();
            if matches!(self.peek(), Some(CssToken::Comma)) {
                self.pos += 1;
                selectors.push(self.parse_selector()?);
            } else {
                break;
            }
        }
        Ok(selectors)
    }

    fn parse_selector(&mut self) -> MochaResult<Selector> {
        let mut parts = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                None | Some(CssToken::Comma) | Some(CssToken::LeftBrace) => break,
                _ => parts.push(self.parse_compound_selector()?),
            }
        }
        if parts.is_empty() {
            return Err(MochaError::Parse("expected a selector".to_string()));
        }
        Ok(Selector { parts })
    }

    fn parse_compound_selector(&mut self) -> MochaResult<CompoundSelector> {
        let mut simple_selectors = Vec::new();
        loop {
            match self.peek() {
                Some(CssToken::Star) => {
                    simple_selectors.push(SimpleSelector::Universal);
                    self.pos += 1;
                }
                Some(CssToken::Ident(name)) => {
                    simple_selectors.push(SimpleSelector::Type(name.to_ascii_lowercase()));
                    self.pos += 1;
                }
                Some(CssToken::Hash(name)) => {
                    simple_selectors.push(SimpleSelector::Id(name.clone()));
                    self.pos += 1;
                }
                Some(CssToken::Dot) => {
                    self.pos += 1;
                    match self.peek() {
                        Some(CssToken::Ident(name)) => {
                            simple_selectors.push(SimpleSelector::Class(name.clone()));
                            self.pos += 1;
                        }
                        other => {
                            return Err(MochaError::Parse(format!(
                                "expected class name after '.', found {other:?}"
                            )))
                        }
                    }
                }
                Some(CssToken::Colon) => {
                    return Err(MochaError::UnsupportedFeature(
                        "pseudo-classes and pseudo-elements are not supported in Milestone 2"
                            .to_string(),
                    ))
                }
                Some(CssToken::Delim(c @ ('>' | '+' | '~'))) => {
                    return Err(MochaError::UnsupportedFeature(format!(
                        "selector combinator '{c}' is not supported in Milestone 2 (only descendant)"
                    )))
                }
                Some(CssToken::Delim('[')) => {
                    return Err(MochaError::UnsupportedFeature(
                        "attribute selectors are not supported in Milestone 2".to_string(),
                    ))
                }
                _ => break,
            }
        }
        if simple_selectors.is_empty() {
            return Err(MochaError::Parse("expected a simple selector".to_string()));
        }
        Ok(CompoundSelector { simple_selectors })
    }

    fn parse_declaration_block(&mut self) -> MochaResult<Vec<Declaration>> {
        let mut declarations = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(CssToken::RightBrace) => {
                    self.pos += 1;
                    break;
                }
                None => {
                    return Err(MochaError::Parse(
                        "unterminated declaration block: missing '}'".to_string(),
                    ))
                }
                _ => declarations.extend(self.parse_declaration()?),
            }
        }
        Ok(declarations)
    }

    /// Parse one `property: value;` and return the (possibly multiple, for
    /// shorthands) declarations it expands to.
    fn parse_declaration(&mut self) -> MochaResult<Vec<Declaration>> {
        self.skip_whitespace();
        let property_name = match self.peek() {
            Some(CssToken::Ident(name)) => {
                let name = name.to_ascii_lowercase();
                self.pos += 1;
                name
            }
            other => {
                return Err(MochaError::Parse(format!(
                    "expected a property name, found {other:?}"
                )))
            }
        };

        self.skip_whitespace();
        match self.peek() {
            Some(CssToken::Colon) => self.pos += 1,
            other => {
                return Err(MochaError::Parse(format!(
                    "expected ':' after property '{property_name}', found {other:?}"
                )))
            }
        }

        let value_tokens = self.collect_value_tokens();
        if value_tokens
            .iter()
            .any(|token| matches!(token, CssToken::Delim('!')))
        {
            return Err(MochaError::UnsupportedFeature(
                "!important is not supported in Milestone 2".to_string(),
            ));
        }
        if value_tokens.is_empty() {
            return Err(MochaError::Parse(format!(
                "property '{property_name}' has no value"
            )));
        }
        build_declarations(&property_name, &value_tokens)
    }

    /// Collect the non-whitespace value tokens up to the terminating `;`, `}`,
    /// or end of input. The `;` is consumed; the `}` is left for the block loop.
    fn collect_value_tokens(&mut self) -> Vec<CssToken> {
        let mut tokens = Vec::new();
        loop {
            match self.peek() {
                Some(CssToken::Semicolon) => {
                    self.pos += 1;
                    break;
                }
                Some(CssToken::RightBrace) | None => break,
                Some(CssToken::Whitespace) => self.pos += 1,
                Some(token) => {
                    tokens.push(token.clone());
                    self.pos += 1;
                }
            }
        }
        tokens
    }
}

/// Map a property name plus its value tokens to one or more declarations.
fn build_declarations(name: &str, tokens: &[CssToken]) -> MochaResult<Vec<Declaration>> {
    let single = |property: CssProperty, value: CssValue| vec![Declaration { property, value }];

    match name {
        "display" => Ok(single(
            CssProperty::Display,
            keyword(tokens, &["block", "inline", "none"])?,
        )),
        "font-weight" => Ok(single(
            CssProperty::FontWeight,
            keyword(tokens, &["normal", "bold"])?,
        )),
        "color" => Ok(single(CssProperty::Color, color(tokens)?)),
        "background-color" => Ok(single(CssProperty::BackgroundColor, color(tokens)?)),
        "border-color" => Ok(single(CssProperty::BorderColor, color(tokens)?)),
        "font-size" => Ok(single(CssProperty::FontSize, length(tokens)?)),
        "width" => Ok(single(CssProperty::Width, length(tokens)?)),
        "height" => Ok(single(CssProperty::Height, length(tokens)?)),
        "border-width" => Ok(single(CssProperty::BorderWidth, length(tokens)?)),
        "margin-top" => Ok(single(CssProperty::MarginTop, length(tokens)?)),
        "margin-right" => Ok(single(CssProperty::MarginRight, length(tokens)?)),
        "margin-bottom" => Ok(single(CssProperty::MarginBottom, length(tokens)?)),
        "margin-left" => Ok(single(CssProperty::MarginLeft, length(tokens)?)),
        "padding-top" => Ok(single(CssProperty::PaddingTop, length(tokens)?)),
        "padding-right" => Ok(single(CssProperty::PaddingRight, length(tokens)?)),
        "padding-bottom" => Ok(single(CssProperty::PaddingBottom, length(tokens)?)),
        "padding-left" => Ok(single(CssProperty::PaddingLeft, length(tokens)?)),
        "margin" => expand_box_shorthand(
            tokens,
            [
                CssProperty::MarginTop,
                CssProperty::MarginRight,
                CssProperty::MarginBottom,
                CssProperty::MarginLeft,
            ],
        ),
        "padding" => expand_box_shorthand(
            tokens,
            [
                CssProperty::PaddingTop,
                CssProperty::PaddingRight,
                CssProperty::PaddingBottom,
                CssProperty::PaddingLeft,
            ],
        ),
        other => Err(MochaError::UnsupportedFeature(format!(
            "CSS property '{other}' is not supported in Milestone 2"
        ))),
    }
}

/// Expand a 1–4 value box shorthand (margin/padding) into four longhands.
fn expand_box_shorthand(
    tokens: &[CssToken],
    [top, right, bottom, left]: [CssProperty; 4],
) -> MochaResult<Vec<Declaration>> {
    let mut values = Vec::new();
    for token in tokens {
        values.push(length_value(token)?);
    }
    let (t, r, b, l) = match values.as_slice() {
        [all] => (*all, *all, *all, *all),
        [v, h] => (*v, *h, *v, *h),
        [t, h, b] => (*t, *h, *b, *h),
        [t, r, b, l] => (*t, *r, *b, *l),
        _ => {
            return Err(MochaError::Parse(
                "box shorthand expects between 1 and 4 length values".to_string(),
            ))
        }
    };
    Ok(vec![
        Declaration {
            property: top,
            value: CssValue::LengthPx(t),
        },
        Declaration {
            property: right,
            value: CssValue::LengthPx(r),
        },
        Declaration {
            property: bottom,
            value: CssValue::LengthPx(b),
        },
        Declaration {
            property: left,
            value: CssValue::LengthPx(l),
        },
    ])
}

fn keyword(tokens: &[CssToken], allowed: &[&str]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Ident(name)] => {
            let lower = name.to_ascii_lowercase();
            if allowed.contains(&lower.as_str()) {
                Ok(CssValue::Keyword(lower))
            } else {
                Err(MochaError::UnsupportedFeature(format!(
                    "unsupported keyword value '{name}' (expected one of {allowed:?})"
                )))
            }
        }
        _ => Err(MochaError::Parse(format!(
            "expected a single keyword, found {tokens:?}"
        ))),
    }
}

fn length(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [token] => Ok(CssValue::LengthPx(length_value(token)?)),
        _ => Err(MochaError::Parse(format!(
            "expected a single length, found {tokens:?}"
        ))),
    }
}

/// Convert a single token to a pixel length, rejecting non-`px` units.
fn length_value(token: &CssToken) -> MochaResult<f32> {
    match token {
        CssToken::Dimension(value, unit) if unit == "px" => Ok(*value),
        CssToken::Dimension(_, unit) => Err(MochaError::UnsupportedFeature(format!(
            "CSS unit '{unit}' is not supported in Milestone 2 (use px)"
        ))),
        // Unitless zero is allowed; any other bare number requires a unit.
        CssToken::Number(value) if *value == 0.0 => Ok(0.0),
        CssToken::Number(_) => Err(MochaError::Parse(
            "lengths require a 'px' unit (except 0)".to_string(),
        )),
        other => Err(MochaError::Parse(format!(
            "expected a length, found {other:?}"
        ))),
    }
}

fn color(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Hash(hex)] => Ok(CssValue::Color(parse_hex_color(hex)?)),
        [CssToken::Ident(name)] => {
            let lower = name.to_ascii_lowercase();
            named_color(&lower).map(CssValue::Color).ok_or_else(|| {
                MochaError::UnsupportedFeature(format!(
                    "color '{name}' is not supported in Milestone 2"
                ))
            })
        }
        // rgb()/rgba()/hsl() would tokenize as an ident followed by other tokens.
        _ => Err(MochaError::UnsupportedFeature(format!(
            "unsupported color value {tokens:?} (use a named color or hex)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;

    fn only_rule(input: &str) -> StyleRule {
        let sheet = parse_stylesheet(input).unwrap();
        assert_eq!(sheet.rules.len(), 1);
        sheet.rules.into_iter().next().unwrap()
    }

    #[test]
    fn parse_type_selector() {
        let rule = only_rule("h1 { color: red; }");
        assert_eq!(
            rule.selectors[0].parts[0].simple_selectors,
            vec![SimpleSelector::Type("h1".into())]
        );
    }

    #[test]
    fn parse_class_selector() {
        let rule = only_rule(".note { color: blue; }");
        assert_eq!(
            rule.selectors[0].parts[0].simple_selectors,
            vec![SimpleSelector::Class("note".into())]
        );
    }

    #[test]
    fn parse_id_selector() {
        let rule = only_rule("#hero { color: red; }");
        assert_eq!(
            rule.selectors[0].parts[0].simple_selectors,
            vec![SimpleSelector::Id("hero".into())]
        );
    }

    #[test]
    fn parse_universal_selector() {
        let rule = only_rule("* { color: red; }");
        assert_eq!(
            rule.selectors[0].parts[0].simple_selectors,
            vec![SimpleSelector::Universal]
        );
    }

    #[test]
    fn parse_descendant_selector() {
        let rule = only_rule("div p { color: green; }");
        assert_eq!(rule.selectors[0].parts.len(), 2);
        assert_eq!(
            rule.selectors[0].parts[0].simple_selectors,
            vec![SimpleSelector::Type("div".into())]
        );
        assert_eq!(
            rule.selectors[0].parts[1].simple_selectors,
            vec![SimpleSelector::Type("p".into())]
        );
    }

    #[test]
    fn parse_compound_selector() {
        let rule = only_rule("div.note#x { color: red; }");
        assert_eq!(
            rule.selectors[0].parts[0].simple_selectors,
            vec![
                SimpleSelector::Type("div".into()),
                SimpleSelector::Class("note".into()),
                SimpleSelector::Id("x".into()),
            ]
        );
    }

    #[test]
    fn parse_selector_list() {
        let rule = only_rule("h1, h2 { color: red; }");
        assert_eq!(rule.selectors.len(), 2);
    }

    #[test]
    fn parse_declarations() {
        let rule = only_rule("p { color: red; font-size: 16px; }");
        assert_eq!(rule.declarations.len(), 2);
        assert_eq!(rule.declarations[0].property, CssProperty::Color);
        assert_eq!(rule.declarations[1].property, CssProperty::FontSize);
    }

    #[test]
    fn parse_inline_style_declarations() {
        let decls = parse_inline_style("color: red; font-size: 20px").unwrap();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].value, CssValue::Color(Color::rgb(255, 0, 0)));
        assert_eq!(decls[1].value, CssValue::LengthPx(20.0));
    }

    #[test]
    fn parse_px_length() {
        let rule = only_rule("p { font-size: 32px; }");
        assert_eq!(rule.declarations[0].value, CssValue::LengthPx(32.0));
    }

    #[test]
    fn parse_named_color() {
        let rule = only_rule("p { color: blue; }");
        assert_eq!(
            rule.declarations[0].value,
            CssValue::Color(Color::rgb(0, 0, 255))
        );
    }

    #[test]
    fn parse_short_and_long_hex_color() {
        let short = only_rule("p { color: #abc; }");
        assert_eq!(
            short.declarations[0].value,
            CssValue::Color(Color::rgb(0xaa, 0xbb, 0xcc))
        );
        let long = only_rule("p { color: #112233; }");
        assert_eq!(
            long.declarations[0].value,
            CssValue::Color(Color::rgb(0x11, 0x22, 0x33))
        );
    }

    #[test]
    fn parse_margin_shorthand_expands() {
        let rule = only_rule("div { margin: 4px 8px; }");
        let props: Vec<_> = rule.declarations.iter().map(|d| d.property).collect();
        assert_eq!(
            props,
            vec![
                CssProperty::MarginTop,
                CssProperty::MarginRight,
                CssProperty::MarginBottom,
                CssProperty::MarginLeft,
            ]
        );
        assert_eq!(rule.declarations[0].value, CssValue::LengthPx(4.0));
        assert_eq!(rule.declarations[1].value, CssValue::LengthPx(8.0));
    }

    #[test]
    fn parse_padding_shorthand_single_value() {
        let rule = only_rule("div { padding: 12px; }");
        assert_eq!(rule.declarations.len(), 4);
        assert!(rule
            .declarations
            .iter()
            .all(|d| d.value == CssValue::LengthPx(12.0)));
    }

    #[test]
    fn reject_unknown_property() {
        let error = parse_stylesheet("p { float: left; }").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn reject_unsupported_unit() {
        let error = parse_stylesheet("p { font-size: 2em; }").unwrap_err();
        match error {
            MochaError::UnsupportedFeature(message) => assert!(message.contains("em")),
            other => panic!("expected UnsupportedFeature, got {other:?}"),
        }
    }

    #[test]
    fn reject_malformed_declaration() {
        let error = parse_stylesheet("p { color }").unwrap_err();
        assert!(matches!(error, MochaError::Parse(_)));
    }

    #[test]
    fn reject_important() {
        let error = parse_stylesheet("p { color: red !important; }").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn reject_child_combinator() {
        let error = parse_stylesheet("div > p { color: red; }").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn comments_are_ignored_by_parser() {
        let rule = only_rule("p { /* hi */ color: red; }");
        assert_eq!(rule.declarations.len(), 1);
    }

    #[test]
    fn empty_stylesheet_is_allowed() {
        assert!(parse_stylesheet("   ").unwrap().rules.is_empty());
        assert!(parse_stylesheet("/* only a comment */")
            .unwrap()
            .rules
            .is_empty());
    }

    #[test]
    fn source_order_is_assigned() {
        let sheet = parse_stylesheet("a { color: red; } b { color: blue; }").unwrap();
        assert_eq!(sheet.rules[0].source_order, 0);
        assert_eq!(sheet.rules[1].source_order, 1);
    }
}
