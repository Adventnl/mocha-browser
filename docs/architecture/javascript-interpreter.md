# JavaScript Interpreter (Milestone 6)

Milestone 6 adds `mocha_js`, a small **from-scratch** JavaScript-subset
interpreter. The interpreter core remains DOM-agnostic and is usable standalone
(via `--eval-js`). Since Milestone 7 it also provides a **host-object mechanism**
(`JsValue::Host` + the `HostObject` trait) that the separate `mocha_js_dom` crate
uses to wire it to `window`/`document`/the DOM and run inline `<script>` — see
[dom-bindings.md](dom-bindings.md). `mocha_js` itself still knows nothing about
the DOM.

## Why Mocha has its own interpreter

Mocha is a from-scratch engine, so it cannot embed an existing JavaScript engine
or parser. The following are **forbidden** and not used: V8, SpiderMonkey,
JavaScriptCore, QuickJS, Deno, Node.js, Boa, rquickjs, swc, tree-sitter, Babel,
or any other existing JS interpreter/compiler/parser. `mocha_js` depends only on
`mocha_error` and the Rust standard library.

## Pipeline

```text
source string -> lexer -> tokens -> parser -> AST -> interpreter -> JsValue
```

- **lexer** (`lexer.rs`): numbers, single/double-quoted strings (with `\n \t \r \\ \' \"`),
  identifiers/keywords, `//` and `/* */` comments, and the operator set.
- **parser** (`parser.rs`): recursive descent for statements, precedence climbing
  for expressions. Clear `Parse` errors, never panics.
- **interpreter** (`interpreter.rs`): a tree-walker over a scope chain.

Public API: `lex`, `parse`, `Interpreter`, and the convenience `JsRuntime`
(`eval` + `take_console_output`). The shell exposes `--eval-js "<source>"`.

## Supported subset

- **Values:** number (f64), string, boolean, `null`, `undefined`, object, array,
  function.
- **Declarations:** `let`, `const` (immutable), `var` (treated like `let`).
- **Statements:** variable/function declarations, `return`, expression
  statements, blocks, `if`/`else`, `while`, `for`.
- **Expressions:** literals, identifiers, assignment, binary (`+ - * / %`),
  comparison (`< <= > >=`), equality (`== != === !==`), logical (`&& ||`,
  short-circuiting), unary (`! -`), calls, member access (`a.b`), indexing
  (`a[i]`), object/array literals, and function expressions.
- **Built-ins:** `console.log` (captured, not printed from the library);
  `Math.abs/floor/ceil/round/max/min`; array `.length`/`.push`/`.pop`; string
  `.length`.

## Value model

`JsValue` (see `value.rs`). Objects and arrays are `Rc<RefCell<…>>` so they have
reference semantics; functions are `Rc<Function>` (user functions capture their
defining environment; native functions are Rust `fn`s, or state-capturing
`NativeClosure`s used by host crates for globals like `setTimeout`). A
`JsValue::Host(Rc<dyn HostObject>)` variant lets an embedder back a value with
native state; `===` on host values compares by pointer identity.

## Scope / environment model

`Environment` (see `environment.rs`) is a `HashMap` of bindings with an optional
parent. Lookups and assignments walk the parent chain. `let`/`var` are mutable;
`const` bindings reject reassignment. Function declarations are **hoisted** within
their scope (enabling forward references and recursion); other hoisting and the
temporal dead zone are not implemented. Block statements get a child scope, so
shadowing works.

## Functions and closures

User functions capture the environment in which they were defined, so closures
work (`makeAdder(10)(5) === 15`). Calls create a child scope binding parameters
(missing arguments are `undefined`); `return` unwinds to the call. Recursion works
because named declarations are bound before they are called.

## Type coercion (documented subset, not ECMAScript)

- `Number + Number` → number; if **either** side is a string, `+` concatenates
  (numbers stringify without a trailing `.0`).
- Other arithmetic coerces both sides to number (`true`→1, `null`→0,
  `undefined`→NaN, strings parse or NaN).
- Comparisons: both strings compare lexicographically, otherwise numerically.
- `==`/`!=` behave like `===`/`!==` (strict). Consequently `null == undefined` is
  **false** here (unlike real JavaScript) — a deliberate simplification.
- Truthiness: `false`, `null`, `undefined`, `0`, `NaN`, `""` are falsy.

## Step limit

Every statement and expression evaluation consumes one step; exceeding
`DEFAULT_STEP_LIMIT` (100,000) aborts with a clear `JavaScript("execution step
limit exceeded")` error, so infinite loops cannot hang the host. **Known caveat:**
the step budget counts *logical* steps, not native stack depth, so very deep
recursion or deeply nested expressions can still overflow the Rust call stack
before the budget trips. This is an accepted limitation.

## Known non-compliance / not implemented

Not ECMAScript-compliant. No promises, `async`/`await`, modules, classes, `new`,
prototype chains, full `this` semantics, regular expressions, `Date`, `JSON`,
template literals, arrow functions, destructuring, spread, generators, ternary
`?:`, `switch`, `break`/`continue`, exceptions/`try`-`catch`, or a garbage
collector beyond Rust ownership + `Rc`. `==`/`!=` are strict (see above). The DOM
binding surface (Milestone 7) is a tiny hand-picked subset, not the Web IDL DOM —
see [dom-bindings.md](dom-bindings.md).

## DOM bindings (Milestone 7, done)

The interpreter is bridged to the DOM by the `mocha_js_dom` crate: it installs
`window`/`document`/`console` globals as host objects, exposes a small DOM
read/mutate/query API, dispatches JS event listeners, runs deterministic timers,
and executes inline `<script>` against a shared `Document` (which gained the
deliberate mutation APIs `set_text_content`/`set_attribute`/`get_element_by_id`/…).
See [dom-bindings.md](dom-bindings.md).
