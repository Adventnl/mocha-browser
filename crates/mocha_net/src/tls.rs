//! TLS support for `https://` loads (Milestone 21), built on rustls.
//!
//! Mocha never hand-rolls cryptography: rustls performs the handshake and
//! record layer, and certificate chains are verified against the embedded
//! Mozilla root store (`webpki-roots`) — there is no "ignore certificate
//! errors" mode. The HTTP protocol on top of the TLS stream stays the
//! hand-written client in [`crate::http`].

use std::fmt;
use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use mocha_error::{MochaError, MochaResult};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};

use crate::http::read_response_bytes;

const TIMEOUT: Duration = Duration::from_secs(15);

/// A reusable TLS client: one verified `ClientConfig` shared by every
/// `https://` request a loader makes.
pub(crate) struct TlsClient {
    config: Arc<ClientConfig>,
}

impl fmt::Debug for TlsClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsClient").finish_non_exhaustive()
    }
}

impl Default for TlsClient {
    fn default() -> TlsClient {
        TlsClient::new()
    }
}

impl TlsClient {
    /// A client trusting the embedded Mozilla CA roots **and** the operating
    /// system's trust store.
    ///
    /// Like a mainstream browser, Mocha trusts the OS certificate store in
    /// addition to the embedded Mozilla roots. This is what makes real sites
    /// load behind corporate HTTPS proxies and TLS-inspecting antivirus (Zscaler,
    /// Kaspersky, ESET, …), which present certificates signed by a CA they inject
    /// into the OS store; with only the embedded roots those connections fail
    /// with `UnknownIssuer`. There is still no "ignore certificate errors" mode:
    /// every chain must validate against a trusted root.
    pub(crate) fn new() -> TlsClient {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        add_native_roots(&mut roots);
        TlsClient::from_root_store(roots)
    }

    /// A client that **additionally** trusts `extra_root_der` (a DER X.509
    /// certificate). Test-only: lets integration tests trust the localhost
    /// test server's self-signed certificate. Production code paths always use
    /// [`TlsClient::new`].
    #[cfg(any(test, feature = "test-util"))]
    pub(crate) fn with_extra_root(extra_root_der: &[u8]) -> MochaResult<TlsClient> {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        roots
            .add(rustls::pki_types::CertificateDer::from(
                extra_root_der.to_vec(),
            ))
            .map_err(|error| {
                MochaError::Network(format!("invalid extra trust root certificate: {error}"))
            })?;
        Ok(TlsClient::from_root_store(roots))
    }

    fn from_root_store(roots: RootCertStore) -> TlsClient {
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        TlsClient {
            config: Arc::new(config),
        }
    }

    /// Connect to `host:port`, handshake (verifying the server certificate for
    /// `host`), send `request`, and read the response until the peer closes.
    pub(crate) fn exchange(&self, host: &str, port: u16, request: &[u8]) -> MochaResult<Vec<u8>> {
        let server_name = ServerName::try_from(host.to_string()).map_err(|_| {
            MochaError::InvalidUrl(format!("{host:?} is not a valid TLS server name"))
        })?;
        let mut connection =
            ClientConnection::new(Arc::clone(&self.config), server_name).map_err(|error| {
                MochaError::Network(format!("tls client setup for {host} failed: {error}"))
            })?;
        let mut tcp = TcpStream::connect((host, port)).map_err(|error| {
            MochaError::Network(format!("cannot connect to {host}:{port}: {error}"))
        })?;
        crate::net_log::trace(format!(
            "  TCP connected to {}",
            tcp.peer_addr()
                .map(|a| a.to_string())
                .unwrap_or_else(|_| format!("{host}:{port}"))
        ));
        tcp.set_read_timeout(Some(TIMEOUT)).ok();
        tcp.set_write_timeout(Some(TIMEOUT)).ok();

        let mut stream = rustls::Stream::new(&mut connection, &mut tcp);
        // The handshake (including certificate verification) runs inside the
        // first write; its errors surface here with rustls's message intact,
        // e.g. "invalid peer certificate: UnknownIssuer".
        stream.write_all(request).map_err(|error| {
            MochaError::Network(format!("tls connection to {host}:{port} failed: {error}"))
        })?;
        crate::net_log::trace(format!(
            "  TLS handshake ok ({}), certificate verified",
            stream
                .conn
                .protocol_version()
                .map(|v| format!("{v:?}"))
                .unwrap_or_else(|| "TLS".to_string())
        ));
        read_response_bytes(&mut stream)
            .map_err(|error| MochaError::Network(format!("tls read from {host}:{port}: {error}")))
    }
}

/// Add the operating system's trust store to `roots` (best effort). Failures to
/// read the store or individual certificates are ignored: the embedded Mozilla
/// roots remain as a baseline. An optional `MOCHA_EXTRA_CA_FILE` environment
/// variable points at a PEM bundle of additional roots to trust (for unusual
/// proxy setups, and used by the test harness here).
fn add_native_roots(roots: &mut RootCertStore) {
    let result = rustls_native_certs::load_native_certs();
    let _ = roots.add_parsable_certificates(result.certs);

    if let Some(path) = std::env::var_os("MOCHA_EXTRA_CA_FILE") {
        if let Ok(pem) = std::fs::read(&path) {
            for der in extract_pem_certificates(&pem) {
                let _ = roots.add(rustls::pki_types::CertificateDer::from(der));
            }
        }
    }
}

/// Extract DER certificate bodies from a PEM bundle: the base64 between each
/// BEGIN/END CERTIFICATE pair (no PEM-parsing dependency).
fn extract_pem_certificates(pem: &[u8]) -> Vec<Vec<u8>> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let text = String::from_utf8_lossy(pem);
    let mut out = Vec::new();
    let mut rest = text.as_ref();
    while let Some(start) = rest.find(BEGIN) {
        rest = &rest[start + BEGIN.len()..];
        let Some(end) = rest.find(END) else { break };
        let body: String = rest[..end].split_whitespace().collect();
        if let Some(der) = base64_decode(&body) {
            out.push(der);
        }
        rest = &rest[end + END.len()..];
    }
    out
}

/// Minimal standard-alphabet base64 decoder for PEM bodies.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' {
            break;
        }
        let Some(v) = val(c) else { continue };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod native_root_tests {
    use super::*;

    #[test]
    fn base64_decodes_known_vector() {
        assert_eq!(base64_decode("TWE=").unwrap(), b"Ma");
        assert_eq!(base64_decode("TWFu").unwrap(), b"Man");
    }

    #[test]
    fn extracts_multiple_pem_blocks() {
        // Two tiny "certificates" (not real DER, just base64 round-trips).
        let pem = b"-----BEGIN CERTIFICATE-----\nTWFu\n-----END CERTIFICATE-----\n\
                    -----BEGIN CERTIFICATE-----\nTWE=\n-----END CERTIFICATE-----\n";
        let certs = extract_pem_certificates(pem);
        assert_eq!(certs.len(), 2);
        assert_eq!(certs[0], b"Man");
        assert_eq!(certs[1], b"Ma");
    }

    #[test]
    fn new_client_builds_with_native_roots() {
        // Smoke test: constructing the default client (which loads OS roots)
        // must not panic and must produce a usable config.
        let _client = TlsClient::new();
    }
}
