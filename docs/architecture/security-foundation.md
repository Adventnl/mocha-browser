# Security Foundation (Milestone 16)

Milestone 16 adds `mocha_security`: a policy crate for explicit security
decisions. It is **not** a full browser security model, **not** a sandbox, and
**not** site isolation.

Implemented policy objects:

- `SecurityDecision` / `SecurityViolation` for allow/block results.
- Same-origin helpers over `mocha_origin::Origin`.
- URL context checks for document navigation, subresources, scripts, forms, and
  web storage.
- A conservative `file://` helper that allows same-directory/descendant file
  resources for local documents.
- Mixed-content awareness for future HTTPS documents.
- A small CSP parser/evaluator (see
  [content-security-policy.md](content-security-policy.md)).
- Origin-keyed `PermissionManager` state with no UI prompt.
- Future-facing certificate error data types; TLS is still unsupported.
- Renderer/browser/network capability sets for future process architecture.

Current integration is deliberately narrow. The policy objects are implemented
and tested, but normal document rendering still uses the existing single-process
pipeline. The default page-loading path still does not automatically use the
profile cookie jar, and JS `localStorage`/`sessionStorage` remain runtime-local
unless an embedder wires persistent/tab-owned state.

M17 uses the capability model conceptually for the renderer-process prototype.
M18 adds `mocha_sandbox`, a capability-restricted prepared-document path that
denies direct file/network/profile/spawn capabilities in the restricted renderer.
This is still not OS-level sandboxing; `NoopPlatformSandbox` reports
`CapabilityRestrictedOnly`.

M16 does not implement HTTPS/TLS, CORS, full Fetch, service workers, site
isolation, process isolation, or OS sandboxing.
