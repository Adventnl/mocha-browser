//! A small recursive CSS parser built on [`crate::tokenizer`].
//!
//! It parses the supported selector grammar and declaration grammar, expanding
//! `margin`/`padding` shorthands into longhands. Parsing is **forgiving**
//! (CSS's own error-recovery model): unknown properties, unsupported values,
//! at-rules, and selectors are skipped — recorded in [`Stylesheet::skipped`] so
//! they surface as render diagnostics — while the rest of the sheet still
//! applies. The standalone [`parse_selector_list`] (the `querySelector` grammar)
//! stays strict and still errors on unsupported selectors.

use mocha_error::{MochaError, MochaResult};

use crate::tokenizer::{tokenize, CssToken};
use crate::{
    named_color, parse_hex_color, AttributeMatch, AttributeSelector, Combinator, CompoundSelector,
    CssProperty, CssValue, Declaration, Nth, PseudoClass, Selector, SimpleSelector, StyleRule,
    Stylesheet,
};

/// Parse a complete stylesheet (`selector { decls } …`).
///
/// Parsing is **forgiving**, following CSS's own error-recovery rules: an
/// unsupported selector, declaration, or at-rule (`@media`, `@font-face`, …) is
/// skipped and the rest of the sheet still parses, so real-world stylesheets
/// render with the subset Mocha understands instead of failing wholesale. The
/// only `Err` is a catastrophic tokenizer failure.
pub fn parse_stylesheet(input: &str) -> MochaResult<Stylesheet> {
    let mut parser = Parser::new(tokenize(input)?);
    let mut rules = Vec::new();
    let mut source_order = 0;
    loop {
        parser.skip_whitespace();
        if parser.at_end() {
            break;
        }
        let before = parser.pos;
        // At-rules (@media/@import/@font-face/@keyframes/@supports/…): skip the
        // whole rule (statement or block) and continue.
        if matches!(parser.peek(), Some(CssToken::Delim('@'))) {
            parser.skip_at_rule();
            parser.skipped.push("CSS at-rule skipped".to_string());
        } else if let Some(rule) = parser.parse_rule_forgiving(source_order) {
            if !rule.selectors.is_empty() && !rule.declarations.is_empty() {
                rules.push(rule);
                source_order += 1;
            }
        }
        // Guarantee forward progress so a pathological input cannot loop forever.
        if parser.pos == before {
            parser.pos += 1;
        }
    }
    let skipped = parser.skipped;
    Ok(Stylesheet { rules, skipped })
}

/// Parse a standalone selector list such as `p.intro, div span` — the grammar
/// behind `querySelector`/`querySelectorAll`. The whole input must be a selector
/// list (no declaration block); trailing tokens are a [`MochaError::Parse`].
pub fn parse_selector_list(input: &str) -> MochaResult<Vec<Selector>> {
    let mut parser = Parser::new(tokenize(input)?);
    parser.skip_whitespace();
    if parser.at_end() {
        return Err(MochaError::Parse("expected a selector".to_string()));
    }
    let selectors = parser.parse_selector_list()?;
    parser.skip_whitespace();
    if !parser.at_end() {
        return Err(MochaError::Parse(format!(
            "unexpected tokens after selector list: {:?}",
            parser.peek()
        )));
    }
    Ok(selectors)
}

/// Parse the body of a `style="…"` attribute into declarations. Forgiving:
/// unsupported declarations are skipped, the rest are kept.
pub fn parse_inline_style(input: &str) -> MochaResult<Vec<Declaration>> {
    let mut parser = Parser::new(tokenize(input)?);
    let mut declarations = Vec::new();
    loop {
        parser.skip_whitespace();
        if parser.at_end() {
            break;
        }
        if matches!(parser.peek(), Some(CssToken::Semicolon)) {
            parser.pos += 1;
            continue;
        }
        let before = parser.pos;
        if let Ok(decls) = parser.parse_declaration() {
            declarations.extend(decls);
        }
        parser.skip_to_declaration_end();
        if parser.pos == before {
            parser.pos += 1;
        }
    }
    Ok(declarations)
}

/// Can `token` begin a compound selector? Used to tell a descendant combinator
/// (whitespace then a compound) from trailing whitespace.
fn starts_compound(token: &CssToken) -> bool {
    matches!(
        token,
        CssToken::Ident(_)
            | CssToken::Star
            | CssToken::Hash(_)
            | CssToken::Dot
            | CssToken::Colon
            | CssToken::Delim('[')
    )
}

/// Interpret the whitespace-free token run of an `:nth-*` argument as `an+b`.
fn nth_from_tokens(tokens: &[CssToken]) -> Option<Nth> {
    let mut text = String::new();
    for token in tokens {
        match token {
            CssToken::Ident(name) => text.push_str(&name.to_ascii_lowercase()),
            CssToken::Number(n) => text.push_str(&(*n as i64).to_string()),
            CssToken::Dimension(n, unit) => {
                text.push_str(&(*n as i64).to_string());
                text.push_str(&unit.to_ascii_lowercase());
            }
            CssToken::Delim(c) => text.push(*c),
            _ => return None,
        }
    }
    parse_anplusb(&text)
}

