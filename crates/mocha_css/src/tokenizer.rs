//! A small CSS tokenizer.
//!
//! It recognises the lexical units needed by Mocha's CSS subset: identifiers,
//! hashes (`#id` / hex colors), numbers and `px`/`%` dimensions, the punctuation
//! used in rules, and significant whitespace (needed to tell descendant
//! selectors apart). CSS comments are skipped. Unknown punctuation becomes a
//! [`CssToken::Delim`] so the parser can produce a precise error.

use mocha_error::{MochaError, MochaResult};

/// A single CSS token.
#[derive(Debug, Clone, PartialEq)]
pub enum CssToken {
    /// An identifier such as `color` or `block`.
    Ident(String),
    /// A `#`-prefixed run (id selector or hex color), stored without the `#`.
    Hash(String),
    /// A unitless number.
    Number(f32),
    /// A number with a unit, for example `16px` → `(16.0, "px")`.
    Dimension(f32, String),
    /// `:`
    Colon,
    /// `;`
    Semicolon,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `*`
    Star,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// A run of whitespace (significant for descendant selectors).
    Whitespace,
    /// Any other single character (handled/rejected by the parser).
    Delim(char),
}

/// Tokenize a CSS source string.
pub fn tokenize(input: &str) -> MochaResult<Vec<CssToken>> {
    let chars: Vec<char> = input.chars().collect();
    let mut pos = 0;
    let mut tokens = Vec::new();

    while let Some(&c) = chars.get(pos) {
        if c.is_whitespace() {
            while chars.get(pos).is_some_and(|c| c.is_whitespace()) {
                pos += 1;
            }
            tokens.push(CssToken::Whitespace);
            continue;
        }

        if c == '/' && chars.get(pos + 1) == Some(&'*') {
            pos += 2;
            while pos < chars.len() && !(chars[pos] == '*' && chars.get(pos + 1) == Some(&'/')) {
                pos += 1;
            }
            if pos >= chars.len() {
                return Err(MochaError::Parse(
                    "unterminated CSS comment: missing '*/'".to_string(),
                ));
            }
            pos += 2; // consume "*/"
            continue;
        }

        match c {
            ':' => push_single(&mut tokens, &mut pos, CssToken::Colon),
            ';' => push_single(&mut tokens, &mut pos, CssToken::Semicolon),
            ',' => push_single(&mut tokens, &mut pos, CssToken::Comma),
            '.' if !next_is_digit(&chars, pos) => push_single(&mut tokens, &mut pos, CssToken::Dot),
            '*' => push_single(&mut tokens, &mut pos, CssToken::Star),
            '{' => push_single(&mut tokens, &mut pos, CssToken::LeftBrace),
            '}' => push_single(&mut tokens, &mut pos, CssToken::RightBrace),
            '#' => {
                pos += 1;
                let name = read_name(&chars, &mut pos);
                if name.is_empty() {
                    return Err(MochaError::Parse("expected a name after '#'".to_string()));
                }
                tokens.push(CssToken::Hash(name));
            }
            '-' if next_is_digit(&chars, pos) => tokens.push(read_number(&chars, &mut pos)),
            '0'..='9' => tokens.push(read_number(&chars, &mut pos)),
            '.' => tokens.push(read_number(&chars, &mut pos)),
            c if is_name_start(c) => tokens.push(CssToken::Ident(read_name(&chars, &mut pos))),
            other => push_single(&mut tokens, &mut pos, CssToken::Delim(other)),
        }
    }

    Ok(tokens)
}

fn push_single(tokens: &mut Vec<CssToken>, pos: &mut usize, token: CssToken) {
    tokens.push(token);
    *pos += 1;
}

fn next_is_digit(chars: &[char], pos: usize) -> bool {
    chars.get(pos + 1).is_some_and(|c| c.is_ascii_digit())
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '-' || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Read an identifier-like name (used for idents, hash bodies, and units).
fn read_name(chars: &[char], pos: &mut usize) -> String {
    let start = *pos;
    while chars.get(*pos).is_some_and(|&c| is_name_char(c)) {
        *pos += 1;
    }
    chars[start..*pos].iter().collect()
}

/// Read a number, then an optional `%` or identifier unit, producing either a
/// [`CssToken::Number`] or [`CssToken::Dimension`].
fn read_number(chars: &[char], pos: &mut usize) -> CssToken {
    let start = *pos;
    if chars.get(*pos) == Some(&'-') {
        *pos += 1;
    }
    while chars.get(*pos).is_some_and(|c| c.is_ascii_digit()) {
        *pos += 1;
    }
    if chars.get(*pos) == Some(&'.') {
        *pos += 1;
        while chars.get(*pos).is_some_and(|c| c.is_ascii_digit()) {
            *pos += 1;
        }
    }
    let number: f32 = chars[start..*pos]
        .iter()
        .collect::<String>()
        .parse()
        .unwrap_or(0.0);

    if chars.get(*pos) == Some(&'%') {
        *pos += 1;
        return CssToken::Dimension(number, "%".to_string());
    }
    if chars.get(*pos).is_some_and(|&c| is_name_start(c)) {
        let unit = read_name(chars, pos);
        return CssToken::Dimension(number, unit);
    }
    CssToken::Number(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple_rule() {
        let tokens = tokenize("h1 { color: red; }").unwrap();
        assert_eq!(
            tokens,
            vec![
                CssToken::Ident("h1".into()),
                CssToken::Whitespace,
                CssToken::LeftBrace,
                CssToken::Whitespace,
                CssToken::Ident("color".into()),
                CssToken::Colon,
                CssToken::Whitespace,
                CssToken::Ident("red".into()),
                CssToken::Semicolon,
                CssToken::Whitespace,
                CssToken::RightBrace,
            ]
        );
    }

    #[test]
    fn tokenize_px_dimension() {
        let tokens = tokenize("16px").unwrap();
        assert_eq!(tokens, vec![CssToken::Dimension(16.0, "px".into())]);
    }

    #[test]
    fn tokenize_percentage_dimension() {
        let tokens = tokenize("50%").unwrap();
        assert_eq!(tokens, vec![CssToken::Dimension(50.0, "%".into())]);
    }

    #[test]
    fn tokenize_hash() {
        let tokens = tokenize("#hero").unwrap();
        assert_eq!(tokens, vec![CssToken::Hash("hero".into())]);
    }

    #[test]
    fn comments_are_ignored() {
        let tokens = tokenize("a/* hi */b").unwrap();
        assert_eq!(
            tokens,
            vec![CssToken::Ident("a".into()), CssToken::Ident("b".into())]
        );
    }

    #[test]
    fn unterminated_comment_errors() {
        assert!(matches!(
            tokenize("a /* oops").unwrap_err(),
            MochaError::Parse(_)
        ));
    }
}
