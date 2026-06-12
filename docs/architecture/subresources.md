# Subresource Loading (Milestone 8)

Milestone 8 loads the stylesheets a document references — inline `<style>` blocks
and external `<link rel="stylesheet">` resources — and integrates them into the
cascade. Discovery and loading live in the `mocha_resources` crate.

## Base URL

The base URL for resolving subresources is the document's **final URL** (after
redirects), already tracked by `mocha_net` as `ResourceResponse.final_url`. A
`<base href>` element is **not** supported (documented limitation).

Relative references are resolved with `Url::join`, which handles absolute URLs,
scheme-relative (`//host/path`), absolute-path (`/path`), and relative paths
(against the URL's directory), and now **normalizes dot-segments** (`.`/`..`) in
`/`-separated paths. Windows file paths (with backslashes) are left for the OS to
resolve at filesystem-access time.

In-memory rendering (`run_html`, no base URL) cannot resolve external references;
an external `<link rel="stylesheet">` there is `UnsupportedFeature`. Inline
`<style>` still works.

## Stylesheet discovery and loading

`collect_document_stylesheets(document, base, loader)` walks the document **once
in document order** and produces a `Vec<Stylesheet>`:

- `<style>` → its CSS text is parsed inline.
- `<link rel="stylesheet" href="…">` → the href is resolved against `base`, loaded
  through `mocha_net`, validated, and parsed.

Because the result preserves source order, the existing cascade tie-break ("later
sheet wins") and inline-`style`-attribute precedence both fall out unchanged.

## Content-type and error policy

An external stylesheet must classify as `text/css` (an explicit `text/css`
content type, or a missing content type on a `.css` URL). A non-`text/css`
response, a non-2xx status, a missing `href`, or an unsupported scheme is a
**clear error** — failed stylesheets are never silently ignored. `<link>`
elements whose `rel` is not `stylesheet` are ignored, as browsers ignore unknown
link relations.

`<link>` is parsed as a **void element** (no `</link>`) with UA `display: none`,
so its text is never laid out or painted.

## HTML & cascade integration

- `mocha_html` accepts `<link>` and `<img>` as void elements (appended but not
  pushed onto the open-element stack).
- The shell collects stylesheets **after** inline scripts run, so a JS-created
  `<link rel="stylesheet">` placed in the tree before collection is picked up in
  the single post-script pass. There is no incremental/dynamic subresource
  loading.

## Out of scope (documented)

External `<script src>` (still unsupported), CSS `url(...)` resources
(backgrounds/fonts — the CSS parser rejects functions), web fonts, a `<base>`
element, and HTTP cache semantics beyond the existing tiny in-memory cache.
HTTPS subresources load like HTTPS documents (TLS via rustls since M21).

## Tests

`mocha_resources` unit tests plus a `subresource_pipeline` integration test cover:
relative/absolute/`..` resolution over file and HTTP bases; external stylesheet
applies (local file and local test server); content-type required;
document-order cascade ("later wins"); inline style beats external; missing
stylesheet and wrong content type error clearly; in-memory external link
unsupported. No public internet is used.
