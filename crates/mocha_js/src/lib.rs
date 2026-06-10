//! A small from-scratch JavaScript-subset interpreter for Mocha Browser.
//!
//! Pipeline: source → [`lex`] → [`parse`] → AST → [`Interpreter`]. This is **not**
//! ECMAScript-compliant and has **no DOM bindings** — it evaluates standalone
//! snippets. No existing JavaScript engine or parser is used.
//!
//! See `docs/architecture/javascript-interpreter.md` for the supported subset and
//! known non-compliance.

mod ast;
mod builtins;
mod environment;
mod host;
mod interpreter;
mod lexer;
mod parser;
mod token;
mod value;

pub use ast::Program;
pub use host::HostObject;
pub use interpreter::{Interpreter, DEFAULT_STEP_LIMIT};
pub use lexer::lex;
pub use parser::parse;
pub use token::Token;
pub use value::JsValue;

use mocha_error::MochaResult;

/// A convenient front-end: evaluate JavaScript source and capture console output.
#[derive(Default)]
pub struct JsRuntime {
    interpreter: Interpreter,
}

impl JsRuntime {
    /// Create a runtime with built-ins installed.
    pub fn new() -> JsRuntime {
        JsRuntime {
            interpreter: Interpreter::new(),
        }
    }

    /// Parse and evaluate `source`, returning the value of its last expression
    /// statement (or `undefined`).
    pub fn eval(&mut self, source: &str) -> MochaResult<JsValue> {
        let program = parse(source)?;
        self.interpreter.run(&program)
    }

