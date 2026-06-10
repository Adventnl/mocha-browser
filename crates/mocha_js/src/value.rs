//! Runtime values for the interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::ast::Stmt;
use crate::environment::Environment;
use crate::host::HostObject;
use crate::interpreter::Interpreter;
use mocha_error::MochaResult;

/// A native (Rust-implemented) built-in function.
pub type NativeFn = fn(&mut Interpreter, &[JsValue]) -> MochaResult<JsValue>;

/// A native built-in function backed by a state-capturing closure.
pub type NativeClosureFn = Rc<dyn Fn(&mut Interpreter, &[JsValue]) -> MochaResult<JsValue>>;

/// A callable function value.
pub enum Function {
    /// A user-defined function with a captured (closure) environment.
    User {
        /// Optional name (for named function expressions / declarations).
        name: Option<String>,
        /// Parameter names.
        params: Vec<String>,
        /// Body statements.
        body: Vec<Stmt>,
        /// The environment captured at definition time.
        closure: Rc<RefCell<Environment>>,
    },
    /// A built-in function.
    Native {
        /// Display name.
        name: String,
        /// The implementation.
        func: NativeFn,
    },
    /// A built-in function backed by a closure that can capture host state (used
    /// by DOM bindings for globals like `setTimeout` that need shared state).
    NativeClosure {
        /// Display name.
        name: String,
        /// The implementation.
        func: NativeClosureFn,
    },
}

/// A JavaScript runtime value.
#[derive(Clone)]
pub enum JsValue {
    /// A 64-bit float (all JS numbers).
    Number(f64),
    /// A string.
    Str(String),
    /// A boolean.
    Bool(bool),
    /// `null`.
    Null,
    /// `undefined`.
    Undefined,
    /// An object with string keys.
    Object(Rc<RefCell<HashMap<String, JsValue>>>),
    /// An array.
    Array(Rc<RefCell<Vec<JsValue>>>),
    /// A function.
    Function(Rc<Function>),
    /// A native host object (see [`HostObject`]). Used to back DOM bindings.
    Host(Rc<dyn HostObject>),
}

impl JsValue {
    /// An empty object value.
    pub fn object(map: HashMap<String, JsValue>) -> JsValue {
        JsValue::Object(Rc::new(RefCell::new(map)))
    }

    /// An array value.
    pub fn array(items: Vec<JsValue>) -> JsValue {
        JsValue::Array(Rc::new(RefCell::new(items)))
    }

    /// A native function backed by a state-capturing closure. Host crates use this
    /// to install global functions (e.g. `setTimeout`) that need shared state.
    pub fn native_closure<F>(name: impl Into<String>, func: F) -> JsValue
    where
        F: Fn(&mut Interpreter, &[JsValue]) -> MochaResult<JsValue> + 'static,
    {
        JsValue::Function(Rc::new(Function::NativeClosure {
            name: name.into(),
            func: Rc::new(func),
        }))
    }

    /// JavaScript truthiness: `false`, `null`, `undefined`, `0`, `NaN`, and `""`
    /// are falsy; everything else is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            JsValue::Bool(b) => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::Str(s) => !s.is_empty(),
            JsValue::Null | JsValue::Undefined => false,
            JsValue::Object(_) | JsValue::Array(_) | JsValue::Function(_) | JsValue::Host(_) => {
                true
            }
        }
    }

    /// A short type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            JsValue::Number(_) => "number",
            JsValue::Str(_) => "string",
            JsValue::Bool(_) => "boolean",
            JsValue::Null => "null",
            JsValue::Undefined => "undefined",
            JsValue::Object(_) => "object",
            JsValue::Array(_) => "array",
            JsValue::Function(_) => "function",
            JsValue::Host(_) => "object",
        }
    }

    /// Coerce to a number for arithmetic (a small, documented subset).
    pub fn to_number(&self) -> f64 {
        match self {
            JsValue::Number(n) => *n,
            JsValue::Bool(true) => 1.0,
            JsValue::Bool(false) => 0.0,
            JsValue::Null => 0.0,
            JsValue::Undefined => f64::NAN,
            JsValue::Str(s) => s.trim().parse().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }

    /// Convert to a display string (as `console.log`/results print it).
    pub fn stringify(&self) -> String {
        match self {
            JsValue::Number(n) => number_to_string(*n),
            JsValue::Str(s) => s.clone(),
            JsValue::Bool(b) => b.to_string(),
            JsValue::Null => "null".to_string(),
            JsValue::Undefined => "undefined".to_string(),
            JsValue::Array(items) => items
                .borrow()
                .iter()
                .map(|v| v.stringify())
                .collect::<Vec<_>>()
                .join(","),
            JsValue::Object(_) => "[object Object]".to_string(),
            JsValue::Function(_) => "function".to_string(),
            JsValue::Host(host) => format!("[object {}]", host.class_name()),
        }
    }

    /// Strict-equality (`===`). Objects/arrays/functions compare by identity.
    pub fn strict_equals(&self, other: &JsValue) -> bool {
        match (self, other) {
            (JsValue::Number(a), JsValue::Number(b)) => a == b,
            (JsValue::Str(a), JsValue::Str(b)) => a == b,
            (JsValue::Bool(a), JsValue::Bool(b)) => a == b,
            (JsValue::Null, JsValue::Null) => true,
            (JsValue::Undefined, JsValue::Undefined) => true,
            (JsValue::Object(a), JsValue::Object(b)) => Rc::ptr_eq(a, b),
            (JsValue::Array(a), JsValue::Array(b)) => Rc::ptr_eq(a, b),
            (JsValue::Function(a), JsValue::Function(b)) => Rc::ptr_eq(a, b),
            (JsValue::Host(a), JsValue::Host(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Debug for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.type_name(), self.stringify())
    }
}

/// Format a JS number the way `console.log` would: integers without a decimal
/// point, plus `NaN`/`Infinity` spellings.
pub fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.is_infinite() {
        if n > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        }
    } else if n == 0.0 {
        "0".to_string() // also normalises -0
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
