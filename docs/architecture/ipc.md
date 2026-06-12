# IPC Protocol (Milestone 17)

M17 adds `mocha_ipc`, a tiny typed protocol between the browser process and a
renderer process prototype.

Transport:

- newline-delimited frames over child-process stdin/stdout;
- tab-separated fields;
- hex-encoded strings;
- protocol version `1`;
- maximum frame size: 16 MiB.

Messages:

- browser → renderer: `Ping`, `RenderDocument`, `RenderHtml`, `Shutdown`,
  `CrashForTest`;
- renderer → browser: `Pong`, `Rendered`, `Error`, `Log`, `Goodbye`.

`Rendered` carries a lightweight `RendererPageSnapshot`: final URL, document
height, display-list length, and console output. It does not serialize the DOM,
layout tree, form state, images, or full display list.

Malformed frames, wrong protocol versions, oversized frames, unknown messages,
and invalid hex/UTF-8 produce clear `MochaError::Network` errors. This is a
prototype IPC format, not a production browser IPC layer.
