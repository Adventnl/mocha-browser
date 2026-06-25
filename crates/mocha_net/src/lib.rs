//! Minimal resource loading for Mocha Browser.
//!
//! `mocha_net` loads documents from `file://`/local paths and `http://` /
//! `https://` URLs, follows redirects, infers content type, and keeps a tiny
//! in-memory cache. It does **not** know about navigation history, HTML, CSS,
//! layout, or painting.
//!
//! Networking scope (intentionally small): `GET` only; a hand-written blocking
//! HTTP/1.1 client over `std::net::TcpStream` with `Transfer-Encoding: chunked`
//! and `Content-Encoding: gzip` decoding (the from-scratch `mocha_gzip` crate).
//! `https://` (Milestone 21) runs the same client over rustls with certificates
//! verified against the embedded Mozilla root store — invalid certificates are
//! clear errors with no override. No keep-alive, auth, proxies, HTTP/2-3, or
//! encodings beyond gzip. Cookie support is optional through [`CookieProvider`]
//! and [`DefaultLoader::load_with_cookies`]; the default load path still sends
//! no cookies.

mod cache;
mod content_type;
mod file;
mod http;
mod net_log;
mod tls;

#[cfg(any(test, feature = "test-util"))]
pub mod test_server;

pub use cache::MemoryCache;
pub use content_type::{classify, ResourceType};

use mocha_error::MochaResult;
use mocha_url::{Scheme, Url};

/// Maximum number of redirects followed before giving up.
pub const MAX_REDIRECTS: usize = 10;

/// The HTTP method. Milestone 4 only issues `GET`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMethod {
    /// HTTP GET.
    Get,
}

/// A request to load one resource.
#[derive(Debug, Clone)]
pub struct LoadRequest {
    /// The resource location.
    pub url: Url,
    /// The method (always `Get` today).
    pub method: LoadMethod,
    /// When `true`, skip the in-memory cache (used by reload / `--no-cache`).
    pub bypass_cache: bool,
}

impl LoadRequest {
    /// A cache-using `GET` request.
    pub fn get(url: Url) -> LoadRequest {
        LoadRequest {
            url,
            method: LoadMethod::Get,
            bypass_cache: false,
        }
    }

    /// A `GET` request that bypasses the cache.
    pub fn get_no_cache(url: Url) -> LoadRequest {
        LoadRequest {
            url,
            method: LoadMethod::Get,
            bypass_cache: true,
        }
    }
}

/// One response header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Header name (as received).
    pub name: String,
    /// Header value (trimmed).
    pub value: String,
}

/// A loaded resource.
#[derive(Debug, Clone)]
pub struct ResourceResponse {
    /// The URL the response actually came from (after redirects).
    pub final_url: Url,
    /// The HTTP status code, or `None` for `file://` loads.
    pub status: Option<u16>,
    /// Response headers (empty for `file://`).
    pub headers: Vec<Header>,
    /// The `Content-Type` value, if known.
    pub content_type: Option<String>,
    /// The raw response body.
    pub body: Vec<u8>,
    /// Whether this response was served from the in-memory cache.
    pub from_cache: bool,
}

impl ResourceResponse {
    /// Case-insensitive header lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        http::header(&self.headers, name)
    }

    /// Classify this response (HTML, CSS, text, binary, …).
    pub fn resource_type(&self) -> ResourceType {
        classify(self.content_type.as_deref(), &self.final_url)
    }
}

/// Something that can load a [`LoadRequest`] into a [`ResourceResponse`].
pub trait ResourceLoader {
    /// Load one resource.
    fn load(&mut self, request: LoadRequest) -> MochaResult<ResourceResponse>;
}

/// A cookie jar the HTTP client consults around a request (Milestone 15).
///
/// `mocha_net` does **not** depend on the storage layer: an embedder implements
/// this trait (e.g. over a `mocha_cookie::CookieJar` or `mocha_storage`'s cookie
/// store) and passes it to [`DefaultLoader::load_with_cookies`]. Cookies apply
/// only to `http`/`https` requests; `file://` never uses cookies.
pub trait CookieProvider {
    /// The `Cookie` request-header value to send for `url` (or `None`).
    fn cookie_header_for_request(&mut self, url: &Url, now_ms: i64) -> MochaResult<Option<String>>;
    /// Store any `Set-Cookie` response headers from a response to `url`.
    fn store_response_cookies(
        &mut self,
        url: &Url,
        headers: &[Header],
        now_ms: i64,
    ) -> MochaResult<()>;
}

