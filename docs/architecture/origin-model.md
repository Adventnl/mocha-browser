# Origin Model (Milestone 15)

The `mocha_origin` crate provides a minimal web **origin**: the
`(scheme, host, port)` tuple that cookies and web storage are scoped to. It is
deliberately small and is **not** the full HTML origin concept (no opaque-origin
algebra beyond `file://`, no `document.domain`, no origin serialization rules
beyond a storage key).

## `Origin`

```rust
pub struct Origin {
    pub scheme: String,      // "http" or "https", lowercased
    pub host: String,        // lowercased
    pub port: Option<u16>,   // None when it is the scheme default
}
```

- `Origin::from_url(&Url)` derives the origin.
- `is_same_origin(&self, other)` is tuple equality.
- `storage_key()` serializes to `http://host` or `http://host:port`.

## Rules

- **Same origin** is exact equality of scheme, host, and (normalized) port.
- **Default ports are normalized:** `http://example.com` and
  `http://example.com:80` are the **same** origin (`port == None`);
  `http://example.com:8080` differs.
- **Different scheme** differs (`http` vs `https`), **different host** differs,
  **different port** differs.
- Host is compared **case-insensitively** (stored lowercased).

## `file://` policy (conservative)

`file://` URLs have an **opaque** origin in real browsers. Mocha takes the
conservative route: `Origin::from_url` returns `MochaError::Security` for
`file://` (and for any URL without a host). Consequently, origin-keyed web
storage and `document.cookie` are simply **unavailable** on `file://` documents
(a clear error) rather than being silently shared across files. This is tested
(`file_origin_is_opaque_and_unsupported`).

## What this is not

- No public-suffix list, no site/eTLD+1 concept, no scheme/host/port coercion
  beyond the above.
- No opaque-origin identity for sandboxed/`data:`/`blob:` documents (those
  schemes are unsupported by `mocha_url`).
- Not a complete security boundary: M16 adds policy objects for same-origin
  checks, CSP, and mixed-content awareness in `mocha_security`, but there is no
  process isolation, OS sandbox, TLS, CORS, or broad enforcement across every
  render path. See [security-foundation.md](security-foundation.md) and
  [limitations.md](limitations.md).
