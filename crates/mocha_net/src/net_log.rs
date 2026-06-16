//! Optional, human-readable network tracing.
//!
//! Set the `MOCHA_NET_LOG` environment variable to print every step of a real
//! request — DNS resolution, the TCP connection, the TLS handshake, the HTTP
//! request line, and the response status/size — to stderr. This is a window
//! onto the genuine network stack (`std::net` sockets + rustls + a hand-written
//! HTTP/1.1 client); it changes no behavior and is silent unless enabled.

use std::sync::OnceLock;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("MOCHA_NET_LOG").is_some())
}

/// Print one `net:` trace line when tracing is enabled.
pub(crate) fn trace(message: impl AsRef<str>) {
    if enabled() {
        eprintln!("net: {}", message.as_ref());
    }
}

/// Whether tracing is on (so callers can skip extra work like a DNS lookup).
pub(crate) fn is_on() -> bool {
    enabled()
}
