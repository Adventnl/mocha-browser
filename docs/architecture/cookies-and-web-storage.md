# Cookies and Origin-Aware Web Storage (Milestone 15)

Milestone 15 adds browser web-state foundations: a cookie model and jar
(`mocha_cookie`), cookie/`localStorage` persistence in the profile
(`mocha_storage`), an `http` cookie integration (`mocha_net`), and the
`document.cookie` / `localStorage` / `sessionStorage` JavaScript bindings
(`mocha_js_dom`). It is **not** a complete cookie or security model. Do not rely
on it for privacy or isolation. See the limitations at the end.

## Cookie model (`mocha_cookie`)

```rust
pub struct Cookie {
    pub name, value, domain, path: String,
    pub expires_ms: Option<i64>,   // from Max-Age or Expires
    pub max_age: Option<i64>,
    pub secure, http_only, host_only: bool,
    pub same_site: SameSite,       // Lax | Strict | None | Unspecified
    pub created_ms: i64,
}
```

- **`parse_set_cookie(header, request_url, now_ms)`** parses `Name=Value` plus
  `Domain`, `Path`, `Max-Age`, `Expires` (the common IMF-fixdate format only;
  see `date.rs`), `Secure`, `HttpOnly`, and `SameSite`. It **errors**
  (`MochaError::Security`) on an empty/invalid cookie name or a `Domain` that
  does not domain-match the request host. `Max-Age` wins over `Expires`;
  `Max-Age <= 0` expires immediately. A cookie with no `Domain` is **host-only**.
- **`CookieJar`** matches and orders cookies for a request:
  - **domain-match** (host-only → exact host; otherwise host or subdomain),
  - **path-match** (RFC 6265 §5.1.4 prefix rule, with a default-path from the
    request directory),
  - **secure** cookies are only sent over `https` (so, since HTTPS is
    unsupported, effectively never sent),
  - **expired** cookies are excluded,
  - send order is **deterministic**: longest path first, then earliest
    `created_ms`, then name.
- `cookie_header_for_request` builds the `Cookie: n1=v1; n2=v2` header.

## Cookie persistence (`mocha_storage`)

`CookieStore` persists cookies in the `cookies` table (primary key
`name, domain, path`) via migration 2. `cookies_for_request` /
`cookie_header_for_request` load the rows into a `CookieJar` for matching. A
**private** profile keeps cookies only in memory (a fresh private profile has
none). `clear_cookies` empties the store.

## Network integration (`mocha_net`)

`mocha_net` defines a `CookieProvider` trait and **does not depend on the storage
layer**:

```rust
pub trait CookieProvider {
    fn cookie_header_for_request(&mut self, url, now_ms) -> MochaResult<Option<String>>;
    fn store_response_cookies(&mut self, url, headers, now_ms) -> MochaResult<()>;
}
```

`DefaultLoader::load_with_cookies(request, provider, now_ms)` attaches the
`Cookie` header before the request and stores `Set-Cookie` headers from the
response, **per redirect hop**. This path **bypasses the in-memory cache** (so the
live `Cookie` header is always sent). Cookies apply only to `http`/`https`;
`file://` never uses cookies. An embedder supplies a `CookieProvider` (e.g. over a
`mocha_cookie::CookieJar` or `mocha_storage`'s `CookieStore`); the engine/desktop
do not yet drive page loads through this path automatically (deferred — see
limitations). Integration is covered by local-test-server round-trip tests.

## `document.cookie` (`mocha_js_dom`)

The JS runtime gets the document URL (`DomRuntime::with_url`), threaded from the
engine's document base URL. `document.cookie`:

- **getter** returns the non-`HttpOnly` cookies for the document URL as
  `n1=v1; n2=v2`;
- **setter** stores one cookie for the document URL (an `HttpOnly` attribute from
  script is ignored; an unparseable value is silently dropped, like browsers);
- is **unavailable without an http(s) origin** — on a `file://` or in-memory
  document the setter returns `MochaError::Security` and the getter returns `""`.

The per-render jar is in-memory and **not** wired to the network jar or the
persistent profile (a single render starts with an empty jar). An embedder can
inject a server `Set-Cookie` via `DomRuntime::store_response_set_cookie`, which is
how an `HttpOnly` cookie can exist that `document.cookie` must not expose.

## `localStorage` / `sessionStorage`

- **Storage layer:** `LocalStorageStore` (persistent, `local_storage` table keyed
  by `origin, key`) and `SessionStorage` (in-memory, origin-keyed). Different
  origins are isolated; a private profile does not persist `localStorage`;
  `sessionStorage` is never persisted.
- **JS bindings:** `localStorage`/`sessionStorage` globals expose
  `getItem`/`setItem`/`removeItem`/`clear` (and `length`). Keys/values are
  strings; a missing key returns `null`. Both require an http(s) origin (on
  `file://`/in-memory documents they return a clear `MochaError::Security`).
- **Note (deferred wiring):** the JS `localStorage`/`sessionStorage` backends are
  **per-render in-memory** maps inside the runtime; they are **not yet** connected
  to the persistent `LocalStorageStore` or shared across renders/tabs. The
  persistent store exists and is tested at the storage layer; wiring it into the
  JS runtime (and `sessionStorage` to the tab) is future work.

## Limitations (M15)

- Cookie implementation is **minimal** — not full RFC 6265bis: no public-suffix
  list, no `__Secure-`/`__Host-` prefixes, no third-party/partitioned-cookie
  policy, no cookie UI, no real `SameSite` enforcement (parsed/stored only, since
  Mocha has no navigation-site model).
- No HTTPS means `Secure` cookies are effectively never sent.
- Session cookies (no `Max-Age`/`Expires`) are treated as non-expiring by the jar.
- `localStorage` is minimal: **no quotas, no `StorageEvent`**, and JS-side
  persistence/sharing is deferred (see above). No IndexedDB, Cache API, WebSQL, or
  StorageManager.
- This is **not** a complete security model: no sandbox, CSP, mixed-content
  blocking, or process isolation. Mocha is still not safe for general browsing.
