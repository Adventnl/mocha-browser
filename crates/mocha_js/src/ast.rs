//! The abstract syntax tree for the JavaScript subset.

/// A parsed program: a sequence of statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Top-level statements.
    pub body: Vec<Stmt>,
}

/// How a variable was declared (affects mutability).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    /// `let` — mutable, block-scoped (block scoping simplified).
    Let,
    /// `const` — immutable binding.
    Const,
    /// `var` — mutable (treated like `let` here).
    Var,
}

/// A statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let`/`const`/`var name = init;` (init optional).
    VariableDeclaration {
        /// Declaration keyword.
        kind: DeclKind,
        /// Variable name.
        name: String,
        /// Optional initializer.
        init: Option<Expr>,
    },
    /// `function name(params) { body }`.
    FunctionDeclaration {
        /// Function name.
        name: String,
        /// Parameter names.
        params: Vec<String>,
        /// Body statements.
        body: Vec<Stmt>,
    },
    /// `return expr;` (expr optional).
    Return(Option<Expr>),
    /// An expression used as a statement.
    Expression(Expr),
    /// A `{ ... }` block.
    Block(Vec<Stmt>),
    /// `if (test) consequent else alternate`.
    If {
        /// Condition.
        test: Expr,
        /// Then branch.
        consequent: Box<Stmt>,
        /// Optional else branch.
        alternate: Option<Box<Stmt>>,
    },
    /// `while (test) body`.
    While {
        /// Condition.
        test: Expr,
        /// Loop body.
        body: Box<Stmt>,
    },
    /// `for (init; test; update) body`.
    For {
        /// Initializer statement.
        init: Option<Box<Stmt>>,
        /// Condition.
        test: Option<Expr>,
        /// Update expression.
        update: Option<Expr>,
        /// Loop body.
        body: Box<Stmt>,
    },
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
    /// `==`
    Eq,
    /// `!=`
    NotEq,
    /// `===`
    StrictEq,
    /// `!==`
    StrictNotEq,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
}

/// A logical operator (short-circuiting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    /// `&&`
    And,
    /// `||`
    Or,
}

/// A unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-` (negation)
    Negate,
    /// `!` (logical not)
    Not,
}

/// An expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A number literal.
    Number(f64),
    /// A string literal.
    Str(String),
    /// A boolean literal.
    Bool(bool),
    /// `null`.
    Null,
    /// `undefined`.
    Undefined,
    /// A variable reference.
    Identifier(String),
    /// `target = value` (target is an identifier, member, or index).
    Assignment {
        /// Assignment target.
        target: Box<Expr>,
        /// Value expression.
        value: Box<Expr>,
    },
    /// A binary operation.
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// A short-circuiting logical operation.
    Logical {
        /// Operator.
        op: LogicalOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// A unary operation.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        operand: Box<Expr>,
    },
    /// A function call `callee(args)`.
    Call {
        /// The function expression.
        callee: Box<Expr>,
        /// Argument expressions.
        args: Vec<Expr>,
    },
    /// Member access `object.property`.
    Member {
        /// The object.
        object: Box<Expr>,
        /// The property name.
        property: String,
    },
    /// Computed index `object[index]`.
    Index {
        /// The object.
        object: Box<Expr>,
        /// The index expression.
        index: Box<Expr>,
    },
    /// An object literal `{ k: v, ... }`.
    Object(Vec<(String, Expr)>),
    /// An array literal `[a, b, ...]`.
    Array(Vec<Expr>),
    /// A function expression `function (params) { body }`.
    Function {
        /// Optional name.
        name: Option<String>,
        /// Parameter names.
        params: Vec<String>,
        /// Body statements.
        body: Vec<Stmt>,
    },
}
