//! A minimal web [`Origin`] model (Milestone 15).
//!
//! An origin is the `(scheme, host, port)` tuple that web storage and cookies are
//! scoped to. This is **not** the full HTML origin concept: there are no opaque
//! origins beyond a conservative `file://` policy, no origin serialization rules
//! beyond a simple storage key, and no document `domain` setter.
//!
//! Port normalization: a URL whose port is the scheme default (80 for http, 443
//! for https) has the **same origin** as one with no explicit port. So
//! `http://example.com` and `http://example.com:80` are equal; `:8080` differs.
//!
//! `file://` URLs have an **opaque** origin in real browsers. Mocha is
//! conservative: [`Origin::from_url`] returns [`MochaError::Security`] for
//! `file://`, so origin-keyed web storage is simply unsupported there (documented
//! and tested) rather than silently shared.

use mocha_error::{MochaError, MochaResult};
use mocha_url::{Scheme, Url};

/// A tuple web origin: scheme + host + (normalized) port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    /// The scheme, lowercased (`"http"` or `"https"`).
    pub scheme: String,
    /// The host, lowercased.
    pub host: String,
    /// The port, or `None` when it is the scheme default.
    pub port: Option<u16>,
}

impl Origin {
    /// Derive the origin of `url`.
    ///
    /// `http`/`https` URLs yield a tuple origin (default ports normalized to
    /// `None`). `file://` URLs have an opaque origin and return
    /// [`MochaError::Security`] — origin-keyed web storage is unsupported there.
    pub fn from_url(url: &Url) -> MochaResult<Origin> {
        match url.scheme {
            Scheme::Http | Scheme::Https => {
                let host = url.host.clone().ok_or_else(|| {
                    MochaError::Security("URL has no host, so no tuple origin".to_string())
                })?;
                let default = url.scheme.default_port();
                let port = match url.port {
                    Some(p) if Some(p) == default => None,
                    other => other,
                };
                Ok(Origin {
                    scheme: url.scheme.as_str().to_string(),
                    host: host.to_ascii_lowercase(),
                    port,
                })
            }
            Scheme::File => Err(MochaError::Security(
                "file:// URLs have an opaque origin; origin-keyed web storage is unsupported"
                    .to_string(),
            )),
        }
    }

    /// Whether two origins are the same origin.
    pub fn is_same_origin(&self, other: &Origin) -> bool {
        self == other
    }

    /// A stable serialization usable as a storage key, e.g.
    /// `http://example.com` or `http://example.com:8080`.
    pub fn storage_key(&self) -> String {
        match self.port {
            Some(port) => format!("{}://{}:{}", self.scheme, self.host, port),
            None => format!("{}://{}", self.scheme, self.host),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(s: &str) -> Origin {
        Origin::from_url(&Url::parse(s).unwrap()).unwrap()
    }

    #[test]
    fn same_http_origin_matches() {
        assert!(origin("http://example.com/a").is_same_origin(&origin("http://example.com/b")));
    }

    #[test]
    fn default_ports_are_normalized() {
        // http://h and http://h:80 are the same origin.
        assert_eq!(
            origin("http://example.com/"),
            origin("http://example.com:80/")
        );
        assert_eq!(origin("http://example.com/").port, None);
    }

    #[test]
    fn different_scheme_differs() {
        // (https parses even though https loading is unsupported.)
        assert_ne!(
            origin("http://example.com/"),
            origin("https://example.com/")
        );
    }

    #[test]
    fn different_host_differs() {
        assert_ne!(origin("http://a.com/"), origin("http://b.com/"));
    }

    #[test]
    fn different_port_differs() {
        assert_ne!(
            origin("http://example.com:8080/"),
            origin("http://example.com/")
        );
    }

    #[test]
    fn host_is_lowercased() {
        assert_eq!(origin("http://Example.COM/").host, "example.com");
    }

    #[test]
    fn storage_key_includes_nondefault_port_only() {
        assert_eq!(
            origin("http://example.com/").storage_key(),
            "http://example.com"
        );
        assert_eq!(
            origin("http://example.com:80/").storage_key(),
            "http://example.com"
        );
        assert_eq!(
            origin("http://example.com:8080/").storage_key(),
            "http://example.com:8080"
        );
    }

    #[test]
    fn file_origin_is_opaque_and_unsupported() {
        let err = Origin::from_url(&Url::parse("file:///tmp/x.html").unwrap()).unwrap_err();
        assert!(matches!(err, MochaError::Security(_)));
    }
}
