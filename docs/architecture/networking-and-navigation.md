# Networking and Navigation (Milestone 4)

Milestone 4 adds **document loading** and a **navigation history** in front of the
existing rendering engine. It is deliberately small and is **not** a real browser
network stack.

## Crate roles

### `mocha_url`

Parses and normalizes locations. The `Url` model carries `scheme`, `host`,
`port`, `path`, `query`, and `fragment`. It lowercases scheme and host, defaults
the path to `/` for HTTP, provides `request_target()` (path + query, never the
fragment), `normalized()` (a cache key), and `join()` for resolving redirect
`Location` values. It performs **no network I/O** and is not WHATWG-compliant.

### `mocha_net`

Loads a single resource into a `ResourceResponse` via the `ResourceLoader` trait.
`DefaultLoader` handles:

- **`file://` and local paths** — reads bytes, infers content type from the file
  extension, returns a directory as a clear `UnsupportedFeature` error.
- **`http://`** — a hand-written blocking HTTP/1.1 `GET` over
  `std::net::TcpStream` (`Connection: close`). It parses the status line and
  headers, follows redirects, and consults a small in-memory cache.
- **`https://`** — **not implemented**; returns `UnsupportedFeature`.

`mocha_net` knows nothing about navigation history, HTML, CSS, layout, or paint.

### `mocha_nav`

Owns the back/forward history (`NavigationController`) over any `ResourceLoader`.
It supports `navigate`, `back`, `forward`, and `reload`, stores each entry's
**final** URL (after redirects), truncates forward history on a new navigation,
and leaves history unchanged when a load fails. It does not parse protocols or
render documents.

### `mocha_shell`

Orchestrates: parse the input with `mocha_url`, load it through a
`NavigationController` + `DefaultLoader`, check the content type, decode the body
as UTF-8, then run the existing HTML → style → layout → paint pipeline.

## file / http / https support

| Scheme   | Status                                                      |
| -------- | ---------------------------------------------------------- |
| `file://`, local paths | Supported (read from disk)                   |
| `http://`              | Supported (blocking HTTP/1.1 GET, no TLS)    |
| `https://`             | **Not implemented** — clear `UnsupportedFeature` |

**Why no HTTPS:** TLS must not be hand-rolled, and pulling in a TLS stack
(rustls/ring/…) is a heavy, build-fragile dependency. Rather than fake it or risk
the build, Milestone 4 implements HTTP fully and fails HTTPS honestly. HTTPS via a
vetted TLS library is future work.

## Redirect behavior

- Follows `301`, `302`, `303`, `307`, `308` using the `Location` header.
- Resolves absolute, scheme-relative (`//host/…`), absolute-path (`/…`), and
  relative locations via `Url::join`, which normalizes dot-segments (`.`/`..`) in
  URL/POSIX paths (also used for subresource resolution).
- Limited to 10 redirects; exceeding the limit is a clear `Network` error.
- A missing `Location` on a redirect is a clear error.
- **Redirects to `file://` are rejected** (a `Network` error); unsupported-scheme
  redirects are rejected. An `http → https` redirect surfaces the HTTPS
  "unsupported" error.

## Content-type handling

A response is classified (`Html`, `Css`, `Text`, `Binary`, `Unknown`) from its
`Content-Type` (parameters like `; charset=utf-8` ignored), falling back to the
URL extension when the type is absent. **Only HTML is rendered**; `text/plain`,
`text/css`, `application/octet-stream`, `image/*`, etc. return a clear
`UnsupportedFeature`. Bytes that are not valid UTF-8 are rejected (no charset
decoding beyond UTF-8).

## Memory cache (limitations)

`MemoryCache` is a process-lifetime `HashMap` keyed by the **final** normalized
URL. It is **not** an HTTP cache: there is no `Cache-Control`, validation, or
expiration, and only `200` responses are stored. A second load of the same URL
returns `from_cache = true`; reload / `--no-cache` bypasses it. The CLI creates a
fresh loader per invocation, so the cache matters within a process (e.g. a
back/forward navigation), not across CLI runs.

## Navigation history behavior

`navigate` appends an entry (storing the final URL) and discards forward history;
`back`/`forward` move only after a successful load; `reload` re-fetches the
current entry bypassing the cache; a failed load never corrupts history.

## Form submission (Milestone 10)

Form submission is **modelled, not performed**. `mocha_forms::build_submission`
collects a form's successful controls and produces a `FormSubmission` whose
`action` is a plain `Url`: the form's `action` attribute resolved against the
document URL (empty/missing `action` → the document URL itself) with the fields
serialized as an `application/x-www-form-urlencoded` query. An embedder may pass
that URL to `NavigationController::navigate` explicitly; nothing in the pipeline
navigates automatically, and the shell never submits. **Only `method="get"` is
supported** — `method="post"` returns
`UnsupportedFeature("POST form submission is not supported in Milestone 10")`,
and no request body is ever constructed. See
[forms-and-controls.md](forms-and-controls.md).

## Security limitations

Networking adds risk; Mocha is **not** safe for general browsing.

- No TLS (no HTTPS); TLS verification is never disabled because TLS is not
  implemented at all.
- **Cookies (Milestone 15):** `mocha_net` exposes a `CookieProvider` trait and
  `DefaultLoader::load_with_cookies`, which attach a `Cookie` header and store
  `Set-Cookie` responses per redirect hop (cache-bypassing). The jar/matching/
  persistence live in `mocha_cookie`/`mocha_storage`; `mocha_net` stays storage-
  agnostic. The default `load`/navigation path still sends no cookies — wiring
  page loads through the jar automatically is deferred. See
  [cookies-and-web-storage.md](cookies-and-web-storage.md).
- No authentication, credentials, or proxy support.
- A **minimal origin model** exists (`mocha_origin`) for storage/cookie scoping,
  and M16 adds same-origin, mixed-content, and CSP policy objects in
  `mocha_security`. Broad networking/render-path enforcement is still incomplete.
- Subresource loading (Milestones 8–9) is layered on top of `mocha_net` by
  `mocha_resources`/`mocha_image`, not by `mocha_net` itself: external
  `<link rel="stylesheet">` CSS and `<img>` images are loaded against the document
  base URL with MIME checks and an error policy (see
  [subresources.md](subresources.md) and
  [images-and-replaced-elements.md](images-and-replaced-elements.md)). External
  `<script src>`, CSS `url(...)`, and web fonts are still not loaded.
- **HTTP caching is not standards-compliant** and must not be relied on for
  correctness.
- Downloaded data is never written to disk.
