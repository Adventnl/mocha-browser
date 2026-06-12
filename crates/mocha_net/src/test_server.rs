//! A tiny, deterministic localhost HTTP/HTTPS server for tests.
//!
//! It binds `127.0.0.1:0`, serves a fixed routing table on a background thread,
//! and never touches the network. [`TestServer::start_tls`] serves the same
//! routes over rustls using the committed self-signed localhost certificate
//! (`testdata/`); clients opt in to trusting it via
//! [`crate::DefaultLoader::with_extra_tls_root`]. It is **only** a testing
//! utility (gated behind the `test-util` feature for use by other crates'
//! integration tests) — it is not part of the browser and production code must
//! not depend on it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

/// What a route replies with.
#[derive(Debug, Clone)]
pub enum Reply {
    /// `200 text/html`.
    Html(String),
    /// `200 text/plain`.
    Text(String),
    /// `200 text/css`.
    Css(String),
    /// `200` with the given content type and raw bytes (e.g. an image).
    Bytes {
        /// The `Content-Type` header value.
        content_type: String,
        /// The response body bytes.
        body: Vec<u8>,
    },
    /// `200` with a body but **no** `Content-Type` header.
    NoContentType(String),
    /// A redirect with the given status and (possibly relative) `Location`.
    Redirect {
        /// Redirect status code (e.g. 301/302).
        status: u16,
        /// The `Location` header value.
        location: String,
    },
    /// A redirect to an absolute URL pointing at this server's own authority,
    /// with the given path (useful for testing absolute-URL redirects).
    RedirectToSelf {
        /// Redirect status code.
        status: u16,
        /// The path to redirect to on this server.
        path: String,
    },
    /// Send these exact bytes verbatim (for malformed/edge-case responses).
    Raw(Vec<u8>),
    /// `200 text/html` with one or more `Set-Cookie` response headers.
    SetCookies {
        /// Each `Set-Cookie` header value.
        set_cookie: Vec<String>,
        /// The response body.
        body: String,
    },
    /// `200 text/plain` whose body is the request's `Cookie` header value (empty
    /// if none was sent). Lets a test observe what the client sent.
    EchoCookie,
    /// `200 text/html` sent with `Transfer-Encoding: chunked` framing, split
    /// into small chunks (exercises the client's chunked decoder end to end).
    ChunkedHtml(String),
    /// `200 text/html` with the body gzip-compressed (stored blocks via
    /// `mocha_gzip::gzip_compress_stored`) and `Content-Encoding: gzip`.
    GzipHtml(String),
}

/// A running test server. Drop it to leak the background thread (fine for tests;
/// the process exits at test end).
pub struct TestServer {
    port: u16,
    scheme: &'static str,
}

impl TestServer {
    /// Start a plain-HTTP server with the given `(path, reply)` routes.
    pub fn start(routes: Vec<(String, Reply)>) -> TestServer {
        TestServer::start_inner(routes, None)
    }

    /// Start an HTTPS server with the given routes, using the committed
    /// self-signed localhost test certificate.
    pub fn start_tls(routes: Vec<(String, Reply)>) -> TestServer {
        TestServer::start_inner(routes, Some(tls_server_config()))
    }

    fn start_inner(
        routes: Vec<(String, Reply)>,
        tls: Option<Arc<rustls::ServerConfig>>,
    ) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().expect("local addr").port();
        let scheme = if tls.is_some() { "https" } else { "http" };
        let authority = format!("127.0.0.1:{port}");

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = routes.clone();
                let authority = authority.clone();
                let tls = tls.clone();
                // Handle each connection on its own thread so redirect chains
                // (which open a fresh connection per hop) never deadlock.
                thread::spawn(move || match tls {
                    Some(config) => handle_tls(stream, &routes, &authority, config),
                    None => handle_plain(stream, &routes, &authority),
                });
            }
        });

        TestServer { port, scheme }
    }

    /// This server's port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Build an `http(s)://127.0.0.1:<port><path>` URL for this server.
    pub fn url(&self, path: &str) -> String {
        format!("{}://127.0.0.1:{}{}", self.scheme, self.port, path)
    }

    /// The DER X.509 certificate the TLS test server presents. Tests pass this
    /// to [`crate::DefaultLoader::with_extra_tls_root`] to trust it.
    pub fn tls_certificate_der() -> &'static [u8] {
        include_bytes!("../testdata/localhost-cert.der")
    }
}

fn tls_server_config() -> Arc<rustls::ServerConfig> {
    let cert = rustls::pki_types::CertificateDer::from(TestServer::tls_certificate_der().to_vec());
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        include_bytes!("../testdata/localhost-key.der").to_vec(),
    ));
    Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("test server tls config"),
    )
}

