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
    /// A client trusting the embedded Mozilla CA roots.
    pub(crate) fn new() -> TlsClient {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
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
        tcp.set_read_timeout(Some(TIMEOUT)).ok();
        tcp.set_write_timeout(Some(TIMEOUT)).ok();

        let mut stream = rustls::Stream::new(&mut connection, &mut tcp);
        // The handshake (including certificate verification) runs inside the
        // first write; its errors surface here with rustls's message intact,
        // e.g. "invalid peer certificate: UnknownIssuer".
        stream.write_all(request).map_err(|error| {
            MochaError::Network(format!("tls connection to {host}:{port} failed: {error}"))
        })?;
        read_response_bytes(&mut stream)
            .map_err(|error| MochaError::Network(format!("tls read from {host}:{port}: {error}")))
    }
}