/// Parse the `an+b` micro-grammar from a compact string such as `2n+1`, `odd`,
/// `-n+3`, or `5`.
fn parse_anplusb(text: &str) -> Option<Nth> {
    match text {
        "odd" => return Some(Nth { a: 2, b: 1 }),
        "even" => return Some(Nth { a: 2, b: 0 }),
        _ => {}
    }
    match text.find('n') {
        Some(n_index) => {
            let a = match &text[..n_index] {
                "" | "+" => 1,
                "-" => -1,
                other => other.parse::<i32>().ok()?,
            };
            let rest = &text[n_index + 1..];
            let b = if rest.is_empty() {
                0
            } else {
                rest.strip_prefix('+').unwrap_or(rest).parse::<i32>().ok()?
            };
            Some(Nth { a, b })
        }
        None => Some(Nth {
            a: 0,
            b: text.parse::<i32>().ok()?,
        }),
    }
}

struct Parser {
    tokens: Vec<CssToken>,
    pos: usize,
    /// Notes about skipped selectors/declarations/at-rules (forgiving parsing).
    skipped: Vec<String>,
}

impl Parser {
    fn new(tokens: Vec<CssToken>) -> Parser {
        Parser {
            tokens,
            pos: 0,
            skipped: Vec::new(),
        }
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

    /// Parse one rule with recovery: unsupported selectors in the list are
    /// dropped, a missing/garbled block is skipped. Returns `None` only when the
    /// input did not contain a parseable rule here (the caller still advances).
    fn parse_rule_forgiving(&mut self, source_order: usize) -> Option<StyleRule> {
        let selectors = self.parse_selector_list_forgiving();
        self.skip_whitespace();
        match self.peek() {
            Some(CssToken::LeftBrace) => self.pos += 1,
            _ => {
                // No block where one was expected: resynchronize past it.
                self.recover_rule();
                return None;
            }
        }
        let declarations = self.parse_declaration_block_forgiving();
        Some(StyleRule {
            selectors,
            declarations,
            source_order,
        })
    }

    /// Parse a comma-separated selector list, dropping any selector that uses
    /// grammar Mocha does not support (so a rule like `a, a:hover { … }` keeps
    /// the `a` selector). Stops at `{` or end.
    fn parse_selector_list_forgiving(&mut self) -> Vec<Selector> {
        let mut selectors = Vec::new();
        loop {
            self.skip_whitespace();
            if matches!(self.peek(), None | Some(CssToken::LeftBrace)) {
                break;
            }
            match self.parse_selector() {
                Ok(selector) => selectors.push(selector),
                Err(error) => {
                    self.skipped.push(format!("CSS selector skipped: {error}"));
                    self.recover_selector();
                }
            }
            self.skip_whitespace();
            if matches!(self.peek(), Some(CssToken::Comma)) {
                self.pos += 1;
            } else {
                break;
            }
        }
        selectors
    }

    /// Parse a `{ … }` body, skipping declarations that fail to parse.
    fn parse_declaration_block_forgiving(&mut self) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(CssToken::RightBrace) => {
                    self.pos += 1;
                    break;
                }
                None => break, // tolerate a missing closing brace at EOF
                Some(CssToken::Semicolon) => self.pos += 1, // empty declaration
                _ => {
                    let before = self.pos;
                    match self.parse_declaration() {
                        Ok(decls) => declarations.extend(decls),
                        Err(error) => self
                            .skipped
                            .push(format!("CSS declaration skipped: {error}")),
                    }
                    // Always resynchronize to the terminator, whether the
                    // declaration parsed or not, so neither path overshoots.
                    self.skip_to_declaration_end();
                    if self.pos == before {
                        self.pos += 1;
                    }
                }
            }
        }
        declarations
    }

    /// Skip tokens of an unsupported selector up to the next `,` or `{`.
    fn recover_selector(&mut self) {
        while !matches!(
            self.peek(),
            None | Some(CssToken::Comma) | Some(CssToken::LeftBrace)
        ) {
            self.pos += 1;
        }
    }

    /// Resynchronize after a malformed rule: skip a balanced `{ … }` block if one
    /// is ahead, else skip to the next `;`.
    fn recover_rule(&mut self) {
        loop {
            match self.peek() {
                None => return,
                Some(CssToken::LeftBrace) => {
                    self.skip_balanced_block();
                    return;
                }
                Some(CssToken::Semicolon) => {
                    self.pos += 1;
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    /// Skip an at-rule: its prelude, then either a `;` (statement at-rule like
    /// `@import`) or a balanced `{ … }` block (`@media`, `@font-face`, …).
    fn skip_at_rule(&mut self) {
        loop {
            match self.peek() {
                None => return,
                Some(CssToken::Semicolon) => {
                    self.pos += 1;
                    return;
                }
                Some(CssToken::LeftBrace) => {
                    self.skip_balanced_block();
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    /// Skip a balanced `{ … }` block (the cursor must be on the opening `{`).
    fn skip_balanced_block(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                None => return,
                Some(CssToken::LeftBrace) => {
                    depth += 1;
                    self.pos += 1;
                }
                Some(CssToken::RightBrace) => {
                    self.pos += 1;
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => self.pos += 1,
            }
        }
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
        self.skip_whitespace();
        let mut parts = vec![self.parse_compound_selector()?];
        let mut combinators = Vec::new();
        while let Some(combinator) = self.parse_combinator()? {
            self.skip_whitespace();
            parts.push(self.parse_compound_selector()?);
            combinators.push(combinator);
        }
        Ok(Selector { parts, combinators })
    }

    /// After a compound selector, read the combinator (if any) that leads to the
    /// next compound. Returns `None` at the end of this selector (`,`, `{`, EOF).
    /// An explicit `>`/`+`/`~` (with optional surrounding whitespace) wins over a
    /// plain descendant; bare whitespace before another compound is descendant.
    fn parse_combinator(&mut self) -> MochaResult<Option<Combinator>> {
        let had_whitespace = matches!(self.peek(), Some(CssToken::Whitespace));
        self.skip_whitespace();
        match self.peek() {
            Some(CssToken::Delim('>')) => {
                self.pos += 1;
                Ok(Some(Combinator::Child))
            }
            Some(CssToken::Delim('+')) => {
                self.pos += 1;
                Ok(Some(Combinator::NextSibling))
            }
            Some(CssToken::Delim('~')) => {
                self.pos += 1;
                Ok(Some(Combinator::SubsequentSibling))
            }
            Some(token) if had_whitespace && starts_compound(token) => {
                Ok(Some(Combinator::Descendant))
            }
            _ => Ok(None),
        }
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
                    simple_selectors.push(self.parse_pseudo()?);
                }
                Some(CssToken::Delim('[')) => {
                    simple_selectors.push(self.parse_attribute_selector()?);
                }
                Some(CssToken::Delim('@')) => {
                    return Err(MochaError::UnsupportedFeature(
                        "at-rules (such as @media and @import) are not valid in a selector"
                            .to_string(),
                    ))
                }
                // `>`, `+`, `~`, whitespace, `,`, `{`, and EOF all end a compound;
                // combinators are handled by `parse_combinator`.
                _ => break,
            }
        }
        if simple_selectors.is_empty() {
            return Err(MochaError::Parse("expected a simple selector".to_string()));
        }
        Ok(CompoundSelector { simple_selectors })
    }

    /// Parse an attribute selector starting at `[`: `[name]`, `[name=value]`,
    /// `[name~=value]`, `[name|=value]`, `[name^=value]`, `[name$=value]`, or
    /// `[name*=value]`. The value may be an identifier or a quoted string.
    fn parse_attribute_selector(&mut self) -> MochaResult<SimpleSelector> {
        self.pos += 1; // consume '['
        self.skip_whitespace();
        let name = match self.peek() {
            Some(CssToken::Ident(name)) => {
                let name = name.to_ascii_lowercase();
                self.pos += 1;
                name
            }
            other => {
                return Err(MochaError::Parse(format!(
                    "expected an attribute name after '[', found {other:?}"
                )))
            }
        };
        self.skip_whitespace();

        // An optional prefix character before the `=` selects the match kind.
        // `*` arrives as its own `Star` token, the rest as `Delim`s.
        let prefix = match self.peek() {
            Some(CssToken::Star) => {
                self.pos += 1;
                Some('*')
            }
            Some(CssToken::Delim(c @ ('~' | '|' | '^' | '$'))) => {
                let c = *c;
                self.pos += 1;
                Some(c)
            }
            _ => None,
        };

        let matcher = if prefix.is_none() && matches!(self.peek(), Some(CssToken::Delim(']'))) {
            AttributeMatch::Exists
        } else {
            match self.peek() {
                Some(CssToken::Delim('=')) => self.pos += 1,
                other => {
                    return Err(MochaError::Parse(format!(
                        "expected '=' in attribute selector, found {other:?}"
                    )))
                }
            }
            self.skip_whitespace();
            let value = match self.peek() {
                Some(CssToken::Ident(value)) => {
                    let value = value.clone();
                    self.pos += 1;
                    value
                }
                Some(CssToken::Str(value)) => {
                    let value = value.clone();
                    self.pos += 1;
                    value
                }
                other => {
                    return Err(MochaError::Parse(format!(
                        "expected an attribute value, found {other:?}"
                    )))
                }
            };
            match prefix {
                None => AttributeMatch::Equals(value),
                Some('~') => AttributeMatch::Includes(value),
                Some('|') => AttributeMatch::DashMatch(value),
                Some('^') => AttributeMatch::Prefix(value),
                Some('$') => AttributeMatch::Suffix(value),
                Some('*') => AttributeMatch::Substring(value),
                Some(other) => {
                    return Err(MochaError::Parse(format!(
                        "unsupported attribute matcher '{other}='"
                    )))
                }
            }
        };

        self.skip_whitespace();
        match self.peek() {
            Some(CssToken::Delim(']')) => self.pos += 1,
            other => {
                return Err(MochaError::Parse(format!(
                    "expected ']' to close attribute selector, found {other:?}"
                )))
            }
        }
        Ok(SimpleSelector::Attribute(AttributeSelector {
            name,
            matcher,
        }))
    }

    /// Parse a pseudo-class or pseudo-element starting at `:`. A leading `::`
    /// marks a pseudo-element, which is parsed but inert. Functional pseudos
    /// (`:nth-child(…)`, `:not(…)`) consume a parenthesised argument.
    fn parse_pseudo(&mut self) -> MochaResult<SimpleSelector> {
        self.pos += 1; // consume ':'
        let is_element = if matches!(self.peek(), Some(CssToken::Colon)) {
            self.pos += 1; // consume the second ':'
            true
        } else {
            false
        };
        let name = match self.peek() {
            Some(CssToken::Ident(name)) => {
                let name = name.to_ascii_lowercase();
                self.pos += 1;
                name
            }
            other => {
                return Err(MochaError::Parse(format!(
                    "expected a pseudo-class name after ':', found {other:?}"
                )))
            }
        };

        // Pseudo-elements are parsed but never match in a static render.
        if is_element {
            return Ok(SimpleSelector::PseudoClass(PseudoClass::Inert(name)));
        }

        let pseudo = match name.as_str() {
            "root" => PseudoClass::Root,
            "empty" => PseudoClass::Empty,
            "first-child" => PseudoClass::FirstChild,
            "last-child" => PseudoClass::LastChild,
            "only-child" => PseudoClass::OnlyChild,
            "first-of-type" => PseudoClass::FirstOfType,
            "last-of-type" => PseudoClass::LastOfType,
            "only-of-type" => PseudoClass::OnlyOfType,
            "nth-child" => PseudoClass::NthChild(self.parse_nth_argument()?),
            "nth-last-child" => PseudoClass::NthLastChild(self.parse_nth_argument()?),
            "nth-of-type" => PseudoClass::NthOfType(self.parse_nth_argument()?),
            "nth-last-of-type" => PseudoClass::NthLastOfType(self.parse_nth_argument()?),
            "not" => PseudoClass::Not(self.parse_not_argument()?),
            // Dynamic pseudo-classes (and any unknown one): parsed, inert. State
            // is never faked, so the rule is retained but never matches.
            "hover" | "focus" | "focus-within" | "focus-visible" | "active" | "visited"
            | "link" | "any-link" | "target" | "checked" | "enabled" | "disabled" | "required"
            | "optional" | "valid" | "invalid" | "default" => PseudoClass::Inert(name),
            other => {
                return Err(MochaError::UnsupportedFeature(format!(
                    "unsupported pseudo-class ':{other}'"
                )))
            }
        };
        Ok(SimpleSelector::PseudoClass(pseudo))
    }

    /// Parse a parenthesised `an+b` argument of an `:nth-*` pseudo-class.
    fn parse_nth_argument(&mut self) -> MochaResult<Nth> {
        let tokens = self.collect_parenthesised()?;
        nth_from_tokens(&tokens)
            .ok_or_else(|| MochaError::Parse("invalid :nth-* argument".to_string()))
    }

    /// Parse a parenthesised `:not(<compound>)` argument into its simple
    /// selectors. Only a single compound (no combinators, no nested `:not`) is
    /// supported.
    fn parse_not_argument(&mut self) -> MochaResult<Vec<SimpleSelector>> {
        match self.peek() {
            Some(CssToken::Delim('(')) => self.pos += 1,
            other => {
                return Err(MochaError::Parse(format!(
                    "expected '(' after ':not', found {other:?}"
                )))
            }
        }
        self.skip_whitespace();
        let compound = self.parse_compound_selector()?;
        if compound
            .simple_selectors
            .iter()
            .any(|s| matches!(s, SimpleSelector::PseudoClass(PseudoClass::Not(_))))
        {
            return Err(MochaError::UnsupportedFeature(
                "nested ':not()' is not supported".to_string(),
            ));
        }
        self.skip_whitespace();
        match self.peek() {
            Some(CssToken::Delim(')')) => self.pos += 1,
            other => {
                return Err(MochaError::Parse(format!(
                    "expected ')' to close ':not(', found {other:?}"
                )))
            }
        }
        Ok(compound.simple_selectors)
    }

    /// Collect the raw tokens inside a `( … )` group (the cursor must be just
    /// before the `(`), dropping whitespace. Used for `:nth-*` arguments.
    fn collect_parenthesised(&mut self) -> MochaResult<Vec<CssToken>> {
        match self.peek() {
            Some(CssToken::Delim('(')) => self.pos += 1,
            other => {
                return Err(MochaError::Parse(format!(
                    "expected '(' after a functional pseudo-class, found {other:?}"
                )))
            }
        }
        let mut tokens = Vec::new();
        loop {
            match self.peek() {
                None => {
                    return Err(MochaError::Parse(
                        "unterminated '(' in selector".to_string(),
                    ))
                }
                Some(CssToken::Delim(')')) => {
                    self.pos += 1;
                    return Ok(tokens);
                }
                Some(CssToken::Whitespace) => self.pos += 1,
                Some(token) => {
                    tokens.push(token.clone());
                    self.pos += 1;
                }
            }
        }
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

        let mut value_tokens = self.collect_value_tokens();
        // A trailing `!important` raises the declaration's cascade priority. Split
        // it off the value tokens and remember it; the cascade applies it.
        let important = if let Some(bang) = value_tokens
            .iter()
            .position(|token| matches!(token, CssToken::Delim('!')))
        {
            value_tokens.truncate(bang);
            true
        } else {
            false
        };
        if value_tokens.is_empty() {
            return Err(MochaError::Parse(format!(
                "property '{property_name}' has no value"
            )));
        }
        let mut declarations = build_declarations(&property_name, &value_tokens)?;
        if important {
            for declaration in &mut declarations {
                declaration.important = true;
            }
        }
        Ok(declarations)
    }

    /// Collect the non-whitespace value tokens up to (but not consuming) the
    /// terminating `;`, `}`, or end of input. The terminator is left for the
    /// caller's loop so error recovery never overshoots the next declaration.
    fn collect_value_tokens(&mut self) -> Vec<CssToken> {
        let mut tokens = Vec::new();
        loop {
            match self.peek() {
                Some(CssToken::Semicolon) | Some(CssToken::RightBrace) | None => break,
                Some(CssToken::Whitespace) => self.pos += 1,
                Some(token) => {
                    tokens.push(token.clone());
                    self.pos += 1;
                }
            }
        }
        tokens
    }

    /// Advance past the rest of a declaration: to the next `;` (consumed) or `}`.
    fn skip_to_declaration_end(&mut self) {
        while !matches!(
            self.peek(),
            None | Some(CssToken::Semicolon) | Some(CssToken::RightBrace)
        ) {
            self.pos += 1;
        }
        if matches!(self.peek(), Some(CssToken::Semicolon)) {
            self.pos += 1;
        }
    }
}

/// Construct a normal-priority declaration. `parse_declaration` flips
/// `important` afterwards when the source carried `!important`.
fn decl(property: CssProperty, value: CssValue) -> Declaration {
    Declaration {
        property,
        value,
        important: false,
    }
}

/// Map a property name plus its value tokens to one or more declarations.
fn build_declarations(name: &str, tokens: &[CssToken]) -> MochaResult<Vec<Declaration>> {
    let single = |property: CssProperty, value: CssValue| vec![decl(property, value)];

    match name {
        "display" => Ok(single(CssProperty::Display, display_value(tokens)?)),
        "font-weight" => Ok(single(CssProperty::FontWeight, font_weight_value(tokens)?)),
        "color" => Ok(single(CssProperty::Color, color(tokens)?)),
        "background-color" => Ok(single(CssProperty::BackgroundColor, color(tokens)?)),
        // `background` shorthand: take any color in it, ignore image/position.
        "background" => background_shorthand(tokens),
        "border-color" => Ok(single(CssProperty::BorderColor, color(tokens)?)),
        "border-width" => Ok(single(CssProperty::BorderWidth, length(tokens)?)),
        // `border`/`border-*` shorthand: pull out a width and/or a color.
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            border_shorthand(tokens)
        }
        "text-align" => Ok(single(CssProperty::TextAlign, text_align_value(tokens)?)),
        "line-height" => Ok(single(CssProperty::LineHeight, line_height_value(tokens)?)),
        "max-width" => Ok(single(CssProperty::MaxWidth, length(tokens)?)),
        "flex-direction" => Ok(single(
            CssProperty::FlexDirection,
            ident_keyword(tokens, &["row", "row-reverse", "column", "column-reverse"])?,
        )),
        "justify-content" => Ok(single(
            CssProperty::JustifyContent,
            ident_keyword(
                tokens,
                &[
                    "flex-start",
                    "flex-end",
                    "center",
                    "space-between",
                    "space-around",
                    "space-evenly",
                    "start",
                    "end",
                    "left",
                    "right",
                ],
            )?,
        )),
        "align-items" => Ok(single(
            CssProperty::AlignItems,
            ident_keyword(
                tokens,
                &[
                    "stretch",
                    "flex-start",
                    "flex-end",
                    "center",
                    "baseline",
                    "start",
                    "end",
                ],
            )?,
        )),
        "gap" | "grid-gap" => Ok(single(
            CssProperty::Gap,
            length(&tokens[..1.min(tokens.len())])?,
        )),
        "row-gap" | "column-gap" => Ok(single(CssProperty::Gap, length(tokens)?)),
        "flex-grow" => Ok(single(CssProperty::FlexGrow, number_value(tokens)?)),
        // `border-radius`: take the first radius value (no per-corner / elliptical).
        "border-radius" => Ok(single(
            CssProperty::BorderRadius,
            length(&tokens[..1.min(tokens.len())])?,
        )),
        // `flex: <grow> [shrink] [basis]` — take the leading grow number.
        "flex" => flex_shorthand(tokens),
        "font-size" => Ok(single(CssProperty::FontSize, length(tokens)?)),
        "width" => Ok(single(CssProperty::Width, length_or_auto(tokens)?)),
        "height" => Ok(single(CssProperty::Height, length_or_auto(tokens)?)),
        "margin-top" => Ok(single(CssProperty::MarginTop, length_or_auto(tokens)?)),
        "margin-right" => Ok(single(CssProperty::MarginRight, length_or_auto(tokens)?)),
        "margin-bottom" => Ok(single(CssProperty::MarginBottom, length_or_auto(tokens)?)),
        "margin-left" => Ok(single(CssProperty::MarginLeft, length_or_auto(tokens)?)),
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
            true,
        ),
        "padding" => expand_box_shorthand(
            tokens,
            [
                CssProperty::PaddingTop,
                CssProperty::PaddingRight,
                CssProperty::PaddingBottom,
                CssProperty::PaddingLeft,
            ],
            false,
        ),
        other => Err(MochaError::UnsupportedFeature(format!(
            "CSS property '{other}' is not supported"
        ))),
    }
}

/// `display`: `flex`/`inline-flex` become a flex container; `grid`/`table`/etc.
/// fall back to block; `inline-*` to inline; `none` generates no box.
fn display_value(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Ident(name)] => {
            let lower = name.to_ascii_lowercase();
            let mapped = match lower.as_str() {
                "none" => "none",
                "flex" | "inline-flex" => "flex",
                "inline" | "inline-block" | "inline-grid" => "inline",
                _ => "block",
            };
            Ok(CssValue::Keyword(mapped.to_string()))
        }
        _ => Err(MochaError::Parse("expected a display keyword".to_string())),
    }
}

