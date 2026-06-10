//! Host objects: a bridge for embedding native (Rust) objects into the interpreter.
//!
//! A [`HostObject`] is exposed to JavaScript as a [`JsValue::Host`]. Property
//! reads/writes and method calls on it are routed to the trait methods, which
//! carry their own shared mutable state (typically behind `Rc<RefCell<…>>`), so a
//! single host can be aliased freely by script. This is the mechanism a later
//! crate uses to back `window`/`document`/DOM nodes with the real DOM — without
//! any string matching, faked commands, or a second engine.

use std::any::Any;

use mocha_error::MochaResult;

use crate::interpreter::Interpreter;
use crate::value::JsValue;

/// A native object embedded into the interpreter as a [`JsValue::Host`].
///
/// Implementors carry their own interior-mutable state, so the trait methods
/// take `&self`. Identity (`===`) is pointer identity on the backing `Rc`.
pub trait HostObject {
    /// A class name for diagnostics and `stringify` (renders as `[object NAME]`).
    fn class_name(&self) -> &str;

    /// Downcast support, so a host method can recover a concrete host type from a
    /// [`JsValue::Host`] argument (e.g. `appendChild(node)` recovering its node).
    fn as_any(&self) -> &dyn Any;

    /// Read property `name`. Unknown properties should return
    /// [`JsValue::Undefined`], matching JavaScript object semantics.
    fn get(&self, interpreter: &mut Interpreter, name: &str) -> MochaResult<JsValue>;

    /// Write `value` to property `name`.
    fn set(&self, interpreter: &mut Interpreter, name: &str, value: JsValue) -> MochaResult<()>;

    /// Invoke method `name` with `args`.
    fn call(
        &self,
        interpreter: &mut Interpreter,
        name: &str,
        args: Vec<JsValue>,
    ) -> MochaResult<JsValue>;
}