/// The default loader: handles `file`/`http`/`https`, with an in-memory cache
/// for HTTP(S).
#[derive(Debug, Default)]
pub struct DefaultLoader {
    cache: MemoryCache,
    tls: tls::TlsClient,
}

impl DefaultLoader {
    /// Create a loader with an empty cache, trusting the embedded Mozilla CA
    /// roots for `https://`.
    pub fn new() -> DefaultLoader {
        DefaultLoader::default()
    }

    /// A loader that additionally trusts `certificate_der` for TLS. **Testing
    /// only**: this exists so integration tests can talk to the localhost
    /// [`test_server::TestServer::start_tls`] server; production paths always
    /// use [`DefaultLoader::new`].
    #[cfg(any(test, feature = "test-util"))]
    pub fn with_extra_tls_root(certificate_der: &[u8]) -> MochaResult<DefaultLoader> {
        Ok(DefaultLoader {
            cache: MemoryCache::default(),
            tls: tls::TlsClient::with_extra_root(certificate_der)?,
        })
    }

    fn load_http(&mut self, request: &LoadRequest) -> MochaResult<ResourceResponse> {
        let key = request.url.normalized();
        if !request.bypass_cache {
            if let Some(cached) = self.cache.get(&key) {
                return Ok(cached);
            }
        }
        let response = http::fetch_with_redirects(&request.url, &self.tls)?;
        // Only cache successful responses, keyed by the *final* URL so that a
        // later direct load of a redirect's destination (e.g. a back/forward
        // navigation, whose history stores the final URL) hits the cache.
        // Errors, non-200s, and the redirects themselves are not cached.
        if response.status == Some(200) {
            self.cache
                .insert(response.final_url.normalized(), response.clone());
        }
        Ok(response)
    }

    /// Load `request`, consulting `cookies` to attach a `Cookie` header and store
    /// any `Set-Cookie` responses (Milestone 15). This path **bypasses the cache**
    /// (so the live `Cookie` header is always sent) and applies cookies per
    /// redirect hop. `file://` loads ignore cookies.
    pub fn load_with_cookies(
        &mut self,
        request: LoadRequest,
        cookies: &mut dyn CookieProvider,
        now_ms: i64,
    ) -> MochaResult<ResourceResponse> {
        match request.url.scheme {
            Scheme::File => file::load_file(&request.url),
            Scheme::Http | Scheme::Https => {
                http::fetch_with_redirects_cookies(&request.url, &self.tls, Some(cookies), now_ms)
            }
        }
    }
}