fn handle_plain(mut stream: TcpStream, routes: &[(String, Reply)], authority: &str) {
    if let Some(response) = respond(&mut stream, routes, authority) {
        let _ = stream.write_all(&response);
    }
}

fn handle_tls(
    mut tcp: TcpStream,
    routes: &[(String, Reply)],
    authority: &str,
    config: Arc<rustls::ServerConfig>,
) {
    let mut connection = match rustls::ServerConnection::new(config) {
        Ok(connection) => connection,
        Err(_) => return,
    };
    let mut stream = rustls::Stream::new(&mut connection, &mut tcp);
    if let Some(response) = respond(&mut stream, routes, authority) {
        let _ = stream.write_all(&response);
    }
    // A clean TLS shutdown so the client does not see a truncated stream.
    connection.send_close_notify();
    let _ = connection.complete_io(&mut tcp);
}

/// Read one request from `stream` and render the matching route's response.
fn respond<S: Read>(
    stream: &mut S,
    routes: &[(String, Reply)],
    authority: &str,
) -> Option<Vec<u8>> {
    let mut buffer = [0_u8; 2048];
    let read = stream.read(&mut buffer).ok()?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|target| target.split('?').next().unwrap_or(target))
        .unwrap_or("/")
        .to_string();
    // The request's Cookie header value (if any), for Reply::EchoCookie.
    let cookie_header = request
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(n, _)| n.trim().eq_ignore_ascii_case("cookie"))
        })
        .map(|(_, value)| value.trim().to_string());

    Some(match routes.iter().find(|(route, _)| *route == path) {
        Some((_, reply)) => render(reply, authority, cookie_header.as_deref()),
        None => http_response(404, "Not Found", Some("text/plain"), &[], b"not found"),
    })
}

fn render(reply: &Reply, authority: &str, cookie_header: Option<&str>) -> Vec<u8> {
    match reply {
        Reply::Html(body) => http_response(
            200,
            "OK",
            Some("text/html; charset=utf-8"),
            &[],
            body.as_bytes(),
        ),
        Reply::Text(body) => http_response(200, "OK", Some("text/plain"), &[], body.as_bytes()),
        Reply::Css(body) => http_response(200, "OK", Some("text/css"), &[], body.as_bytes()),
        Reply::Bytes { content_type, body } => {
            http_response(200, "OK", Some(content_type), &[], body)
        }
        Reply::NoContentType(body) => http_response(200, "OK", None, &[], body.as_bytes()),
        Reply::Redirect { status, location } => http_response(
            *status,
            "Redirect",
            Some("text/html"),
            &[("Location", location)],
            b"",
        ),
        Reply::RedirectToSelf { status, path } => {
            let location = format!("http://{authority}{path}");
            http_response(
                *status,
                "Redirect",
                Some("text/html"),
                &[("Location", &location)],
                b"",
            )
        }
        Reply::Raw(bytes) => bytes.clone(),
        Reply::SetCookies { set_cookie, body } => {
            let headers: Vec<(&str, &str)> = set_cookie
                .iter()
                .map(|value| ("Set-Cookie", value.as_str()))
                .collect();
            http_response(
                200,
                "OK",
                Some("text/html; charset=utf-8"),
                &headers,
                body.as_bytes(),
            )
        }
        Reply::EchoCookie => http_response(
            200,
            "OK",
            Some("text/plain"),
            &[],
            cookie_header.unwrap_or("").as_bytes(),
        ),
        Reply::ChunkedHtml(body) => {
            let mut response = String::from("HTTP/1.1 200 OK\r\n");
            response.push_str("Content-Type: text/html; charset=utf-8\r\n");
            response.push_str("Transfer-Encoding: chunked\r\n");
            response.push_str("Connection: close\r\n\r\n");
            let mut bytes = response.into_bytes();
            for chunk in body.as_bytes().chunks(7) {
                bytes.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
                bytes.extend_from_slice(chunk);
                bytes.extend_from_slice(b"\r\n");
            }
            bytes.extend_from_slice(b"0\r\n\r\n");
            bytes
        }
        Reply::GzipHtml(body) => {
            let compressed = mocha_gzip::gzip_compress_stored(body.as_bytes());
            http_response(
                200,
                "OK",
                Some("text/html; charset=utf-8"),
                &[("Content-Encoding", "gzip")],
                &compressed,
            )
        }
    }
}

fn http_response(
    status: u16,
    reason: &str,
    content_type: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 {status} {reason}\r\n");
    response.push_str(&format!("Content-Length: {}\r\n", body.len()));
    if let Some(content_type) = content_type {
        response.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    for (name, value) in extra_headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("Connection: close\r\n\r\n");

    let mut bytes = response.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