/// Match a single-ident value against an allow-list, returning it as a keyword.
fn ident_keyword(tokens: &[CssToken], allowed: &[&str]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Ident(name)] => {
            let lower = name.to_ascii_lowercase();
            if allowed.contains(&lower.as_str()) {
                Ok(CssValue::Keyword(lower))
            } else {
                Err(MochaError::UnsupportedFeature(format!(
                    "unsupported keyword '{name}'"
                )))
            }
        }
        _ => Err(MochaError::Parse("expected a single keyword".to_string())),
    }
}

/// A unitless number value.
fn number_value(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Number(n)] => Ok(CssValue::Number(*n)),
        _ => Err(MochaError::Parse("expected a number".to_string())),
    }
}

/// `flex` shorthand: keep the flex-grow factor (`none`→0, `auto`/`1`→1, the
/// first number otherwise).
fn flex_shorthand(tokens: &[CssToken]) -> MochaResult<Vec<Declaration>> {
    let grow = match tokens.first() {
        Some(CssToken::Ident(name)) if name.eq_ignore_ascii_case("none") => 0.0,
        Some(CssToken::Ident(name)) if name.eq_ignore_ascii_case("auto") => 1.0,
        _ => tokens
            .iter()
            .find_map(|t| match t {
                CssToken::Number(n) => Some(*n),
                _ => None,
            })
            .unwrap_or(1.0),
    };
    Ok(vec![decl(CssProperty::FlexGrow, CssValue::Number(grow))])
}

