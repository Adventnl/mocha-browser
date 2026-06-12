# Multi-Process Prototype (Milestone 17)

Milestone 17 adds a small renderer-process prototype:

- `mocha_process::RendererProcess` spawns `mocha_renderer`;
- browser and renderer communicate with `mocha_ipc`;
- the manager supports ping, render, shutdown, test crash, liveness checks, and
  respawn;
- tests cover spawn/ping, rendering a local document, renderer error responses,
  clean shutdown, crash detection, and respawn.

This is **not** a production multi-process browser. It is **not** site isolation,
not a network process, not a GPU process, and not an OS sandbox. The legacy
renderer command still calls `mocha_engine::render_url` directly for
`RenderDocument`, which means it can perform file/http loads with the same OS
privileges as the parent process.

M18 adds a separate prepared-document path with capability restrictions. That
path proves a browser-owned-I/O boundary, but it is still not an OS sandbox or
production site isolation.

The normal desktop and shell render paths remain single-process by default. M17
proves that renderer work can run in a child process and that the browser side
can recover from a renderer crash.
