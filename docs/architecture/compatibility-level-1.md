# Mocha Compatibility Level 1

This document is the **truth source** for what Mocha Browser is expected to handle
after Milestone 20. It is a deliberately small, honest target.

**Mocha Compatibility Level 1 is not modern-web compatibility.** It is not
Chromium, WebKit, or Gecko behaviour. It is the realistic subset that the
`mocha_compat` harness (see [compatibility-testing.md](compatibility-testing.md))
exercises and that the engine is expected to keep working. Anything not listed as
*supported* is either listed as *unsupported* (and must fail with a clear error,
never silently) or is simply out of scope for this milestone.

## Supported document loading

- Local file documents (bare path or `file://`).
- `http://` and `https://` documents (TLS via rustls, Mozilla roots, since M21).
- HTTP redirects (the final URL becomes the document base URL).
- `text/html` only; other content types fail clearly.
- UTF-8 only; other encodings fail clearly.
- External `<link rel="stylesheet">` stylesheets (same base URL).
- `<img>` images decoded as PNG or JPEG.
- The GET-only form-submission model (form-urlencoded query URLs).

**Unsupported:** `<script src>` (external scripts), CSS `url(...)`,
non-UTF-8 encodings, content types other than `text/html`, POST submission,
HTTP caching beyond the in-memory loader cache.

## Supported HTML subset

`html`, `body`, `h1`, `h2`, `p`, `div`, `span`, `a`, `style`, `script` (inline
only), `link rel=stylesheet`, `img`, `form`, `input`
(`text`/`password`/`checkbox`/`radio`/`submit`/`reset`/`hidden`), `button`,
`label`, `textarea`, `select`, `option`. A leading `<!doctype html>` and HTML
comments are accepted and ignored. Unknown/unsupported tags and malformed nesting
produce a clear `UnsupportedFeature`/`Parse` error rather than guessing — the
parser does not implement HTML5 error recovery.

**Unsupported:** `<head>` and `<title>` (there is no document title element; a tab
title falls back to the URL), the rest of the HTML5 tag set, and the full HTML5
error-recovery ("tag soup") algorithm. Tables, lists, semantic sectioning
elements, media elements, `<canvas>`, `<svg>`, `<iframe>`, and `<input>` types
beyond the list above are out of scope and error clearly.

## Supported CSS subset

- Selectors: type, class, id, universal, descendant, compound, selector lists.
- Specificity, the cascade, inheritance, inline `style=""`, `<style>` blocks,
  external stylesheets (in document order).
- Properties: `display` (`block`/`inline`/`none`), `color`, `background-color`,
  `font-size`, `font-weight`, `width`, `height`, `margin`, `padding`,
  `border-width`, `border-color`.

**Unsupported:** flexbox, grid, `position`, `float`, `z-index`, media queries,
pseudo-classes, pseudo-elements, attribute selectors, combinators other than
descendant, `calc()`, `var()`/custom properties, `rgb()`/`hsl()` colour
functions, `@font-face`/web fonts, `url(...)`, animations, transitions,
transforms. Unsupported properties fail the render with a clear
`UnsupportedFeature` error.

## Supported layout subset

- Block layout and a simple inline formatting context (line boxes, word
  wrapping, text runs, anonymous blocks).
- `<img>` as a replaced element; form controls as control boxes.
- A simple box model (content + padding + border + margin).
- Document scrolling in the desktop shell.

**Unsupported:** real font metrics and shaping (a fixed-advance debug font is
used), baseline / `vertical-align`, tables, flexbox, grid, positioning, and any
overflow model beyond simple document scroll.

## Supported JavaScript subset

`mocha_js` is a tiny from-scratch interpreter (no V8/SpiderMonkey/QuickJS/Boa).

- Values: numbers, strings, booleans, `null`, `undefined`, objects, arrays,
  functions (with closures).
- Statements/operators: `let`/`const`/`var`, `function`, `return`, `if`/`else`,
  `while`, `for`, arithmetic/comparison/logical operators, member and index
  access.
- Built-ins: `console.log`, a subset of `Math` (`abs`, `ceil`, `floor`, `round`,
  `max`, `min`), and arrays (`push`, `pop`, `length`).
- An execution **step budget** (`DEFAULT_STEP_LIMIT`) aborts runaway loops with a
  clear "step limit" error instead of hanging.

**Unsupported:** classes, modules, `import`/`export`, promises, `async`/`await`,
`try`/`catch`/`throw`, `switch`, `do`/`while`, `break`/`continue`, `new`,
`typeof`, `instanceof`, `RegExp`, `Date`, `JSON`, most `String`/`Array` methods,
generators, and full prototype/`this` semantics. Unsupported syntax fails with a
clear `Parse`/`JavaScript` error.

## Supported DOM subset

Exposed by `mocha_js_dom` over the shared document/form state:

- `window`, `document`, `console`.
- `document.getElementById`, `document.querySelector`,
  `document.querySelectorAll`, `document.createElement`,
  `document.createTextNode`, `document.body`.
- `Element.textContent`, `Element.innerHTML` (basic), `Element.setAttribute`,
  `Element.getAttribute`, `Element.className`, `Element.id`,
  `Element.style` (subset), `appendChild`, `removeChild`.
- `addEventListener` / `removeEventListener`, `preventDefault`, and the
  click default actions (checkbox/radio toggle, reset, submit identification).
- `setTimeout` / `clearTimeout` (timers are run once, synchronously, after the
  scripts — there is no real event loop).
- Form-control reflection: `value`, `checked`, `disabled`, `type`, `name`,
  `selectedIndex`, and `form.submit()` (recorded, never navigated).
- `document.cookie` and `localStorage`/`sessionStorage` foundations (origin-keyed;
  see [cookies-and-web-storage.md](cookies-and-web-storage.md)).

**Unsupported:** the broad Web API surface (fetch, History, URL, MutationObserver,
Web Storage events, IndexedDB, Cache API, service workers, etc.), live
collections, and a real asynchronous event loop.

## Browser shell

Desktop shell with browser chrome (address bar, navigation buttons), tabs, an
in-memory session model, profile storage, cookie/`localStorage` foundations, and
headless DevTools snapshots. See
[desktop-shell.md](desktop-shell.md), [browser-chrome.md](browser-chrome.md),
[tabs-and-session.md](tabs-and-session.md), [profile-storage.md](profile-storage.md),
and [devtools.md](devtools.md).

## Security honesty

Mocha has an origin model and policy helpers, a tiny CSP parser/evaluator, a
sandbox prototype, and a multi-process prototype. **None of this makes Mocha safe
for arbitrary web browsing.** It is not production-secure, not site isolation,
not a complete CSP/CORS implementation, and not an OS sandbox. See
[security-foundation.md](security-foundation.md),
[security-sandbox.md](security-sandbox.md), and
[multiprocess-prototype.md](multiprocess-prototype.md).