/// `font-weight`: `bold`/`bolder` (and numeric ≥ 600) are bold, else normal.
fn font_weight_value(tokens: &[CssToken]) -> MochaResult<CssValue> {
    let bold = match tokens {
        [CssToken::Ident(name)] => matches!(name.to_ascii_lowercase().as_str(), "bold" | "bolder"),
        [CssToken::Number(n)] => *n >= 600.0,
        _ => return Err(MochaError::Parse("expected a font-weight".to_string())),
    };
    Ok(CssValue::Keyword(
        if bold { "bold" } else { "normal" }.to_string(),
    ))
}

/// `text-align`: normalize to `left`/`right`/`center` (`start`→left, `end`→right,
/// `justify`→left).
fn text_align_value(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Ident(name)] => {
            let value = match name.to_ascii_lowercase().as_str() {
                "center" => "center",
                "right" | "end" => "right",
                "left" | "start" | "justify" => "left",
                other => {
                    return Err(MochaError::UnsupportedFeature(format!(
                        "text-align '{other}' is not supported"
                    )))
                }
            };
            Ok(CssValue::Keyword(value.to_string()))
        }
        _ => Err(MochaError::Parse(
            "expected a text-align keyword".to_string(),
        )),
    }
}

/// `line-height`: a unitless multiplier ([`CssValue::Number`]), a length, or
/// `normal` (≈ 1.2).
fn line_height_value(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Number(n)] => Ok(CssValue::Number(*n)),
        [CssToken::Ident(name)] if name.eq_ignore_ascii_case("normal") => Ok(CssValue::Number(1.2)),
        [token] => Ok(dimension_value(token)?),
        _ => Err(MochaError::Parse("expected a line-height".to_string())),
    }
}

