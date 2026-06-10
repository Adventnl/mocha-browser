//! Minimal URL and local-path parsing for Mocha Browser.
//!
//! **This is not a full [WHATWG URL] implementation.** It recognises just enough
//! to drive Milestone 1: local file paths and `file:`/`http:`/`https:` URLs.
//! Query strings, fragments, ports, userinfo, percent-encoding, and relative URL
//! resolution are all out of scope and are not parsed. Networking is never
//! performed here; this crate only classifies and normalises input strings.
//!
//! [WHATWG URL]: https://url.spec.whatwg.org/

use mocha_error::{MochaError, MochaResult};

/// The URL schemes Mocha can currently recognise.
///
/// Only [`Scheme::File`] can actually be loaded in Milestone 1; the HTTP schemes
/// are recognised so the shell can report a clear "networking not available"
/// error instead of treating an `http://` URL as a local path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// A local file, either a bare path or a `file:` URL.
    File,
    /// An `http:` URL. Recognised but not loadable in Milestone 1.
    Http,
    /// An `https:` URL. Recognised but not loadable in Milestone 1.
    Https,
}

/// A parsed location.
///
/// For [`Scheme::File`], `host` is always `None` and `path` is the local file
/// path. For HTTP schemes, `host` holds the authority and `path` holds the
/// path component (defaulting to `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// The recognised scheme.
    pub scheme: Scheme,
    /// The host/authority for HTTP schemes; always `None` for files.
    pub host: Option<String>,
    /// The path component.
    pub path: String,
}

impl Url {
    /// Parse a user-supplied location string.
    ///
    /// Accepts:
    /// - relative file paths (`examples/basic/index.html`, `./a/b.html`)
    /// - absolute file paths (`/path/to/file.html`)
    /// - `file:///path/to/file.html`
    /// - `http://host/path` and `https://host/path`
    ///
    /// Rejects unsupported schemes, empty input, and HTTP URLs without a host.
    pub fn parse(input: &str) -> MochaResult<Url> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(MochaError::InvalidUrl("input is empty".to_string()));
        }

        if let Some(rest) = trimmed.strip_prefix("file://") {
            return parse_file_url(rest);
        }
        if let Some(rest) = trimmed.strip_prefix("http://") {
            return parse_http_url(Scheme::Http, rest);
        }
        if let Some(rest) = trimmed.strip_prefix("https://") {
            return parse_http_url(Scheme::Https, rest);
        }

        // Reject any other explicit `scheme:` prefix (ftp:, javascript:, data:, ...).
        if let Some(scheme) = explicit_scheme(trimmed) {
            return Err(MochaError::InvalidUrl(format!(
                "unsupported url scheme: {scheme}:"
            )));
        }

        // No recognised scheme prefix: treat the whole string as a local path.
        Ok(Url {
            scheme: Scheme::File,
            host: None,
            path: trimmed.to_string(),
        })
    }

    /// Returns `true` when this URL points at a local file.
    pub fn is_file(&self) -> bool {
        self.scheme == Scheme::File
    }
}

/// Parse the portion of a `file:` URL that follows `file://`.
///
/// A standard `file:///path` URL has an empty authority, so after stripping
/// `file://` the remainder begins with `/`. We do not support a non-empty
/// authority (for example `file://host/path`).
fn parse_file_url(rest: &str) -> MochaResult<Url> {
    if rest.is_empty() {
        return Err(MochaError::InvalidUrl(
            "file url is missing a path".to_string(),
        ));
    }
    if !rest.starts_with('/') {
        return Err(MochaError::InvalidUrl(
            "file url with a non-empty host is not supported (use file:///path)".to_string(),
        ));
    }
    Ok(Url {
        scheme: Scheme::File,
        host: None,
        path: rest.to_string(),
    })
}

/// Parse the portion of an HTTP(S) URL that follows `http://` / `https://`.
fn parse_http_url(scheme: Scheme, rest: &str) -> MochaResult<Url> {
    let (host, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if host.is_empty() {
        return Err(MochaError::InvalidUrl(
            "http url is missing a host".to_string(),
        ));
    }
    Ok(Url {
        scheme,
        host: Some(host.to_string()),
        path: path.to_string(),
    })
}

/// Detect an explicit `scheme:` prefix so unsupported schemes can be rejected.
///
/// A scheme per RFC 3986 starts with a letter and continues with letters,
/// digits, `+`, `-`, or `.`. We deliberately ignore Windows drive letters
/// (`C:\...`) by requiring the scheme to be longer than one character.
fn explicit_scheme(input: &str) -> Option<&str> {
    let colon = input.find(':')?;
    if colon < 2 {
        return None;
    }
    let candidate = &input[..colon];
    let mut chars = candidate.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_relative_file_path() {
        let url = Url::parse("examples/basic/index.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.host, None);
        assert_eq!(url.path, "examples/basic/index.html");
    }

    #[test]
    fn parse_dot_relative_file_path() {
        let url = Url::parse("./examples/basic/index.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.path, "./examples/basic/index.html");
    }

    #[test]
    fn parse_absolute_file_path() {
        let url = Url::parse("/path/to/file.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.path, "/path/to/file.html");
    }

    #[test]
    fn parse_file_url() {
        let url = Url::parse("file:///path/to/file.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.host, None);
        assert_eq!(url.path, "/path/to/file.html");
    }

    #[test]
    fn parse_http_url() {
        let url = Url::parse("http://example.com/index.html").unwrap();
        assert_eq!(url.scheme, Scheme::Http);
        assert_eq!(url.host.as_deref(), Some("example.com"));
        assert_eq!(url.path, "/index.html");
    }

    #[test]
    fn parse_https_url() {
        let url = Url::parse("https://example.com/index.html").unwrap();
        assert_eq!(url.scheme, Scheme::Https);
        assert_eq!(url.host.as_deref(), Some("example.com"));
        assert_eq!(url.path, "/index.html");
    }

    #[test]
    fn http_url_without_path_defaults_to_root() {
        let url = Url::parse("http://example.com").unwrap();
        assert_eq!(url.host.as_deref(), Some("example.com"));
        assert_eq!(url.path, "/");
    }

    #[test]
    fn reject_unsupported_scheme() {
        for input in [
            "ftp://example.com",
            "javascript:alert(1)",
            "data:text/html,x",
        ] {
            let error = Url::parse(input).unwrap_err();
            assert!(
                matches!(error, MochaError::InvalidUrl(_)),
                "expected InvalidUrl for {input}, got {error:?}"
            );
        }
    }

    #[test]
    fn reject_empty_input() {
        let error = Url::parse("   ").unwrap_err();
        assert!(matches!(error, MochaError::InvalidUrl(_)));
    }

    #[test]
    fn reject_missing_http_host() {
        let error = Url::parse("http:///index.html").unwrap_err();
        match error {
            MochaError::InvalidUrl(message) => assert!(message.contains("host")),
            other => panic!("expected InvalidUrl, got {other:?}"),
        }
    }
}
