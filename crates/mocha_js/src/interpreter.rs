//! The tree-walking interpreter.
//!
//! Evaluates a [`Program`] over a scope chain, with closures, a small set of
//! built-ins, documented type coercion, and an execution step budget that turns
//! runaway loops into a clear [`MochaError::JavaScript`] error instead of hanging.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mocha_error::{MochaError, MochaResult};

use crate::ast::{BinaryOp, DeclKind, Expr, LogicalOp, Program, Stmt, UnaryOp};
use crate::builtins;
use crate::environment::{AssignOutcome, Environment};
use crate::value::{Function, JsValue};

/// Default maximum number of evaluation steps before aborting.
pub const DEFAULT_STEP_LIMIT: usize = 100_000;

/// Control-flow result of executing a statement.
enum Flow {
    /// Normal completion carrying the statement's value (for expression
    /// statements; `Undefined` otherwise).
    Normal(JsValue),
    /// A `return` is unwinding with this value.
    Return(JsValue),
}

/// The interpreter: holds the global scope, captured console output, and the
/// step budget.
pub struct Interpreter {
    global: Rc<RefCell<Environment>>,
    pub(crate) console: Vec<String>,
    steps: usize,
    step_limit: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Interpreter::new()
    }
}

impl Interpreter {
    /// Create an interpreter with built-ins installed in the global scope.
    pub fn new() -> Interpreter {
        let global = Environment::global();
        builtins::install(&global);
        Interpreter {
            global,
            console: Vec::new(),
            steps: 0,
            step_limit: DEFAULT_STEP_LIMIT,
        }
    }