/// `background` shorthand: keep any color found, ignore the rest (images,
/// position, repeat). Errors only when no color is present (so it's skipped).
fn background_shorthand(tokens: &[CssToken]) -> MochaResult<Vec<Declaration>> {
    for token in tokens {
        if let Ok(CssValue::Color(c)) = color(std::slice::from_ref(token)) {
            return Ok(vec![decl(CssProperty::BackgroundColor, CssValue::Color(c))]);
        }
    }
    // rgb()/hsl() span several tokens; try the whole list too.
    if let Ok(value @ CssValue::Color(_)) = color(tokens) {
        return Ok(vec![decl(CssProperty::BackgroundColor, value)]);
    }
    Err(MochaError::UnsupportedFeature(
        "background shorthand without a usable color".to_string(),
    ))
}

/// `border`/`border-*` shorthand: emit a `border-width` for any length and a
/// `border-color` for any color (the line style is ignored).
fn border_shorthand(tokens: &[CssToken]) -> MochaResult<Vec<Declaration>> {
    let mut decls = Vec::new();
    for token in tokens {
        if let Ok(value) = length(std::slice::from_ref(token)) {
            decls.push(decl(CssProperty::BorderWidth, value));
        } else if let Ok(value @ CssValue::Color(_)) = color(std::slice::from_ref(token)) {
            decls.push(decl(CssProperty::BorderColor, value));
        }
    }
    if decls.is_empty() {
        return Err(MochaError::UnsupportedFeature(
            "border shorthand with no width or color".to_string(),
        ));
    }
    Ok(decls)
}

