# Content Security Policy (Milestone 16)

Mocha's M16 CSP support is a tiny policy subset in `mocha_security`. It is useful
for tests and future integration, but it is **not** complete CSP.

Supported directives:

- `default-src`
- `script-src`
- `style-src`
- `img-src`
- `connect-src`
- `form-action`

Supported source expressions:

- `'self'`
- `'none'`
- `*`
- `http:`
- `https:`
- exact origins such as `http://example.com`

Unknown directives are ignored. Malformed known source expressions return a clear
`MochaError::Security`. Nonces, hashes, `unsafe-inline`, path matching, wildcard
hosts, reporting, workers, and the full CSP grammar are unsupported.

The parser and evaluator are implemented and tested. Broad runtime enforcement is
deferred: inline script CSP enforcement is incomplete, and subresource/form
checks are not yet wired through every render path.
