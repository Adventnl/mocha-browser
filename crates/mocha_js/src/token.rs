//! Lexical tokens for the JavaScript subset.

/// A single lexical token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A numeric literal.
    Number(f64),
    /// A string literal (already unescaped).
    Str(String),
    /// An identifier.
    Ident(String),

    // Keywords.
    /// `let`
    Let,
    /// `const`
    Const,
    /// `var`
    Var,
    /// `function`
    Function,
    /// `return`
    Return,
    /// `if`
    If,
    /// `else`
    Else,
    /// `while`
    While,
    /// `for`
    For,
    /// `true`
    True,
    /// `false`
    False,
    /// `null`
    Null,
    /// `undefined`
    Undefined,

    // Operators and punctuation.
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `=`
    Assign,
    /// `==`
    EqEq,
    /// `!=`
    NotEq,
    /// `===`
    EqEqEq,
    /// `!==`
    NotEqEq,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
    /// `!`
    Bang,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `:`
    Colon,
    /// `?`
    Question,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,

    /// End of input.
    Eof,
}