/// Expand a 1–4 value box shorthand (margin/padding) into four longhands.
/// `allow_auto` accepts the `auto` keyword (margins) for centering.
fn expand_box_shorthand(
    tokens: &[CssToken],
    [top, right, bottom, left]: [CssProperty; 4],
    allow_auto: bool,
) -> MochaResult<Vec<Declaration>> {
    let mut values = Vec::new();
    for token in tokens {
        if allow_auto {
            if let CssToken::Ident(name) = token {
                if name.eq_ignore_ascii_case("auto") {
                    values.push(CssValue::Keyword("auto".to_string()));
                    continue;
                }
            }
        }
        values.push(dimension_value(token)?);
    }
    let (t, r, b, l) = match values.as_slice() {
        [all] => (all, all, all, all),
        [v, h] => (v, h, v, h),
        [t, h, b] => (t, h, b, h),
        [t, r, b, l] => (t, r, b, l),
        _ => {
            return Err(MochaError::Parse(
                "box shorthand expects between 1 and 4 values".to_string(),
            ))
        }
    };
    Ok(vec![
        decl(top, t.clone()),
        decl(right, r.clone()),
        decl(bottom, b.clone()),
        decl(left, l.clone()),
    ])
}

/// A single length value (px/em/rem/%/pt or unitless 0).
fn length(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [token] => dimension_value(token),
        _ => Err(MochaError::Parse(format!(
            "expected a single length, found {tokens:?}"
        ))),
    }
}

