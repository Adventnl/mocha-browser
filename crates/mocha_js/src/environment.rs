//! Lexical environments (scopes) with a parent chain.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::JsValue;

/// The outcome of an [`Environment::assign`] attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOutcome {
    /// The binding was updated.
    Updated,
    /// No such binding exists in the chain.
    NotDefined,
    /// The binding exists but is `const`.
    Constant,
}

struct Binding {
    value: JsValue,
    mutable: bool,
}

/// A scope: a set of bindings plus an optional parent scope.
#[derive(Default)]
pub struct Environment {
    bindings: HashMap<String, Binding>,
    parent: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    /// Create a new global (parentless) environment.
    pub fn global() -> Rc<RefCell<Environment>> {
        Rc::new(RefCell::new(Environment::default()))
    }

    /// Create a child environment of `parent`.
    pub fn child(parent: Rc<RefCell<Environment>>) -> Rc<RefCell<Environment>> {
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent: Some(parent),
        }))
    }

    /// Define (or redefine) a binding in this scope.
    pub fn define(&mut self, name: impl Into<String>, value: JsValue, mutable: bool) {
        self.bindings
            .insert(name.into(), Binding { value, mutable });
    }

    /// Look a name up through the scope chain.
    pub fn get(&self, name: &str) -> Option<JsValue> {
        if let Some(binding) = self.bindings.get(name) {
            return Some(binding.value.clone());
        }
        match &self.parent {
            Some(parent) => parent.borrow().get(name),
            None => None,
        }
    }

    /// Assign to an existing binding through the chain.
    pub fn assign(&mut self, name: &str, value: JsValue) -> AssignOutcome {
        if let Some(binding) = self.bindings.get_mut(name) {
            if !binding.mutable {
                return AssignOutcome::Constant;
            }
            binding.value = value;
            return AssignOutcome::Updated;
        }
        match &self.parent {
            Some(parent) => parent.borrow_mut().assign(name, value),
            None => AssignOutcome::NotDefined,
        }
    }
}