impl ResourceLoader for DefaultLoader {
    fn load(&mut self, request: LoadRequest) -> MochaResult<ResourceResponse> {
        match request.url.scheme {
            Scheme::File => file::load_file(&request.url),
            Scheme::Http | Scheme::Https => self.load_http(&request),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::{Reply, TestServer};
    use super::*;
    use mocha_error::MochaError;

    fn load(loader: &mut DefaultLoader, url: &str) -> MochaResult<ResourceResponse> {
        loader.load(LoadRequest::get(Url::parse(url).unwrap()))
    }

    #[test]
    fn load_local_html_file() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/basic/index.html"
        );
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, path).unwrap();
        assert_eq!(response.content_type.as_deref(), Some("text/html"));
        assert_eq!(response.resource_type(), ResourceType::Html);
        assert!(!response.body.is_empty());
    }

    #[test]
    fn missing_local_file_errors_clearly() {
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, "does/not/exist.html").unwrap_err();
        assert!(matches!(error, MochaError::Io(_)));
    }

    #[test]
    fn https_loads_over_tls() {
        let server = TestServer::start_tls(vec![(
            "/index.html".to_string(),
            Reply::Html("<html><body><p>secure</p></body></html>".to_string()),
        )]);
        let mut loader = DefaultLoader::with_extra_tls_root(TestServer::tls_certificate_der())
            .expect("test trust root");
        let response = load(&mut loader, &server.url("/index.html")).unwrap();
        assert_eq!(response.status, Some(200));
        assert_eq!(response.final_url.scheme, Scheme::Https);
        assert!(String::from_utf8_lossy(&response.body).contains("secure"));
    }

    #[test]
    fn https_self_signed_certificate_is_rejected_by_default() {
        // The default loader trusts only the Mozilla roots, so the test
        // server's self-signed certificate must fail — with no override.
        let server = TestServer::start_tls(vec![(
            "/index.html".to_string(),
            Reply::Html("<html></html>".to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/index.html")).unwrap_err();
        match error {
            MochaError::Network(message) => {
                assert!(
                    message.contains("certificate"),
                    "expected a certificate error, got: {message}"
                );
            }
            other => panic!("expected Network error, got {other:?}"),
        }
    }

    #[test]
    fn http_redirect_to_https_is_followed() {
        let tls_server = TestServer::start_tls(vec![(
            "/dest.html".to_string(),
            Reply::Html("<html><body>upgraded</body></html>".to_string()),
        )]);
        let plain_server = TestServer::start(vec![(
            "/start".to_string(),
            Reply::Redirect {
                status: 301,
                location: tls_server.url("/dest.html"),
            },
        )]);
        let mut loader = DefaultLoader::with_extra_tls_root(TestServer::tls_certificate_der())
            .expect("test trust root");
        let response = load(&mut loader, &plain_server.url("/start")).unwrap();
        assert_eq!(response.status, Some(200));
        assert_eq!(response.final_url.scheme, Scheme::Https);
        assert!(String::from_utf8_lossy(&response.body).contains("upgraded"));
    }

    #[test]
    fn chunked_response_is_decoded() {
        let body = "<html><body><p>chunked transfer works end to end</p></body></html>";
        let server = TestServer::start(vec![(
            "/c.html".to_string(),
            Reply::ChunkedHtml(body.to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/c.html")).unwrap();
        assert_eq!(response.status, Some(200));
        assert_eq!(String::from_utf8_lossy(&response.body), body);
    }

    #[test]
    fn gzip_response_is_decoded() {
        let body = "<html><body><p>gzip content-encoding works</p></body></html>";
        let server = TestServer::start(vec![(
            "/g.html".to_string(),
            Reply::GzipHtml(body.to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/g.html")).unwrap();
        assert_eq!(response.status, Some(200));
        assert_eq!(String::from_utf8_lossy(&response.body), body);
    }

    #[test]
    fn corrupt_gzip_body_errors_clearly() {
        let mut compressed = mocha_gzip::gzip_compress_stored(b"<html></html>");
        let length = compressed.len();
        compressed[length - 6] ^= 0xFF; // corrupt the CRC
        let raw = [
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {length}\r\nContent-Encoding: gzip\r\nConnection: close\r\n\r\n"
            )
            .into_bytes(),
            compressed,
        ]
        .concat();
        let server = TestServer::start(vec![("/bad.html".to_string(), Reply::Raw(raw))]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/bad.html")).unwrap_err();
        assert!(matches!(error, MochaError::Decompression(_)));
    }

    #[test]
    fn deflate_response_is_decoded() {
        let body = "<html><body><p>deflate content-encoding works</p></body></html>";
        let server = TestServer::start(vec![(
            "/d.html".to_string(),
            Reply::DeflateHtml(body.to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/d.html")).unwrap();
        assert_eq!(response.status, Some(200));
        assert_eq!(String::from_utf8_lossy(&response.body), body);
    }

    #[test]
    fn corrupt_deflate_body_errors_clearly() {
        let mut compressed = mocha_gzip::zlib_compress_stored(b"<html></html>");
        let length = compressed.len();
        compressed[length - 1] ^= 0xFF; // corrupt the Adler-32
        let raw = [
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {length}\r\nContent-Encoding: deflate\r\nConnection: close\r\n\r\n"
            )
            .into_bytes(),
            compressed,
        ]
        .concat();
        let server = TestServer::start(vec![("/bad.html".to_string(), Reply::Raw(raw))]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/bad.html")).unwrap_err();
        assert!(matches!(error, MochaError::Decompression(_)));
    }

    #[test]
    fn unsupported_content_encoding_errors_clearly() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 2\r\nContent-Encoding: br\r\nConnection: close\r\n\r\nxx".to_vec();
        let server = TestServer::start(vec![("/br.html".to_string(), Reply::Raw(raw))]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/br.html")).unwrap_err();
        match error {
            MochaError::UnsupportedFeature(message) => assert!(message.contains("'br'")),
            other => panic!("expected UnsupportedFeature, got {other:?}"),
        }
    }

    #[test]
    fn truncated_content_length_errors_clearly() {
        let raw =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort".to_vec();
        let server = TestServer::start(vec![("/t.html".to_string(), Reply::Raw(raw))]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/t.html")).unwrap_err();
        match error {
            MochaError::Network(message) => assert!(message.contains("truncated")),
            other => panic!("expected Network error, got {other:?}"),
        }
    }

    #[test]
    fn http_200_loads_body_and_content_type() {
        let server = TestServer::start(vec![(
            "/index.html".to_string(),
            Reply::Html("<html><body><p>hi</p></body></html>".to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/index.html")).unwrap();
        assert_eq!(response.status, Some(200));
        assert_eq!(response.resource_type(), ResourceType::Html);
        assert!(String::from_utf8_lossy(&response.body).contains("hi"));
    }

    #[test]
    fn text_plain_is_classified_text_not_html() {
        let server = TestServer::start(vec![(
            "/note.txt".to_string(),
            Reply::Text("just text".to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/note.txt")).unwrap();
        assert_eq!(response.resource_type(), ResourceType::Text);
    }

    #[test]
    fn missing_content_type_with_html_extension_is_html() {
        let server = TestServer::start(vec![(
            "/page.html".to_string(),
            Reply::NoContentType("<html></html>".to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/page.html")).unwrap();
        assert_eq!(response.content_type, None);
        assert_eq!(response.resource_type(), ResourceType::Html);
    }

    #[test]
    fn absolute_redirect_is_followed() {
        let server = TestServer::start(vec![
            (
                "/start".to_string(),
                Reply::Redirect {
                    status: 302,
                    location: "/dest.html".to_string(),
                },
            ),
            (
                "/dest.html".to_string(),
                Reply::Html("<html><body>dest</body></html>".to_string()),
            ),
        ]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/start")).unwrap();
        assert_eq!(response.status, Some(200));
        assert!(response.final_url.path == "/dest.html");
        assert!(String::from_utf8_lossy(&response.body).contains("dest"));
    }

    #[test]
    fn full_url_redirect_is_followed() {
        // `RedirectToSelf` makes the server emit an absolute `Location` pointing
        // back at its own (port-dependent) authority.
        let server = TestServer::start(vec![
            (
                "/start".to_string(),
                Reply::RedirectToSelf {
                    status: 301,
                    path: "/dest.html".to_string(),
                },
            ),
            (
                "/dest.html".to_string(),
                Reply::Html("<html><body>dest</body></html>".to_string()),
            ),
        ]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/start")).unwrap();
        assert_eq!(response.status, Some(200));
        assert_eq!(response.final_url.path, "/dest.html");
    }

    #[test]
    fn redirect_loop_hits_limit() {
        let server = TestServer::start(vec![(
            "/loop".to_string(),
            Reply::Redirect {
                status: 302,
                location: "/loop".to_string(),
            },
        )]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/loop")).unwrap_err();
        match error {
            MochaError::Network(message) => assert!(message.contains("too many redirects")),
            other => panic!("expected Network error, got {other:?}"),
        }
    }

    #[test]
    fn redirect_without_location_errors_clearly() {
        let server = TestServer::start(vec![(
            "/r".to_string(),
            Reply::Raw(b"HTTP/1.1 302 Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
        )]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/r")).unwrap_err();
        match error {
            MochaError::Network(message) => assert!(message.contains("Location")),
            other => panic!("expected Network error, got {other:?}"),
        }
    }

    #[test]
    fn raw_chunked_response_is_decoded() {
        let server = TestServer::start(vec![(
            "/c".to_string(),
            Reply::Raw(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n"
                    .to_vec(),
            ),
        )]);
        let mut loader = DefaultLoader::new();
        let response = load(&mut loader, &server.url("/c")).unwrap();
        assert_eq!(response.body, b"abc");
    }

    #[test]
    fn unsupported_transfer_encoding_errors_clearly() {
        let server = TestServer::start(vec![(
            "/te".to_string(),
            Reply::Raw(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\nxx".to_vec()),
        )]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/te")).unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
    }

    #[test]
    fn redirect_to_file_is_rejected() {
        let server = TestServer::start(vec![(
            "/evil".to_string(),
            Reply::Redirect {
                status: 302,
                location: "file:///etc/passwd".to_string(),
            },
        )]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/evil")).unwrap_err();
        assert!(matches!(error, MochaError::Network(_)));
    }

    #[test]
    fn second_load_comes_from_cache_and_no_cache_bypasses() {
        let server = TestServer::start(vec![(
            "/cached.html".to_string(),
            Reply::Html("<html></html>".to_string()),
        )]);
        let mut loader = DefaultLoader::new();
        let url = Url::parse(&server.url("/cached.html")).unwrap();

        let first = loader.load(LoadRequest::get(url.clone())).unwrap();
        assert!(!first.from_cache);
        let second = loader.load(LoadRequest::get(url.clone())).unwrap();
        assert!(second.from_cache, "second load should be cached");
        let bypass = loader.load(LoadRequest::get_no_cache(url)).unwrap();
        assert!(!bypass.from_cache, "no-cache should bypass the cache");
    }
}

#[cfg(test)]
mod cookie_tests {
    use super::test_server::{Reply, TestServer};
    use super::*;
    use mocha_cookie::CookieJar;

    /// A `CookieProvider` over an in-memory `mocha_cookie::CookieJar`.
    struct JarProvider(CookieJar);

    impl CookieProvider for JarProvider {
        fn cookie_header_for_request(
            &mut self,
            url: &Url,
            now_ms: i64,
        ) -> MochaResult<Option<String>> {
            Ok(self.0.cookie_header_for_request(url, now_ms))
        }

        fn store_response_cookies(
            &mut self,
            url: &Url,
            headers: &[Header],
            now_ms: i64,
        ) -> MochaResult<()> {
            for header in headers {
                if header.name.eq_ignore_ascii_case("set-cookie") {
                    // Ignore a single malformed Set-Cookie rather than aborting.
                    let _ = self.0.store_set_cookie(&header.value, url, now_ms);
                }
            }
            Ok(())
        }
    }

    fn get(loader: &mut DefaultLoader, jar: &mut JarProvider, url: &str, now_ms: i64) -> String {
        let request = LoadRequest::get(Url::parse(url).unwrap());
        let response = loader.load_with_cookies(request, jar, now_ms).unwrap();
        String::from_utf8_lossy(&response.body).into_owned()
    }

    #[test]
    fn server_sets_cookie_and_second_request_sends_it() {
        let server = TestServer::start(vec![
            (
                "/set".to_string(),
                Reply::SetCookies {
                    set_cookie: vec!["sid=abc; Path=/".to_string()],
                    body: "ok".to_string(),
                },
            ),
            ("/echo".to_string(), Reply::EchoCookie),
        ]);
        let mut loader = DefaultLoader::new();
        let mut jar = JarProvider(CookieJar::new());

        // First request: server sets the cookie.
        get(&mut loader, &mut jar, &server.url("/set"), 0);
        // Second request: the client sends the stored cookie back.
        let echoed = get(&mut loader, &mut jar, &server.url("/echo"), 0);
        assert_eq!(echoed, "sid=abc");
    }

    #[test]
    fn multiple_set_cookie_headers_are_stored() {
        let server = TestServer::start(vec![
            (
                "/set".to_string(),
                Reply::SetCookies {
                    set_cookie: vec!["a=1; Path=/".to_string(), "b=2; Path=/".to_string()],
                    body: "ok".to_string(),
                },
            ),
            ("/echo".to_string(), Reply::EchoCookie),
        ]);
        let mut loader = DefaultLoader::new();
        let mut jar = JarProvider(CookieJar::new());
        get(&mut loader, &mut jar, &server.url("/set"), 0);
        let echoed = get(&mut loader, &mut jar, &server.url("/echo"), 0);
        // Both cookies are sent (path-equal → ordered by name).
        assert_eq!(echoed, "a=1; b=2");
    }

    #[test]
    fn expired_cookie_is_not_sent() {
        let server = TestServer::start(vec![
            (
                "/set".to_string(),
                Reply::SetCookies {
                    set_cookie: vec!["a=1; Max-Age=10".to_string()],
                    body: "ok".to_string(),
                },
            ),
            ("/echo".to_string(), Reply::EchoCookie),
        ]);
        let mut loader = DefaultLoader::new();
        let mut jar = JarProvider(CookieJar::new());
        get(&mut loader, &mut jar, &server.url("/set"), 1000);
        // Long after expiry: no Cookie header.
        let echoed = get(&mut loader, &mut jar, &server.url("/echo"), 999_999);
        assert_eq!(echoed, "");
    }

    #[test]
    fn secure_cookie_not_sent_over_http() {
        let server = TestServer::start(vec![
            (
                "/set".to_string(),
                Reply::SetCookies {
                    set_cookie: vec!["a=1; Secure".to_string()],
                    body: "ok".to_string(),
                },
            ),
            ("/echo".to_string(), Reply::EchoCookie),
        ]);
        let mut loader = DefaultLoader::new();
        let mut jar = JarProvider(CookieJar::new());
        get(&mut loader, &mut jar, &server.url("/set"), 0);
        let echoed = get(&mut loader, &mut jar, &server.url("/echo"), 0);
        assert_eq!(echoed, "", "Secure cookie is not sent over http");
    }

    #[test]
    fn file_url_does_not_use_cookies() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/basic/index.html"
        );
        let mut loader = DefaultLoader::new();
        let mut jar = JarProvider(CookieJar::new());
        // A file:// load works and simply ignores the cookie provider.
        let request = LoadRequest::get(Url::parse(path).unwrap());
        let response = loader.load_with_cookies(request, &mut jar, 0).unwrap();
        assert!(!response.body.is_empty());
    }
}