/// A length, or the `auto` keyword (for width/height/margins).
fn length_or_auto(tokens: &[CssToken]) -> MochaResult<CssValue> {
    if let [CssToken::Ident(name)] = tokens {
        if name.eq_ignore_ascii_case("auto") {
            return Ok(CssValue::Keyword("auto".to_string()));
        }
    }
    length(tokens)
}

/// Convert a single dimension token to a typed [`CssValue`].
fn dimension_value(token: &CssToken) -> MochaResult<CssValue> {
    match token {
        CssToken::Dimension(value, unit) => match unit.as_str() {
            "px" => Ok(CssValue::LengthPx(*value)),
            "em" => Ok(CssValue::Em(*value)),
            "rem" => Ok(CssValue::Rem(*value)),
            "%" => Ok(CssValue::Percent(*value)),
            "pt" => Ok(CssValue::LengthPx(*value * 96.0 / 72.0)),
            "pc" => Ok(CssValue::LengthPx(*value * 16.0)),
            "in" => Ok(CssValue::LengthPx(*value * 96.0)),
            "cm" => Ok(CssValue::LengthPx(*value * 96.0 / 2.54)),
            "mm" => Ok(CssValue::LengthPx(*value * 96.0 / 25.4)),
            other => Err(MochaError::UnsupportedFeature(format!(
                "CSS unit '{other}' is not supported"
            ))),
        },
        CssToken::Number(value) if *value == 0.0 => Ok(CssValue::LengthPx(0.0)),
        CssToken::Number(_) => Err(MochaError::Parse(
            "lengths require a unit (except 0)".to_string(),
        )),
        other => Err(MochaError::Parse(format!(
            "expected a length, found {other:?}"
        ))),
    }
}

/// Parse a color value: hex, a named color, `transparent`, or a
/// `rgb()/rgba()/hsl()/hsla()` function.
fn color(tokens: &[CssToken]) -> MochaResult<CssValue> {
    match tokens {
        [CssToken::Hash(hex)] => Ok(CssValue::Color(parse_hex_color(hex)?)),
        [CssToken::Ident(name)] => {
            let lower = name.to_ascii_lowercase();
            if lower == "transparent" {
                return Ok(CssValue::Color(crate::Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                }));
            }
            named_color(&lower).map(CssValue::Color).ok_or_else(|| {
                MochaError::UnsupportedFeature(format!("color '{name}' is not supported"))
            })
        }
        [CssToken::Ident(name), CssToken::Delim('('), rest @ ..] => {
            color_function(&name.to_ascii_lowercase(), rest)
        }
        _ => Err(MochaError::UnsupportedFeature(
            "unsupported color value".to_string(),
        )),
    }
}

/// Parse the arguments of `rgb()/rgba()/hsl()/hsla()` (a trailing `)` may be
/// present). Commas and slashes between components are ignored.
fn color_function(name: &str, args: &[CssToken]) -> MochaResult<CssValue> {
    let mut numbers: Vec<f32> = Vec::new();
    for token in args {
        match token {
            CssToken::Number(n) => numbers.push(*n),
            CssToken::Dimension(n, unit) if unit == "%" => numbers.push(*n),
            _ => {} // skip commas, '/', ')', whitespace
        }
    }
    let color = match name {
        "rgb" | "rgba" if numbers.len() >= 3 => crate::Color {
            r: numbers[0].clamp(0.0, 255.0) as u8,
            g: numbers[1].clamp(0.0, 255.0) as u8,
            b: numbers[2].clamp(0.0, 255.0) as u8,
            a: numbers
                .get(3)
                .map(|a| (a.clamp(0.0, 1.0) * 255.0) as u8)
                .unwrap_or(255),
        },
        "hsl" | "hsla" if numbers.len() >= 3 => {
            let (r, g, b) = hsl_to_rgb(numbers[0], numbers[1] / 100.0, numbers[2] / 100.0);
            crate::Color {
                r,
                g,
                b,
                a: numbers
                    .get(3)
                    .map(|a| (a.clamp(0.0, 1.0) * 255.0) as u8)
                    .unwrap_or(255),
            }
        }
        _ => {
            return Err(MochaError::UnsupportedFeature(format!(
                "color function '{name}()' is not supported"
            )))
        }
    };
    Ok(CssValue::Color(color))
}

