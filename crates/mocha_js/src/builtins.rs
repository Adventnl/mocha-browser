//! Minimal built-in objects and functions (`console`, `Math`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mocha_error::MochaResult;

use crate::environment::Environment;
use crate::interpreter::Interpreter;
use crate::value::{Function, JsValue, NativeFn};

/// Install the built-in globals into `global`.
pub(crate) fn install(global: &Rc<RefCell<Environment>>) {
    let mut env = global.borrow_mut();

    let mut console = HashMap::new();
    console.insert("log".to_string(), native("log", console_log));
    env.define("console", JsValue::object(console), false);

    let mut math = HashMap::new();
    math.insert("abs".to_string(), native("abs", math_abs));
    math.insert("floor".to_string(), native("floor", math_floor));
    math.insert("ceil".to_string(), native("ceil", math_ceil));
    math.insert("round".to_string(), native("round", math_round));
    math.insert("max".to_string(), native("max", math_max));
    math.insert("min".to_string(), native("min", math_min));
    env.define("Math", JsValue::object(math), false);
}

fn native(name: &str, func: NativeFn) -> JsValue {
    JsValue::Function(Rc::new(Function::Native {
        name: name.to_string(),
        func,
    }))
}

fn console_log(interpreter: &mut Interpreter, args: &[JsValue]) -> MochaResult<JsValue> {
    let line = args
        .iter()
        .map(JsValue::stringify)
        .collect::<Vec<_>>()
        .join(" ");
    interpreter.console.push(line);
    Ok(JsValue::Undefined)
}

fn arg_number(args: &[JsValue], index: usize) -> f64 {
    args.get(index).map(JsValue::to_number).unwrap_or(f64::NAN)
}

fn math_abs(_: &mut Interpreter, args: &[JsValue]) -> MochaResult<JsValue> {
    Ok(JsValue::Number(arg_number(args, 0).abs()))
}

fn math_floor(_: &mut Interpreter, args: &[JsValue]) -> MochaResult<JsValue> {
    Ok(JsValue::Number(arg_number(args, 0).floor()))
}

fn math_ceil(_: &mut Interpreter, args: &[JsValue]) -> MochaResult<JsValue> {
    Ok(JsValue::Number(arg_number(args, 0).ceil()))
}

fn math_round(_: &mut Interpreter, args: &[JsValue]) -> MochaResult<JsValue> {
    // Round half up, matching JavaScript's Math.round.
    Ok(JsValue::Number((arg_number(args, 0) + 0.5).floor()))
}

fn math_max(_: &mut Interpreter, args: &[JsValue]) -> MochaResult<JsValue> {
    let mut result = f64::NEG_INFINITY;
    for arg in args {
        result = result.max(arg.to_number());
    }
    Ok(JsValue::Number(result))
}

fn math_min(_: &mut Interpreter, args: &[JsValue]) -> MochaResult<JsValue> {
    let mut result = f64::INFINITY;
    for arg in args {
        result = result.min(arg.to_number());
    }
    Ok(JsValue::Number(result))
}
