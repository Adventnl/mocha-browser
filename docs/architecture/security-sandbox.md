# Security Sandbox Prototype (Milestone 18)

Milestone 18 adds a capability-based renderer sandbox prototype. It is **not** a
production browser sandbox, not Chromium-level security, and not site isolation.

Implemented pieces:

- `mocha_sandbox::RendererSandboxPolicy`
- default renderer policy denying direct file reads, network loads, profile
  storage, and process spawning
- `NoopPlatformSandbox`, which honestly reports `CapabilityRestrictedOnly`
- a prepared-document renderer path over IPC
- process tests proving the restricted renderer rejects legacy direct document
  loads

The legacy M17 `RenderDocument { input }` path still exists and is explicitly
unsandboxed. The M18 restricted path is `RenderPreparedDocument`: the browser
side prepares HTML and metadata, then the renderer parses/scripts/layouts/paints
that data without receiving a path or URL to load.

OS-level sandboxing is not applied in M18. Future platform work may use seccomp,
Landlock, pledge/unveil, AppContainer/job objects, or macOS sandbox profiles, but
none of those are claimed here.
