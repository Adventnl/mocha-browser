//! A tiny, deterministic localhost HTTP server for tests.
//!
//! It binds `127.0.0.1:0`, serves a fixed routing table on a background thread,
//! and never touches the network. It is **only** a testing utility (gated behind
//! the `test-util` feature for use by other crates' integration tests) — it is
//! not part of the browser and production code must not depend on it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

/// What a route replies with.
#[derive(Debug, Clone)]
pub enum Reply {
    /// `200 text/html`.
    Html(String),
    /// `200 text/plain`.
    Text(String),
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
}

/// A running test server. Drop it to leak the background thread (fine for tests;
/// the process exits at test end).
pub struct TestServer {
    port: u16,
}

impl TestServer {
    /// Start a server with the given `(path, reply)` routes.
    pub fn start(routes: Vec<(String, Reply)>) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().expect("local addr").port();
        let authority = format!("127.0.0.1:{port}");

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = routes.clone();
                let authority = authority.clone();
                // Handle each connection on its own thread so redirect chains
                // (which open a fresh connection per hop) never deadlock.
                thread::spawn(move || handle(stream, &routes, &authority));
            }
        });

        TestServer { port }
    }

    /// This server's port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Build an `http://127.0.0.1:<port><path>` URL for this server.
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

fn handle(mut stream: TcpStream, routes: &[(String, Reply)], authority: &str) {
    let mut buffer = [0_u8; 2048];
    let read = match stream.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return,
    };
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|target| target.split('?').next().unwrap_or(target))
        .unwrap_or("/")
        .to_string();

    let response = match routes.iter().find(|(route, _)| *route == path) {
        Some((_, reply)) => render(reply, authority),
        None => http_response(404, "Not Found", Some("text/plain"), &[], b"not found"),
    };
    let _ = stream.write_all(&response);
}

fn render(reply: &Reply, authority: &str) -> Vec<u8> {
    match reply {
        Reply::Html(body) => http_response(
            200,
            "OK",
            Some("text/html; charset=utf-8"),
            &[],
            body.as_bytes(),
        ),
        Reply::Text(body) => http_response(200, "OK", Some("text/plain"), &[], body.as_bytes()),
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