    /// Take (and clear) the captured `console.log` output.
    pub fn take_console_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.console)
    }

    /// Define (or replace) a mutable binding in the global scope. Host crates use
    /// this to install globals such as `window`/`document` before running scripts.
    pub fn define_global(&mut self, name: &str, value: JsValue) {
        self.global
            .borrow_mut()
            .define(name.to_string(), value, true);
    }

    /// Read a global binding by name (e.g. to expose `console` as `window.console`).
    pub fn global_get(&self, name: &str) -> Option<JsValue> {
        self.global.borrow().get(name)
    }

    /// Call a callable [`JsValue`] (user or native function) with `args`. Host
    /// objects use this to invoke JavaScript callbacks (event listeners, timers).
    pub fn call_function(&mut self, callee: JsValue, args: Vec<JsValue>) -> MochaResult<JsValue> {
        self.call_value(callee, args)
    }

    /// Append a line to the captured console output (as `console.log` does). Host
    /// objects use this to surface diagnostics through the same channel.
    pub fn record_console(&mut self, line: String) {
        self.console.push(line);
    }

    /// Run a program, returning the value of its last expression statement (or
    /// `undefined`).
    pub fn run(&mut self, program: &Program) -> MochaResult<JsValue> {
        let env = self.global.clone();
        match self.exec_block(&program.body, &env)? {
            Flow::Return(value) | Flow::Normal(value) => Ok(value),
        }
    }

    fn tick(&mut self) -> MochaResult<()> {
        self.steps += 1;
        if self.steps > self.step_limit {
            return Err(MochaError::JavaScript(
                "execution step limit exceeded".to_string(),
            ));
        }
        Ok(())
    }

    /// Define all function declarations in `stmts` (a minimal hoist enabling
    /// forward references and recursion).
    fn hoist(&self, stmts: &[Stmt], env: &Rc<RefCell<Environment>>) {
        for stmt in stmts {
            if let Stmt::FunctionDeclaration { name, params, body } = stmt {
                let function = JsValue::Function(Rc::new(Function::User {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: body.clone(),
                    closure: env.clone(),
                }));
                env.borrow_mut().define(name.clone(), function, true);
            }
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt], env: &Rc<RefCell<Environment>>) -> MochaResult<Flow> {
        self.hoist(stmts, env);
        let mut last = JsValue::Undefined;
        for stmt in stmts {
            match self.exec(stmt, env)? {
                Flow::Return(value) => return Ok(Flow::Return(value)),
                Flow::Normal(value) => last = value,
            }
        }
        Ok(Flow::Normal(last))
    }

    fn exec(&mut self, stmt: &Stmt, env: &Rc<RefCell<Environment>>) -> MochaResult<Flow> {
        self.tick()?;
        match stmt {
            Stmt::VariableDeclaration { kind, name, init } => {
                let value = match init {
                    Some(expr) => self.eval(expr, env)?,
                    None => JsValue::Undefined,
                };
                let mutable = *kind != DeclKind::Const;
                env.borrow_mut().define(name.clone(), value, mutable);
                Ok(Flow::Normal(JsValue::Undefined))
            }
            Stmt::FunctionDeclaration { name, params, body } => {
                let function = JsValue::Function(Rc::new(Function::User {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: body.clone(),
                    closure: env.clone(),
                }));
                env.borrow_mut().define(name.clone(), function, true);
                Ok(Flow::Normal(JsValue::Undefined))
            }
            Stmt::Return(expr) => {
                let value = match expr {
                    Some(expr) => self.eval(expr, env)?,
                    None => JsValue::Undefined,
                };
                Ok(Flow::Return(value))
            }
            Stmt::Expression(expr) => Ok(Flow::Normal(self.eval(expr, env)?)),
            Stmt::Block(stmts) => {
                let child = Environment::child(env.clone());
                self.exec_block(stmts, &child)
            }
            Stmt::If {
                test,
                consequent,
                alternate,
            } => {
                if self.eval(test, env)?.is_truthy() {
                    self.exec(consequent, env)
                } else if let Some(alternate) = alternate {
                    self.exec(alternate, env)
                } else {
                    Ok(Flow::Normal(JsValue::Undefined))
                }
            }
            Stmt::While { test, body } => {
                while self.eval(test, env)?.is_truthy() {
                    self.tick()?;
                    if let Flow::Return(value) = self.exec(body, env)? {
                        return Ok(Flow::Return(value));
                    }
                }
                Ok(Flow::Normal(JsValue::Undefined))
            }
            Stmt::For {
                init,
                test,
                update,
                body,
            } => {
                let child = Environment::child(env.clone());
                if let Some(init) = init {
                    self.exec(init, &child)?;
                }
                loop {
                    let proceed = match test {
                        Some(test) => self.eval(test, &child)?.is_truthy(),
                        None => true,
                    };
                    if !proceed {
                        break;
                    }
                    self.tick()?;
                    if let Flow::Return(value) = self.exec(body, &child)? {
                        return Ok(Flow::Return(value));
                    }
                    if let Some(update) = update {
                        self.eval(update, &child)?;
                    }
                }
                Ok(Flow::Normal(JsValue::Undefined))
            }
        }
    }

    fn eval(&mut self, expr: &Expr, env: &Rc<RefCell<Environment>>) -> MochaResult<JsValue> {
        self.tick()?;
        match expr {
            Expr::Number(n) => Ok(JsValue::Number(*n)),
            Expr::Str(s) => Ok(JsValue::Str(s.clone())),
            Expr::Bool(b) => Ok(JsValue::Bool(*b)),
            Expr::Null => Ok(JsValue::Null),
            Expr::Undefined => Ok(JsValue::Undefined),
            Expr::Identifier(name) => env
                .borrow()
                .get(name)
                .ok_or_else(|| MochaError::JavaScript(format!("undefined variable: {name}"))),
            Expr::Assignment { target, value } => {
                let value = self.eval(value, env)?;
                self.assign_target(target, value.clone(), env)?;
                Ok(value)
            }
            Expr::Binary { op, left, right } => {
                let left = self.eval(left, env)?;
                let right = self.eval(right, env)?;
                self.binary(*op, left, right)
            }
            Expr::Logical { op, left, right } => {
                let left = self.eval(left, env)?;
                match op {
                    LogicalOp::And if !left.is_truthy() => Ok(left),
                    LogicalOp::Or if left.is_truthy() => Ok(left),
                    _ => self.eval(right, env),
                }
            }
            Expr::Unary { op, operand } => {
                let value = self.eval(operand, env)?;
                match op {
                    UnaryOp::Negate => Ok(JsValue::Number(-value.to_number())),
                    UnaryOp::Not => Ok(JsValue::Bool(!value.is_truthy())),
                }
            }
            Expr::Call { callee, args } => self.eval_call(callee, args, env),
            Expr::Member { object, property } => {
                let object = self.eval(object, env)?;
                self.get_member(&object, property)
            }
            Expr::Index { object, index } => {
                let object = self.eval(object, env)?;
                let index = self.eval(index, env)?;
                self.get_index(&object, &index)
            }
            Expr::Object(entries) => {
                let mut map = HashMap::new();
                for (key, value_expr) in entries {
                    let value = self.eval(value_expr, env)?;
                    map.insert(key.clone(), value);
                }
                Ok(JsValue::object(map))
            }
            Expr::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, env)?);
                }
                Ok(JsValue::array(values))
            }
            Expr::Function { name, params, body } => {
                Ok(JsValue::Function(Rc::new(Function::User {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    closure: env.clone(),
                })))
            }
        }
    }

    fn assign_target(
        &mut self,
        target: &Expr,
        value: JsValue,
        env: &Rc<RefCell<Environment>>,
    ) -> MochaResult<()> {
        match target {
            Expr::Identifier(name) => match env.borrow_mut().assign(name, value) {
                AssignOutcome::Updated => Ok(()),
                AssignOutcome::NotDefined => Err(MochaError::JavaScript(format!(
                    "assignment to undefined variable: {name}"
                ))),
                AssignOutcome::Constant => Err(MochaError::JavaScript(format!(
                    "assignment to constant variable: {name}"
                ))),
            },
            Expr::Member { object, property } => {
                let object = self.eval(object, env)?;
                match object {
                    JsValue::Host(host) => host.set(self, property, value),
                    JsValue::Object(map) => {
                        map.borrow_mut().insert(property.clone(), value);
                        Ok(())
                    }
                    other => Err(MochaError::JavaScript(format!(
                        "cannot set property '{property}' on {}",
                        other.type_name()
                    ))),
                }
            }
            Expr::Index { object, index } => {
                let object = self.eval(object, env)?;
                let index = self.eval(index, env)?;
                self.set_index(&object, &index, value)
            }
            _ => Err(MochaError::JavaScript(
                "invalid assignment target".to_string(),
            )),
        }
    }

    fn binary(&self, op: BinaryOp, left: JsValue, right: JsValue) -> MochaResult<JsValue> {
        let result = match op {
            BinaryOp::Add => {
                if matches!(left, JsValue::Str(_)) || matches!(right, JsValue::Str(_)) {
                    JsValue::Str(format!("{}{}", left.stringify(), right.stringify()))
                } else {
                    JsValue::Number(left.to_number() + right.to_number())
                }
            }
            BinaryOp::Sub => JsValue::Number(left.to_number() - right.to_number()),
            BinaryOp::Mul => JsValue::Number(left.to_number() * right.to_number()),
            BinaryOp::Div => JsValue::Number(left.to_number() / right.to_number()),
            BinaryOp::Rem => JsValue::Number(left.to_number() % right.to_number()),
            BinaryOp::Eq | BinaryOp::StrictEq => JsValue::Bool(left.strict_equals(&right)),
            BinaryOp::NotEq | BinaryOp::StrictNotEq => JsValue::Bool(!left.strict_equals(&right)),
            BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                let ordering = compare(&left, &right);
                JsValue::Bool(match op {
                    BinaryOp::Lt => ordering == Some(std::cmp::Ordering::Less),
                    BinaryOp::LtEq => matches!(
                        ordering,
                        Some(std::cmp::Ordering::Less) | Some(std::cmp::Ordering::Equal)
                    ),
                    BinaryOp::Gt => ordering == Some(std::cmp::Ordering::Greater),
                    BinaryOp::GtEq => matches!(
                        ordering,
                        Some(std::cmp::Ordering::Greater) | Some(std::cmp::Ordering::Equal)
                    ),
                    _ => unreachable!(),
                })
            }
        };
        Ok(result)
    }

    fn eval_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        env: &Rc<RefCell<Environment>>,
    ) -> MochaResult<JsValue> {
        let mut arg_values = Vec::with_capacity(args.len());
        for arg in args {
            arg_values.push(self.eval(arg, env)?);
        }

        // Method calls (`object.method(...)`) need the object value.
        if let Expr::Member { object, property } = callee {
            let object = self.eval(object, env)?;
            return self.call_member(object, property, arg_values);
        }

        let callee = self.eval(callee, env)?;
        self.call_value(callee, arg_values)
    }

    fn call_member(
        &mut self,
        object: JsValue,
        property: &str,
        args: Vec<JsValue>,
    ) -> MochaResult<JsValue> {
        // Host objects route method calls to their `call` implementation.
        if let JsValue::Host(host) = &object {
            let host = host.clone();
            return host.call(self, property, args);
        }
        // A few built-in array methods that need the receiving array.
        if let JsValue::Array(array) = &object {
            match property {
                "push" => {
                    let mut borrowed = array.borrow_mut();
                    for arg in args {
                        borrowed.push(arg);
                    }
                    return Ok(JsValue::Number(borrowed.len() as f64));
                }
                "pop" => {
                    return Ok(array.borrow_mut().pop().unwrap_or(JsValue::Undefined));
                }
                _ => {}
            }
        }
        let function = self.get_member(&object, property)?;
        self.call_value(function, args)
    }

    fn call_value(&mut self, callee: JsValue, args: Vec<JsValue>) -> MochaResult<JsValue> {
        let JsValue::Function(function) = callee else {
            return Err(MochaError::JavaScript(format!(
                "cannot call non-function value of type {}",
                callee.type_name()
            )));
        };
        match &*function {
            Function::Native { func, .. } => func(self, &args),
            Function::NativeClosure { func, .. } => func(self, &args),
            Function::User {
                params,
                body,
                closure,
                ..
            } => {
                let call_env = Environment::child(closure.clone());
                for (index, param) in params.iter().enumerate() {
                    let value = args.get(index).cloned().unwrap_or(JsValue::Undefined);
                    call_env.borrow_mut().define(param.clone(), value, true);
                }
                match self.exec_block(body, &call_env)? {
                    Flow::Return(value) => Ok(value),
                    Flow::Normal(_) => Ok(JsValue::Undefined),
                }
            }
        }
    }

    fn get_member(&mut self, object: &JsValue, property: &str) -> MochaResult<JsValue> {
        match object {
            JsValue::Host(host) => {
                let host = host.clone();
                host.get(self, property)
            }
            JsValue::Object(map) => Ok(map
                .borrow()
                .get(property)
                .cloned()
                .unwrap_or(JsValue::Undefined)),
            JsValue::Array(array) => {
                if property == "length" {
                    Ok(JsValue::Number(array.borrow().len() as f64))
                } else {
                    Ok(JsValue::Undefined)
                }
            }
            JsValue::Str(s) => {
                if property == "length" {
                    Ok(JsValue::Number(s.chars().count() as f64))
                } else {
                    Ok(JsValue::Undefined)
                }
            }
            JsValue::Null | JsValue::Undefined => Err(MochaError::JavaScript(format!(
                "cannot read property '{property}' of {}",
                object.type_name()
            ))),
            _ => Ok(JsValue::Undefined),
        }
    }

    fn get_index(&mut self, object: &JsValue, index: &JsValue) -> MochaResult<JsValue> {
        match object {
            JsValue::Host(host) => {
                let host = host.clone();
                host.get(self, &index.stringify())
            }
            JsValue::Array(array) => {
                let i = index.to_number();
                if i >= 0.0 && i.fract() == 0.0 {
                    Ok(array
                        .borrow()
                        .get(i as usize)
                        .cloned()
                        .unwrap_or(JsValue::Undefined))
                } else {
                    Ok(JsValue::Undefined)
                }
            }
            JsValue::Object(map) => Ok(map
                .borrow()
                .get(&index.stringify())
                .cloned()
                .unwrap_or(JsValue::Undefined)),
            JsValue::Null | JsValue::Undefined => Err(MochaError::JavaScript(format!(
                "cannot index {}",
                object.type_name()
            ))),
            _ => Ok(JsValue::Undefined),
        }
    }

    fn set_index(&mut self, object: &JsValue, index: &JsValue, value: JsValue) -> MochaResult<()> {
        match object {
            JsValue::Host(host) => {
                let host = host.clone();
                host.set(self, &index.stringify(), value)
            }
            JsValue::Array(array) => {
                let i = index.to_number();
                if i < 0.0 || i.fract() != 0.0 {
                    return Err(MochaError::JavaScript("invalid array index".to_string()));
                }
                let idx = i as usize;
                let mut borrowed = array.borrow_mut();
                if idx < borrowed.len() {
                    borrowed[idx] = value;
                } else if idx == borrowed.len() {
                    borrowed.push(value);
                } else {
                    return Err(MochaError::JavaScript(
                        "array index out of bounds".to_string(),
                    ));
                }
                Ok(())
            }
            JsValue::Object(map) => {
                map.borrow_mut().insert(index.stringify(), value);
                Ok(())
            }
            other => Err(MochaError::JavaScript(format!(
                "cannot set an index on {}",
                other.type_name()
            ))),
        }
    }
}

/// Compare two values for ordering: strings lexicographically, otherwise by
/// numeric coercion. Returns `None` when a number coercion yields `NaN`.
fn compare(left: &JsValue, right: &JsValue) -> Option<std::cmp::Ordering> {
    if let (JsValue::Str(a), JsValue::Str(b)) = (left, right) {
        return Some(a.cmp(b));
    }
    left.to_number().partial_cmp(&right.to_number())
}