    /// Take (and clear) the `console.log` output captured so far.
    pub fn take_console_output(&mut self) -> Vec<String> {
        self.interpreter.take_console_output()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_error::MochaError;

    fn eval(source: &str) -> JsValue {
        JsRuntime::new().eval(source).unwrap()
    }

    fn num(source: &str) -> f64 {
        match eval(source) {
            JsValue::Number(n) => n,
            other => panic!("expected number, got {}", other.stringify()),
        }
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(num("1 + 2 * 3;"), 7.0);
        assert_eq!(num("(1 + 2) * 3;"), 9.0);
        assert_eq!(num("10 % 3;"), 1.0);
    }

    #[test]
    fn string_concatenation() {
        assert_eq!(eval("\"hello \" + \"world\";").stringify(), "hello world");
        assert_eq!(eval("\"x\" + 1;").stringify(), "x1");
    }

    #[test]
    fn comparison_and_logical() {
        assert!(matches!(eval("1 < 2;"), JsValue::Bool(true)));
        assert!(matches!(eval("2 === 2;"), JsValue::Bool(true)));
        assert!(matches!(eval("1 === \"1\";"), JsValue::Bool(false)));
        assert_eq!(num("0 || 5;"), 5.0); // short-circuit returns operand
        assert_eq!(num("3 && 4;"), 4.0);
    }

    #[test]
    fn truthiness() {
        assert!(matches!(eval("!0;"), JsValue::Bool(true)));
        assert!(matches!(eval("!\"\";"), JsValue::Bool(true)));
        assert!(matches!(eval("!\"x\";"), JsValue::Bool(false)));
    }

    #[test]
    fn variables_and_assignment() {
        assert_eq!(num("let x = 1; x = x + 2; x;"), 3.0);
    }

    #[test]
    fn const_assignment_errors() {
        let error = JsRuntime::new().eval("const x = 1; x = 2;").unwrap_err();
        assert!(matches!(error, MochaError::JavaScript(_)));
    }

    #[test]
    fn shadowing_in_block_scope() {
        // Inner block shadows; outer value is unchanged.
        assert_eq!(num("let x = 1; { let x = 5; } x;"), 1.0);
    }

    #[test]
    fn if_else_chooses_branch() {
        assert_eq!(
            num("let x = 5; let y; if (x > 3) { y = 1; } else { y = 0; } y;"),
            1.0
        );
    }

    #[test]
    fn while_loop_accumulates() {
        assert_eq!(
            num("let i = 0; let total = 0; while (i < 3) { total = total + i; i = i + 1; } total;"),
            3.0
        );
    }

    #[test]
    fn for_loop_accumulates() {
        assert_eq!(
            num("let total = 0; for (let i = 0; i < 4; i = i + 1) { total = total + i; } total;"),
            6.0
        );
    }

    #[test]
    fn function_call_and_return() {
        assert_eq!(num("function add(a, b) { return a + b; } add(2, 3);"), 5.0);
    }

    #[test]
    fn recursion_works() {
        assert_eq!(
            num("function fact(n) { if (n <= 1) { return 1; } return n * fact(n - 1); } fact(5);"),
            120.0
        );
    }

    #[test]
    fn closure_captures_outer_variable() {
        let source = "function makeAdder(x) { return function (y) { return x + y; }; } \
                      let add10 = makeAdder(10); add10(5);";
        assert_eq!(num(source), 15.0);
    }

    #[test]
    fn objects_get_and_set() {
        assert_eq!(num("let o = { a: 1 }; o.b = 2; o.a + o.b;"), 3.0);
        assert!(matches!(eval("let o = {}; o.missing;"), JsValue::Undefined));
    }

    #[test]
    fn arrays_index_length_push() {
        assert_eq!(num("let a = [1, 2, 3]; a[0] + a[2];"), 4.0);
        assert_eq!(num("let a = [1]; a[1] = 9; a[1];"), 9.0);
        assert_eq!(num("let a = [1, 2]; a.length;"), 2.0);
        assert_eq!(num("let a = [1]; a.push(2); a.push(3); a.length;"), 3.0);
    }

    #[test]
    fn string_length() {
        assert_eq!(num("\"hello\".length;"), 5.0);
    }

    #[test]
    fn console_log_is_captured() {
        let mut runtime = JsRuntime::new();
        runtime.eval("console.log(\"hello\", 123);").unwrap();
        assert_eq!(runtime.take_console_output(), vec!["hello 123".to_string()]);
    }

    #[test]
    fn math_builtins() {
        assert_eq!(num("Math.max(1, 7, 3);"), 7.0);
        assert_eq!(num("Math.min(1, 7, 3);"), 1.0);
        assert_eq!(num("Math.abs(0 - 4);"), 4.0);
        assert_eq!(num("Math.floor(3.9);"), 3.0);
    }

    #[test]
    fn undefined_variable_errors() {
        let error = JsRuntime::new().eval("missing;").unwrap_err();
        assert!(matches!(error, MochaError::JavaScript(_)));
    }

    #[test]
    fn calling_non_function_errors() {
        let error = JsRuntime::new().eval("let x = 5; x();").unwrap_err();
        assert!(matches!(error, MochaError::JavaScript(_)));
    }

    #[test]
    fn syntax_error_is_clear() {
        assert!(matches!(
            JsRuntime::new().eval("let = ;"),
            Err(MochaError::Parse(_))
        ));
    }

    #[test]
    fn step_limit_prevents_infinite_loop() {
        let error = JsRuntime::new().eval("while (true) { }").unwrap_err();
        match error {
            MochaError::JavaScript(message) => assert!(message.contains("step limit")),
            other => panic!("expected step-limit error, got {other:?}"),
        }
    }

    #[test]
    fn program_returns_last_expression_value() {
        assert_eq!(num("let a = 1; let b = 2; a + b;"), 3.0);
        assert!(matches!(eval("let a = 1;"), JsValue::Undefined));
    }

    // --- host objects -------------------------------------------------------

    use crate::Interpreter;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A tiny host object backing a single shared counter, exercising get/set/call
    /// and proving host state survives across property access and method calls.
    struct Counter {
        value: RefCell<f64>,
    }

    impl crate::HostObject for Counter {
        fn class_name(&self) -> &str {
            "Counter"
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn get(&self, _: &mut Interpreter, name: &str) -> MochaResult<JsValue> {
            match name {
                "value" => Ok(JsValue::Number(*self.value.borrow())),
                _ => Ok(JsValue::Undefined),
            }
        }
        fn set(&self, _: &mut Interpreter, name: &str, value: JsValue) -> MochaResult<()> {
            if name == "value" {
                *self.value.borrow_mut() = value.to_number();
                Ok(())
            } else {
                Err(MochaError::JavaScript(format!("no such property: {name}")))
            }
        }
        fn call(
            &self,
            _: &mut Interpreter,
            name: &str,
            args: Vec<JsValue>,
        ) -> MochaResult<JsValue> {
            match name {
                "add" => {
                    let delta = args.first().map(JsValue::to_number).unwrap_or(0.0);
                    *self.value.borrow_mut() += delta;
                    Ok(JsValue::Number(*self.value.borrow()))
                }
                other => Err(MochaError::JavaScript(format!("no such method: {other}"))),
            }
        }
    }

    fn host_value() -> JsValue {
        JsValue::Host(Rc::new(Counter {
            value: RefCell::new(10.0),
        }))
    }

    fn run_with_counter(source: &str) -> JsValue {
        let mut interpreter = Interpreter::new();
        interpreter.define_global("counter", host_value());
        interpreter.run(&parse(source).unwrap()).unwrap()
    }

    #[test]
    fn host_object_get_set_and_method_call() {
        assert!(matches!(run_with_counter("counter.value;"), JsValue::Number(n) if n == 10.0));
        assert!(matches!(run_with_counter("counter.add(5);"), JsValue::Number(n) if n == 15.0));
        // A set is observable through a later get on the same host.
        assert!(
            matches!(run_with_counter("counter.value = 3; counter.add(2); counter.value;"),
                JsValue::Number(n) if n == 5.0)
        );
        // String indexing routes to the same host get.
        assert!(matches!(run_with_counter("counter[\"value\"];"), JsValue::Number(n) if n == 10.0));
    }

    #[test]
    fn host_object_identity_is_pointer_equality() {
        let mut interpreter = Interpreter::new();
        let host = host_value();
        interpreter.define_global("a", host.clone());
        interpreter.define_global("b", host);
        interpreter.define_global("c", host_value());
        assert!(matches!(
            interpreter.run(&parse("a === b;").unwrap()).unwrap(),
            JsValue::Bool(true)
        ));
        assert!(matches!(
            interpreter.run(&parse("a === c;").unwrap()).unwrap(),
            JsValue::Bool(false)
        ));
    }

    #[test]
    fn host_unknown_property_is_undefined_and_bad_method_errors() {
        assert!(matches!(
            run_with_counter("counter.missing;"),
            JsValue::Undefined
        ));
        let mut interpreter = Interpreter::new();
        interpreter.define_global("counter", host_value());
        let error = interpreter
            .run(&parse("counter.nope();").unwrap())
            .unwrap_err();
        assert!(matches!(error, MochaError::JavaScript(_)));
    }
}
