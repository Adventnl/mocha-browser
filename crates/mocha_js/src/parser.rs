//! A recursive-descent parser (with precedence-climbing expressions).
//!
//! Produces a [`Program`] from source. Reports clear [`MochaError::Parse`]
//! errors and never panics on malformed input. Semicolons are optional between
//! statements; function bodies and control-flow bodies use blocks.

use mocha_error::{MochaError, MochaResult};

use crate::ast::{BinaryOp, DeclKind, Expr, LogicalOp, Program, Stmt, UnaryOp};
use crate::lexer::lex;
use crate::token::Token;

/// Parse `source` into a [`Program`].
pub fn parse(source: &str) -> MochaResult<Program> {
    let tokens = lex(source)?;
    let mut parser = Parser { tokens, pos: 0 };
    let mut body = Vec::new();
    while !parser.at_end() {
        body.push(parser.statement()?);
    }
    Ok(Program { body })
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn at_end(&self) -> bool {
        *self.peek() == Token::Eof
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if !self.at_end() {
            self.pos += 1;
        }
        token
    }

    fn check(&self, token: &Token) -> bool {
        self.peek() == token
    }

    fn eat(&mut self, expected: &Token, context: &str) -> MochaResult<()> {
        if self.check(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(MochaError::Parse(format!(
                "expected {expected:?} {context}, found {:?}",
                self.peek()
            )))
        }
    }

    fn eat_optional_semicolon(&mut self) {
        if self.check(&Token::Semicolon) {
            self.pos += 1;
        }
    }

    // --- statements ---------------------------------------------------------

    fn statement(&mut self) -> MochaResult<Stmt> {
        match self.peek() {
            Token::Let | Token::Const | Token::Var => self.variable_declaration(),
            Token::Function => self.function_declaration(),
            Token::Return => self.return_statement(),
            Token::If => self.if_statement(),
            Token::While => self.while_statement(),
            Token::For => self.for_statement(),
            Token::LBrace => Ok(Stmt::Block(self.block()?)),
            _ => {
                let expr = self.expression()?;
                self.eat_optional_semicolon();
                Ok(Stmt::Expression(expr))
            }
        }
    }

    fn variable_declaration(&mut self) -> MochaResult<Stmt> {
        let kind = match self.advance() {
            Token::Let => DeclKind::Let,
            Token::Const => DeclKind::Const,
            Token::Var => DeclKind::Var,
            other => {
                return Err(MochaError::Parse(format!(
                    "expected a declaration, found {other:?}"
                )))
            }
        };
        let name = self.identifier_name("in declaration")?;
        let init = if self.check(&Token::Assign) {
            self.pos += 1;
            Some(self.expression()?)
        } else {
            None
        };
        self.eat_optional_semicolon();
        Ok(Stmt::VariableDeclaration { kind, name, init })
    }

    fn function_declaration(&mut self) -> MochaResult<Stmt> {
        self.pos += 1; // `function`
        let name = self.identifier_name("after 'function'")?;
        let params = self.parameter_list()?;
        let body = self.block()?;
        Ok(Stmt::FunctionDeclaration { name, params, body })
    }

    fn return_statement(&mut self) -> MochaResult<Stmt> {
        self.pos += 1; // `return`
        let value = if self.check(&Token::Semicolon) || self.check(&Token::RBrace) {
            None
        } else {
            Some(self.expression()?)
        };
        self.eat_optional_semicolon();
        Ok(Stmt::Return(value))
    }

    fn if_statement(&mut self) -> MochaResult<Stmt> {
        self.pos += 1; // `if`
        self.eat(&Token::LParen, "after 'if'")?;
        let test = self.expression()?;
        self.eat(&Token::RParen, "after if condition")?;
        let consequent = Box::new(self.statement()?);
        let alternate = if self.check(&Token::Else) {
            self.pos += 1;
            Some(Box::new(self.statement()?))
        } else {
            None
        };
        Ok(Stmt::If {
            test,
            consequent,
            alternate,
        })
    }

    fn while_statement(&mut self) -> MochaResult<Stmt> {
        self.pos += 1; // `while`
        self.eat(&Token::LParen, "after 'while'")?;
        let test = self.expression()?;
        self.eat(&Token::RParen, "after while condition")?;
        let body = Box::new(self.statement()?);
        Ok(Stmt::While { test, body })
    }

    fn for_statement(&mut self) -> MochaResult<Stmt> {
        self.pos += 1; // `for`
        self.eat(&Token::LParen, "after 'for'")?;

        // `for (kind name in/of iterable)`. Detect by looking past the
        // declaration keyword and binding name for `in` (a reserved word) or the
        // contextual `of` (lexed as the identifier "of").
        let sep_is_in_or_of = |token: Option<&Token>| {
            matches!(token, Some(Token::In))
                || matches!(token, Some(Token::Ident(name)) if name == "of")
        };
        if matches!(self.peek(), Token::Let | Token::Const | Token::Var)
            && sep_is_in_or_of(self.tokens.get(self.pos + 2))
        {
            let kind = match self.advance() {
                Token::Let => DeclKind::Let,
                Token::Const => DeclKind::Const,
                Token::Var => DeclKind::Var,
                _ => unreachable!(),
            };
            let name = self.identifier_name("in for-in/for-of binding")?;
            let is_of = matches!(self.advance(), Token::Ident(name) if name == "of");
            let iterable = self.expression()?;
            self.eat(&Token::RParen, "after for-in/for-of iterable")?;
            let body = Box::new(self.statement()?);
            return Ok(if is_of {
                Stmt::ForOf {
                    kind,
                    name,
                    iterable,
                    body,
                }
            } else {
                Stmt::ForIn {
                    kind,
                    name,
                    iterable,
                    body,
                }
            });
        }

        // Initializer.
        let init = if self.check(&Token::Semicolon) {
            self.pos += 1;
            None
        } else if matches!(self.peek(), Token::Let | Token::Const | Token::Var) {
            Some(Box::new(self.variable_declaration()?)) // consumes its own ';'
        } else {
            let expr = self.expression()?;
            self.eat(&Token::Semicolon, "after for-initializer")?;
            Some(Box::new(Stmt::Expression(expr)))
        };

        // Condition.
        let test = if self.check(&Token::Semicolon) {
            None
        } else {
            Some(self.expression()?)
        };
        self.eat(&Token::Semicolon, "after for-condition")?;

        // Update.
        let update = if self.check(&Token::RParen) {
            None
        } else {
            Some(self.expression()?)
        };
        self.eat(&Token::RParen, "after for-update")?;

        let body = Box::new(self.statement()?);
        Ok(Stmt::For {
            init,
            test,
            update,
            body,
        })
    }

    fn block(&mut self) -> MochaResult<Vec<Stmt>> {
        self.eat(&Token::LBrace, "to open a block")?;
        let mut statements = Vec::new();
        while !self.check(&Token::RBrace) && !self.at_end() {
            statements.push(self.statement()?);
        }
        self.eat(&Token::RBrace, "to close a block")?;
        Ok(statements)
    }

    fn parameter_list(&mut self) -> MochaResult<Vec<String>> {
        self.eat(&Token::LParen, "to open parameters")?;
        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            params.push(self.identifier_name("in parameter list")?);
            if !self.check(&Token::RParen) {
                self.eat(&Token::Comma, "between parameters")?;
            }
        }
        self.eat(&Token::RParen, "to close parameters")?;
        Ok(params)
    }

    fn identifier_name(&mut self, context: &str) -> MochaResult<String> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            other => Err(MochaError::Parse(format!(
                "expected an identifier {context}, found {other:?}"
            ))),
        }
    }

    // --- expressions (precedence climbing) ----------------------------------

    fn expression(&mut self) -> MochaResult<Expr> {
        self.assignment()
    }

    fn assignment(&mut self) -> MochaResult<Expr> {
        let left = self.logical_or()?;
        if self.check(&Token::Assign) {
            self.pos += 1;
            if !matches!(
                left,
                Expr::Identifier(_) | Expr::Member { .. } | Expr::Index { .. }
            ) {
                return Err(MochaError::Parse("invalid assignment target".to_string()));
            }
            let value = self.assignment()?; // right-associative
            return Ok(Expr::Assignment {
                target: Box::new(left),
                value: Box::new(value),
            });
        }
        Ok(left)
    }

    fn logical_or(&mut self) -> MochaResult<Expr> {
        let mut left = self.logical_and()?;
        while self.check(&Token::OrOr) {
            self.pos += 1;
            let right = self.logical_and()?;
            left = Expr::Logical {
                op: LogicalOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn logical_and(&mut self) -> MochaResult<Expr> {
        let mut left = self.equality()?;
        while self.check(&Token::AndAnd) {
            self.pos += 1;
            let right = self.equality()?;
            left = Expr::Logical {
                op: LogicalOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn equality(&mut self) -> MochaResult<Expr> {
        let mut left = self.comparison()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinaryOp::Eq,
                Token::NotEq => BinaryOp::NotEq,
                Token::EqEqEq => BinaryOp::StrictEq,
                Token::NotEqEq => BinaryOp::StrictNotEq,
                _ => break,
            };
            self.pos += 1;
            let right = self.comparison()?;
            left = Self::binary(op, left, right);
        }
        Ok(left)
    }

    fn comparison(&mut self) -> MochaResult<Expr> {
        let mut left = self.additive()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinaryOp::Lt,
                Token::LtEq => BinaryOp::LtEq,
                Token::Gt => BinaryOp::Gt,
                Token::GtEq => BinaryOp::GtEq,
                _ => break,
            };
            self.pos += 1;
            let right = self.additive()?;
            left = Self::binary(op, left, right);
        }
        Ok(left)
    }

    fn additive(&mut self) -> MochaResult<Expr> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.multiplicative()?;
            left = Self::binary(op, left, right);
        }
        Ok(left)
    }

    fn multiplicative(&mut self) -> MochaResult<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Rem,
                _ => break,
            };
            self.pos += 1;
            let right = self.unary()?;
            left = Self::binary(op, left, right);
        }
        Ok(left)
    }

    fn unary(&mut self) -> MochaResult<Expr> {
        let op = match self.peek() {
            Token::Bang => Some(UnaryOp::Not),
            Token::Minus => Some(UnaryOp::Negate),
            _ => None,
        };
        if let Some(op) = op {
            self.pos += 1;
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op,
                operand: Box::new(operand),
            });
        }
        self.call()
    }

    fn call(&mut self) -> MochaResult<Expr> {
        let mut expr = self.primary()?;
        loop {
            match self.peek() {
                Token::LParen => {
                    self.pos += 1;
                    let args = self.argument_list()?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                    };
                }
                Token::Dot => {
                    self.pos += 1;
                    let property = self.identifier_name("after '.'")?;
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property,
                    };
                }
                Token::LBracket => {
                    self.pos += 1;
                    let index = self.expression()?;
                    self.eat(&Token::RBracket, "after index expression")?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn argument_list(&mut self) -> MochaResult<Vec<Expr>> {
        let mut args = Vec::new();
        while !self.check(&Token::RParen) {
            args.push(self.expression()?);
            if !self.check(&Token::RParen) {
                self.eat(&Token::Comma, "between arguments")?;
            }
        }
        self.eat(&Token::RParen, "after arguments")?;
        Ok(args)
    }

    fn primary(&mut self) -> MochaResult<Expr> {
        match self.advance() {
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::Str(s) => Ok(Expr::Str(s)),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Null => Ok(Expr::Null),
            Token::Undefined => Ok(Expr::Undefined),
            Token::Ident(name) => Ok(Expr::Identifier(name)),
            Token::Function => self.function_expression(),
            Token::LParen => {
                let expr = self.expression()?;
                self.eat(&Token::RParen, "after parenthesized expression")?;
                Ok(expr)
            }
            Token::LBracket => self.array_literal(),
            Token::LBrace => self.object_literal(),
            other => Err(MochaError::Parse(format!(
                "unexpected token in expression: {other:?}"
            ))),
        }
    }

    fn function_expression(&mut self) -> MochaResult<Expr> {
        let name = if let Token::Ident(name) = self.peek().clone() {
            self.pos += 1;
            Some(name)
        } else {
            None
        };
        let params = self.parameter_list()?;
        let body = self.block()?;
        Ok(Expr::Function { name, params, body })
    }

    fn array_literal(&mut self) -> MochaResult<Expr> {
        let mut elements = Vec::new();
        while !self.check(&Token::RBracket) {
            elements.push(self.expression()?);
            if !self.check(&Token::RBracket) {
                self.eat(&Token::Comma, "between array elements")?;
            }
        }
        self.eat(&Token::RBracket, "to close an array literal")?;
        Ok(Expr::Array(elements))
    }

    fn object_literal(&mut self) -> MochaResult<Expr> {
        let mut entries = Vec::new();
        while !self.check(&Token::RBrace) {
            let key = match self.advance() {
                Token::Ident(name) => name,
                Token::Str(s) => s,
                other => {
                    return Err(MochaError::Parse(format!(
                        "expected an object key, found {other:?}"
                    )))
                }
            };
            self.eat(&Token::Colon, "after object key")?;
            let value = self.expression()?;
            entries.push((key, value));
            if !self.check(&Token::RBrace) {
                self.eat(&Token::Comma, "between object entries")?;
            }
        }
        self.eat(&Token::RBrace, "to close an object literal")?;
        Ok(Expr::Object(entries))
    }

    fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
        Expr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Program {
        parse(source).unwrap()
    }

    #[test]
    fn parse_variable_declaration() {
        let program = parse_ok("let x = 1;");
        assert!(matches!(
            program.body[0],
            Stmt::VariableDeclaration {
                kind: DeclKind::Let,
                ..
            }
        ));
    }

    #[test]
    fn parse_function_declaration() {
        let program = parse_ok("function add(a, b) { return a + b; }");
        match &program.body[0] {
            Stmt::FunctionDeclaration { name, params, .. } => {
                assert_eq!(name, "add");
                assert_eq!(params, &["a".to_string(), "b".to_string()]);
            }
            other => panic!("expected function declaration, got {other:?}"),
        }
    }

    #[test]
    fn parse_if_while_for() {
        assert!(parse("if (x) { y; } else { z; }").is_ok());
        assert!(parse("while (x) { y; }").is_ok());
        assert!(parse("for (let i = 0; i < 3; i = i + 1) { y; }").is_ok());
    }

    #[test]
    fn parse_for_of_and_for_in() {
        match &parse_ok("for (let x of [1, 2]) { y; }").body[0] {
            Stmt::ForOf { name, .. } => assert_eq!(name, "x"),
            other => panic!("expected for-of, got {other:?}"),
        }
        match &parse_ok("for (const k in obj) { y; }").body[0] {
            Stmt::ForIn { name, .. } => assert_eq!(name, "k"),
            other => panic!("expected for-in, got {other:?}"),
        }
    }

    #[test]
    fn parse_call_and_member_and_index() {
        assert!(parse("a.b.c(1, 2)[3];").is_ok());
    }

    #[test]
    fn parse_object_and_array_literals() {
        assert!(parse("let o = { a: 1, b: 2 };").is_ok());
        assert!(parse("let a = [1, 2, 3];").is_ok());
    }

    #[test]
    fn operator_precedence_groups_multiplication_first() {
        // 1 + 2 * 3 parses as 1 + (2 * 3).
        let program = parse_ok("1 + 2 * 3;");
        match &program.body[0] {
            Stmt::Expression(Expr::Binary {
                op: BinaryOp::Add,
                right,
                ..
            }) => {
                assert!(matches!(
                    **right,
                    Expr::Binary {
                        op: BinaryOp::Mul,
                        ..
                    }
                ));
            }
            other => panic!("expected addition at top, got {other:?}"),
        }
    }

    #[test]
    fn malformed_code_errors_clearly() {
        assert!(matches!(parse("let = 5;"), Err(MochaError::Parse(_))));
        assert!(matches!(parse("function () {"), Err(MochaError::Parse(_))));
        assert!(matches!(parse("(1 + )"), Err(MochaError::Parse(_))));
    }
}
