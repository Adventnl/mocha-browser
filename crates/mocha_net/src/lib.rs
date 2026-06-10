//! Minimal resource loading for Mocha Browser.
//!
//! `mocha_net` loads documents from `file://`/local paths and `http://` URLs,
//! follows redirects, infers content type, and keeps a tiny in-memory cache. It
//! does **not** know about navigation history, HTML, CSS, layout, or painting.
//!
//! Networking scope (intentionally small): `GET` only; a hand-written blocking
//! HTTP/1.1 client over `std::net::TcpStream` (no keep-alive, chunked decoding,
//! or compression); **no TLS** — `https://` returns
//! [`MochaError::UnsupportedFeature`]. No cookies, auth, or proxy support.

mod cache;
mod content_type;
mod file;
mod http;

#[cfg(any(test, feature = "test-util"))]
pub mod test_server;

pub use cache::MemoryCache;
pub use content_type::{classify, ResourceType};

use mocha_error::{MochaError, MochaResult};
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

/// The default loader: handles `file`/`http`, with an in-memory cache for HTTP.
#[derive(Debug, Default)]
pub struct DefaultLoader {
    cache: MemoryCache,
}

impl DefaultLoader {
    /// Create a loader with an empty cache.
    pub fn new() -> DefaultLoader {
        DefaultLoader::default()
    }

    fn load_http(&mut self, request: &LoadRequest) -> MochaResult<ResourceResponse> {
        let key = request.url.normalized();
        if !request.bypass_cache {
            if let Some(cached) = self.cache.get(&key) {
                return Ok(cached);
            }
        }
        let response = http::fetch_with_redirects(&request.url)?;
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
}

impl ResourceLoader for DefaultLoader {
    fn load(&mut self, request: LoadRequest) -> MochaResult<ResourceResponse> {
        match request.url.scheme {
            Scheme::File => file::load_file(&request.url),
            Scheme::Http => self.load_http(&request),
            Scheme::Https => Err(MochaError::UnsupportedFeature(
                "https loading is not implemented in Milestone 4".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::{Reply, TestServer};
    use super::*;

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
    fn https_is_unsupported() {
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, "https://example.com/").unwrap_err();
        assert!(matches!(error, MochaError::UnsupportedFeature(_)));
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
    fn chunked_transfer_encoding_is_unsupported() {
        let server = TestServer::start(vec![(
            "/c".to_string(),
            Reply::Raw(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n"
                    .to_vec(),
            ),
        )]);
        let mut loader = DefaultLoader::new();
        let error = load(&mut loader, &server.url("/c")).unwrap_err();
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
