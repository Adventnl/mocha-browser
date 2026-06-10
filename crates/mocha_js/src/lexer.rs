//! The lexer: source text → [`Token`]s.
//!
//! Supports numbers, single/double-quoted strings (with simple escapes),
//! identifiers/keywords, `//` and `/* */` comments, and the operator/punctuation
//! set used by the parser. Unterminated strings/comments and unexpected
//! characters are reported as [`MochaError::Parse`] errors.

use mocha_error::{MochaError, MochaResult};

use crate::token::Token;

/// Tokenize `source` into a list of tokens terminated by [`Token::Eof`].
pub fn lex(source: &str) -> MochaResult<Vec<Token>> {
    Lexer {
        chars: source.chars().collect(),
        pos: 0,
    }
    .run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    fn run(mut self) -> MochaResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia()?;
            let Some(&c) = self.chars.get(self.pos) else {
                tokens.push(Token::Eof);
                return Ok(tokens);
            };
            let token = match c {
                '0'..='9' => self.read_number(),
                '"' | '\'' => self.read_string(c)?,
                c if is_ident_start(c) => self.read_ident_or_keyword(),
                _ => self.read_operator()?,
            };
            tokens.push(token);
        }
    }

    /// Skip whitespace and comments.
    fn skip_trivia(&mut self) -> MochaResult<()> {
        loop {
            match self.chars.get(self.pos) {
                Some(c) if c.is_whitespace() => self.pos += 1,
                Some('/') if self.chars.get(self.pos + 1) == Some(&'/') => {
                    while self.chars.get(self.pos).is_some_and(|&c| c != '\n') {
                        self.pos += 1;
                    }
                }
                Some('/') if self.chars.get(self.pos + 1) == Some(&'*') => {
                    self.pos += 2;
                    while self.pos < self.chars.len()
                        && !(self.chars[self.pos] == '*'
                            && self.chars.get(self.pos + 1) == Some(&'/'))
                    {
                        self.pos += 1;
                    }
                    if self.pos >= self.chars.len() {
                        return Err(MochaError::Parse("unterminated block comment".to_string()));
                    }
                    self.pos += 2;
                }
                _ => return Ok(()),
            }
        }
    }

    fn read_number(&mut self) -> Token {
        let start = self.pos;
        while self.chars.get(self.pos).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.chars.get(self.pos) == Some(&'.') {
            self.pos += 1;
            while self.chars.get(self.pos).is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        Token::Number(text.parse().unwrap_or(0.0))
    }

    fn read_string(&mut self, quote: char) -> MochaResult<Token> {
        self.pos += 1; // opening quote
        let mut value = String::new();
        loop {
            match self.chars.get(self.pos) {
                None | Some('\n') => {
                    return Err(MochaError::Parse("unterminated string literal".to_string()))
                }
                Some(&c) if c == quote => {
                    self.pos += 1;
                    return Ok(Token::Str(value));
                }
                Some('\\') => {
                    self.pos += 1;
                    let escaped = match self.chars.get(self.pos) {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('\\') => '\\',
                        Some('\'') => '\'',
                        Some('"') => '"',
                        Some(&other) => other,
                        None => {
                            return Err(MochaError::Parse(
                                "unterminated escape in string literal".to_string(),
                            ))
                        }
                    };
                    value.push(escaped);
                    self.pos += 1;
                }
                Some(&c) => {
                    value.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn read_ident_or_keyword(&mut self) -> Token {
        let start = self.pos;
        while self.chars.get(self.pos).is_some_and(|&c| is_ident_part(c)) {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        match text.as_str() {
            "let" => Token::Let,
            "const" => Token::Const,
            "var" => Token::Var,
            "function" => Token::Function,
            "return" => Token::Return,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "for" => Token::For,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            "undefined" => Token::Undefined,
            _ => Token::Ident(text),
        }
    }

    fn read_operator(&mut self) -> MochaResult<Token> {
        let c = self.chars[self.pos];
        let next = self.chars.get(self.pos + 1).copied();
        let next2 = self.chars.get(self.pos + 2).copied();

        // Three-character operators.
        if c == '=' && next == Some('=') && next2 == Some('=') {
            self.pos += 3;
            return Ok(Token::EqEqEq);
        }
        if c == '!' && next == Some('=') && next2 == Some('=') {
            self.pos += 3;
            return Ok(Token::NotEqEq);
        }

        // Two-character operators.
        let two = match (c, next) {
            ('=', Some('=')) => Some(Token::EqEq),
            ('!', Some('=')) => Some(Token::NotEq),
            ('<', Some('=')) => Some(Token::LtEq),
            ('>', Some('=')) => Some(Token::GtEq),
            ('&', Some('&')) => Some(Token::AndAnd),
            ('|', Some('|')) => Some(Token::OrOr),
            _ => None,
        };
        if let Some(token) = two {
            self.pos += 2;
            return Ok(token);
        }

        // Single-character operators/punctuation.
        let single = match c {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '=' => Token::Assign,
            '<' => Token::Lt,
            '>' => Token::Gt,
            '!' => Token::Bang,
            '.' => Token::Dot,
            ',' => Token::Comma,
            ';' => Token::Semicolon,
            ':' => Token::Colon,
            '?' => Token::Question,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            other => {
                return Err(MochaError::Parse(format!(
                    "unexpected character: {other:?}"
                )))
            }
        };
        self.pos += 1;
        Ok(single)
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_ok(source: &str) -> Vec<Token> {
        lex(source).unwrap()
    }

    #[test]
    fn tokenize_numbers() {
        assert_eq!(
            lex_ok("12 3.5"),
            vec![Token::Number(12.0), Token::Number(3.5), Token::Eof]
        );
    }

    #[test]
    fn tokenize_strings_both_quotes_and_escapes() {
        assert_eq!(
            lex_ok(r#""a\n" 'b'"#),
            vec![Token::Str("a\n".into()), Token::Str("b".into()), Token::Eof]
        );
    }

    #[test]
    fn tokenize_identifiers_and_keywords() {
        assert_eq!(
            lex_ok("let x function"),
            vec![
                Token::Let,
                Token::Ident("x".into()),
                Token::Function,
                Token::Eof
            ]
        );
    }

    #[test]
    fn tokenize_comments_are_skipped() {
        assert_eq!(
            lex_ok("1 // line\n/* block */ 2"),
            vec![Token::Number(1.0), Token::Number(2.0), Token::Eof]
        );
    }

    #[test]
    fn tokenize_operators() {
        assert_eq!(
            lex_ok("=== !== == != <= >= && || ! ="),
            vec![
                Token::EqEqEq,
                Token::NotEqEq,
                Token::EqEq,
                Token::NotEq,
                Token::LtEq,
                Token::GtEq,
                Token::AndAnd,
                Token::OrOr,
                Token::Bang,
                Token::Assign,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn unterminated_string_errors() {
        assert!(matches!(lex("\"oops"), Err(MochaError::Parse(_))));
    }

    #[test]
    fn unexpected_character_errors() {
        assert!(matches!(lex("@"), Err(MochaError::Parse(_))));
    }
}
