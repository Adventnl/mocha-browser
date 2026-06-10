# JavaScript Interpreter (Milestone 6)

Milestone 6 adds `mocha_js`, a small **from-scratch** JavaScript-subset
interpreter. It evaluates standalone snippets — it is **not** wired to the DOM,
`window`, `document`, events, or `<script>` tags (that is Milestone 7).

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
defining environment; native functions are Rust `fn`s).

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
limit exceeded")` error, so infinite loops cannot hang the host.

## Known non-compliance / not implemented

Not ECMAScript-compliant. No DOM bindings, `window`/`document`, `<script>`
execution, external scripts, timers, promises, `async`/`await`, modules, classes,
`new`, prototype chains, full `this` semantics, regular expressions, `Date`,
`JSON`, template literals, arrow functions, destructuring, spread, generators,
`break`/`continue`, exceptions/`try`-`catch`, or a garbage collector beyond Rust
ownership + `Rc`. `==`/`!=` are strict (see above).

## What Milestone 7 will add

Milestone 7 will bridge this interpreter to the DOM: exposing `document`/`window`,
`querySelector`, DOM mutation, and `addEventListener` (wrapping JS callbacks as
the `mocha_events` listeners), and executing `<script>` tags. That requires
deliberate **DOM mutation APIs** in `mocha_dom`, which are intentionally absent
today.