/// Convert HSL (h in degrees, s and l in `0..=1`) to 8-bit RGB.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue = |t: f32| {
        let t = t.rem_euclid(1.0);
        let c = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (c * 255.0).round() as u8
    };
    (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
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
    fn parse_standalone_selector_list_for_query() {
        let selectors = super::parse_selector_list("p.intro, div span").unwrap();
        assert_eq!(selectors.len(), 2);
        assert_eq!(selectors[1].parts.len(), 2); // descendant: div span
                                                 // A declaration block is not a selector list.
        assert!(super::parse_selector_list("p { color: red; }").is_err());
        // A dynamic pseudo-class now parses (it is inert, not an error).
        assert!(super::parse_selector_list("p:hover").is_ok());
        // A genuinely unknown pseudo-class still surfaces clearly.
        assert!(super::parse_selector_list("p:totally-unknown").is_err());
        assert!(super::parse_selector_list("").is_err());
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
    fn unknown_property_is_skipped_keeping_the_rest() {
        // An unknown property is dropped; supported declarations in the same
        // rule survive (forgiving parsing).
        let rule = only_rule("p { float: left; color: red; }");
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0].property, CssProperty::Color);
    }

    #[test]
    fn unsupported_unit_is_skipped() {
        // `vh` needs viewport units Mocha doesn't resolve yet: that declaration
        // is dropped while the supported `color` is kept.
        let sheet = parse_stylesheet("p { font-size: 2vh; color: blue; }").unwrap();
        let rule = &sheet.rules[0];
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0].property, CssProperty::Color);
    }

    #[test]
    fn relative_units_and_functions_parse() {
        // em/rem/% and rgb()/hsl() now parse into typed values.
        let rule = only_rule("p { font-size: 2em; width: 50%; color: rgb(255, 0, 0); }");
        assert!(rule
            .declarations
            .iter()
            .any(|d| d.value == CssValue::Em(2.0)));
        assert!(rule
            .declarations
            .iter()
            .any(|d| d.value == CssValue::Percent(50.0)));
        assert!(rule
            .declarations
            .iter()
            .any(|d| matches!(d.value, CssValue::Color(c) if c.r == 255 && c.g == 0 && c.b == 0)));
    }

    #[test]
    fn text_align_and_margin_auto_parse() {
        let rule = only_rule("div { text-align: center; margin: 0 auto; }");
        assert!(rule
            .declarations
            .iter()
            .any(|d| d.property == CssProperty::TextAlign
                && d.value == CssValue::Keyword("center".into())));
        assert!(rule
            .declarations
            .iter()
            .any(|d| d.property == CssProperty::MarginLeft
                && d.value == CssValue::Keyword("auto".into())));
    }

    #[test]
    fn malformed_declaration_is_skipped() {
        // `color` with no value is dropped; the next declaration still parses.
        let rule = only_rule("p { color; font-size: 12px; }");
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0].property, CssProperty::FontSize);
    }

    #[test]
    fn important_is_recorded_and_value_kept() {
        let rule = only_rule("p { color: red !important; }");
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(
            rule.declarations[0].value,
            CssValue::Color(Color::rgb(255, 0, 0))
        );
        assert!(rule.declarations[0].important);
    }

    #[test]
    fn important_marks_every_expanded_longhand() {
        // A shorthand with `!important` marks all of its longhands important.
        let rule = only_rule("p { margin: 4px !important; }");
        assert_eq!(rule.declarations.len(), 4);
        assert!(rule.declarations.iter().all(|d| d.important));
        // A normal declaration is not important.
        let rule = only_rule("p { color: red; }");
        assert!(!rule.declarations[0].important);
    }

    #[test]
    fn unsupported_selector_drops_only_that_rule() {
        // An unknown pseudo-class is dropped, but neighbouring rules survive.
        let sheet =
            parse_stylesheet("p:totally-unknown { color: red; } a { color: blue; }").unwrap();
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(
            sheet.rules[0].selectors[0].parts[0].simple_selectors,
            vec![SimpleSelector::Type("a".into())]
        );
    }

    #[test]
    fn at_rules_and_unsupported_syntax_are_skipped_not_fatal() {
        // None of these error; supported rules around them still parse.
        for input in [
            "@media screen { p { color: red; } }",
            "@import url(x.css);",
            "@font-face { font-family: x; }",
            "p:hover { color: red; }",
            "p::before { color: red; }",
            "p[class] { color: red; }",
            "div + p { color: red; }",
            "p { width: calc(10px + 2px); }",
            "p { width: 50%; }",
        ] {
            let sheet = parse_stylesheet(input);
            assert!(sheet.is_ok(), "`{input}` must not error, got {sheet:?}");
        }
        // A supported rule following a skipped at-rule still applies.
        let sheet =
            parse_stylesheet("@media screen { x { color: red; } } a { color: blue; }").unwrap();
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations.len(), 1);
    }

    #[test]
    fn mixed_partial_selector_list_keeps_supported_selectors() {
        // `a, a:totally-unknown` keeps `a`, drops the unknown-pseudo selector.
        let rule = only_rule("a, a:totally-unknown { color: red; }");
        assert_eq!(rule.selectors.len(), 1);
        assert_eq!(
            rule.selectors[0].parts[0].simple_selectors,
            vec![SimpleSelector::Type("a".into())]
        );
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
