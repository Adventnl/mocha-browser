//! Minimal URL and local-path parsing for Mocha Browser.
//!
//! **This is not a full [WHATWG URL] implementation.** It recognises just enough
//! to drive Milestone 4: local file paths, `file:` URLs, and `http:`/`https:`
//! URLs with host, optional port, path, query, and fragment. Percent-encoding,
//! userinfo, IPv6 hosts, and dot-segment normalization are out of scope.
//! Networking is never performed here; this crate only classifies and normalises
//! input strings.
//!
//! [WHATWG URL]: https://url.spec.whatwg.org/

use mocha_error::{MochaError, MochaResult};

/// The URL schemes Mocha can currently recognise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// A local file, either a bare path or a `file:` URL.
    File,
    /// An `http:` URL.
    Http,
    /// An `https:` URL.
    Https,
}

impl Scheme {
    /// The lowercase scheme name without the trailing colon.
    pub fn as_str(&self) -> &'static str {
        match self {
            Scheme::File => "file",
            Scheme::Http => "http",
            Scheme::Https => "https",
        }
    }

    /// The default TCP port for HTTP schemes.
    pub fn default_port(&self) -> Option<u16> {
        match self {
            Scheme::File => None,
            Scheme::Http => Some(80),
            Scheme::Https => Some(443),
        }
    }
}

/// A parsed location.
///
/// For [`Scheme::File`], `host`/`port`/`query`/`fragment` are `None` and `path`
/// is the local file path. For HTTP schemes, `host` holds the authority host and
/// `path` defaults to `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// The recognised scheme.
    pub scheme: Scheme,
    /// The host (lowercased) for HTTP schemes; always `None` for files.
    pub host: Option<String>,
    /// An explicit port, if one was given.
    pub port: Option<u16>,
    /// The path component.
    pub path: String,
    /// The query string without the leading `?`, if present.
    pub query: Option<String>,
    /// The fragment without the leading `#`, if present. Never sent over HTTP.
    pub fragment: Option<String>,
}

impl Url {
    /// Parse a user-supplied location string.
    pub fn parse(input: &str) -> MochaResult<Url> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(MochaError::InvalidUrl("input is empty".to_string()));
        }

        if let Some(separator) = trimmed.find("://") {
            let scheme = trimmed[..separator].to_ascii_lowercase();
            let rest = &trimmed[separator + 3..];
            return match scheme.as_str() {
                "file" => parse_file_url(rest),
                "http" => parse_authority_url(Scheme::Http, rest),
                "https" => parse_authority_url(Scheme::Https, rest),
                other => Err(MochaError::InvalidUrl(format!(
                    "unsupported url scheme: {other}:"
                ))),
            };
        }

        // Reject any other explicit `scheme:` prefix (ftp:, javascript:, data:).
        if let Some(scheme) = explicit_scheme(trimmed) {
            return Err(MochaError::InvalidUrl(format!(
                "unsupported url scheme: {scheme}:"
            )));
        }

        // No recognised scheme prefix: treat the whole string as a local path.
        Ok(Url {
            scheme: Scheme::File,
            host: None,
            port: None,
            path: trimmed.to_string(),
            query: None,
            fragment: None,
        })
    }

    /// Returns `true` when this URL points at a local file.
    pub fn is_file(&self) -> bool {
        self.scheme == Scheme::File
    }

    /// The `host[:port]` authority for HTTP schemes, if any.
    pub fn authority(&self) -> Option<String> {
        let host = self.host.as_ref()?;
        Some(match self.port {
            Some(port) => format!("{host}:{port}"),
            None => host.clone(),
        })
    }

    /// The effective port: the explicit port, else the scheme default.
    pub fn effective_port(&self) -> Option<u16> {
        self.port.or_else(|| self.scheme.default_port())
    }

    /// The HTTP request target: path plus `?query` (the fragment is never sent).
    pub fn request_target(&self) -> String {
        match &self.query {
            Some(query) => format!("{}?{}", self.path, query),
            None => self.path.clone(),
        }
    }

    /// A normalized string form used as a cache key. Excludes the fragment.
    pub fn normalized(&self) -> String {
        match self.scheme {
            Scheme::File => format!("file://{}", self.path),
            Scheme::Http | Scheme::Https => {
                let authority = self.authority().unwrap_or_default();
                format!(
                    "{}://{}{}",
                    self.scheme.as_str(),
                    authority,
                    self.request_target()
                )
            }
        }
    }

    /// Resolve a relative reference (a redirect `Location` or a subresource
    /// `href`/`src`) against this URL.
    ///
    /// Handles absolute URLs, scheme-relative (`//host/path`), absolute-path
    /// (`/path`), and relative paths (resolved against this URL's directory).
    /// Dot-segments (`.`/`..`) in the resulting `/`-separated path are normalized.
    pub fn join(&self, location: &str) -> MochaResult<Url> {
        let location = location.trim();
        if location.is_empty() {
            return Err(MochaError::InvalidUrl(
                "relative reference is empty".to_string(),
            ));
        }
        if location.contains("://") {
            return Url::parse(location);
        }
        if let Some(rest) = location.strip_prefix("//") {
            return Url::parse(&format!("{}://{}", self.scheme.as_str(), rest));
        }

        let (raw_path, query, fragment) = split_path_query_fragment(location);
        let merged = if raw_path.starts_with('/') {
            raw_path.to_string()
        } else {
            // Resolve against this URL's directory. Accept either separator so
            // Windows file paths (backslashes) resolve too.
            let directory = match self.path.rfind(['/', '\\']) {
                Some(index) => &self.path[..=index],
                None => "/",
            };
            format!("{directory}{raw_path}")
        };
        let path = normalize_dot_segments(&merged);

        Ok(Url {
            scheme: self.scheme,
            host: self.host.clone(),
            port: self.port,
            path,
            query,
            fragment,
        })
    }
}

/// Resolve `.` and `..` segments in a `/`-separated path, preserving a leading
/// slash. URL paths and POSIX file paths are normalized here. Windows paths
/// (which contain backslashes) are left untouched so the OS resolves their
/// dot-segments at filesystem-access time — normalizing the backslash-prefixed
/// drive segment would mangle the path.
fn normalize_dot_segments(path: &str) -> String {
    if !path.contains('/') || path.contains('\\') {
        return path.to_string();
    }
    let leading_slash = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let mut result = String::new();
    if leading_slash {
        result.push('/');
    }
    result.push_str(&segments.join("/"));
    result
}

/// Parse the portion of a `file:` URL that follows `file://`.
///
/// Supports POSIX absolute paths (`file:///etc/hosts` → `/etc/hosts`) and Windows
/// drive paths in both the canonical (`file:///C:/dir/x` → `C:/dir/x`) and the
/// lenient two-slash (`file://C:\dir\x` → `C:\dir\x`) spellings. A `file:` URL
/// with a real (non-drive) host is still unsupported.
fn parse_file_url(rest: &str) -> MochaResult<Url> {
    if rest.is_empty() {
        return Err(MochaError::InvalidUrl(
            "file url is missing a path".to_string(),
        ));
    }

    let path = if let Some(after_slash) = rest.strip_prefix('/') {
        if starts_with_drive(after_slash) {
            // Canonical `file:///C:/...`: drop the leading slash before the drive
            // so the result is a real Windows path.
            after_slash.to_string()
        } else {
            // POSIX absolute path: keep the leading slash.
            rest.to_string()
        }
    } else if starts_with_drive(rest) {
        // Lenient `file://C:\...`, where the host position holds a drive letter.
        rest.to_string()
    } else {
        return Err(MochaError::InvalidUrl(
            "file url with a non-empty host is not supported (use file:///path)".to_string(),
        ));
    };

    Ok(Url {
        scheme: Scheme::File,
        host: None,
        port: None,
        path,
        query: None,
        fragment: None,
    })
}

/// Whether `s` begins with a Windows drive prefix such as `C:/` or `C:\`.
fn starts_with_drive(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

/// Parse the portion of an HTTP(S) URL that follows `http://` / `https://`.
fn parse_authority_url(scheme: Scheme, rest: &str) -> MochaResult<Url> {
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let remainder = &rest[authority_end..];

    let (host_part, port) = match authority.rsplit_once(':') {
        Some((host, port_str)) => {
            let port = port_str
                .parse::<u16>()
                .map_err(|_| MochaError::InvalidUrl(format!("invalid port: {port_str}")))?;
            (host, Some(port))
        }
        None => (authority, None),
    };
    if host_part.is_empty() {
        return Err(MochaError::InvalidUrl(
            "http url is missing a host".to_string(),
        ));
    }

    let (path, query, fragment) = split_path_query_fragment(remainder);
    let path = if path.is_empty() {
        "/".to_string()
    } else {
        path
    };

    Ok(Url {
        scheme,
        host: Some(host_part.to_ascii_lowercase()),
        port,
        path,
        query,
        fragment,
    })
}

/// Split a `path?query#fragment` tail into its three parts.
fn split_path_query_fragment(input: &str) -> (String, Option<String>, Option<String>) {
    let (without_fragment, fragment) = match input.split_once('#') {
        Some((head, frag)) => (head, Some(frag.to_string())),
        None => (input, None),
    };
    let (path, query) = match without_fragment.split_once('?') {
        Some((head, query)) => (head, Some(query.to_string())),
        None => (without_fragment, None),
    };
    (path.to_string(), query, fragment)
}

/// Detect an explicit `scheme:` prefix so unsupported schemes can be rejected.
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
    fn parse_relative_file_path_unchanged() {
        let url = Url::parse("examples/basic/index.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.host, None);
        assert_eq!(url.path, "examples/basic/index.html");
    }

    #[test]
    fn parse_absolute_file_path_unchanged() {
        let url = Url::parse("/path/to/file.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.path, "/path/to/file.html");
    }

    #[test]
    fn parse_file_url_unchanged() {
        let url = Url::parse("file:///path/to/file.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.path, "/path/to/file.html");
    }

    #[test]
    fn parse_windows_drive_file_url_canonical() {
        // `file:///C:/dir/x` drops the leading slash before the drive letter.
        let url = Url::parse("file:///C:/Users/x.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.path, "C:/Users/x.html");
    }

    #[test]
    fn parse_windows_drive_file_url_two_slash() {
        // Lenient `file://C:\dir\x`: the drive letter sits in the host position.
        let url = Url::parse(r"file://C:\Users\x.html").unwrap();
        assert_eq!(url.scheme, Scheme::File);
        assert_eq!(url.path, r"C:\Users\x.html");
    }

    #[test]
    fn parse_file_url_with_real_host_is_rejected() {
        let error = Url::parse("file://server/share/x.html").unwrap_err();
        assert!(matches!(error, MochaError::InvalidUrl(_)));
    }

    #[test]
    fn http_default_path_is_slash() {
        let url = Url::parse("http://example.com").unwrap();
        assert_eq!(url.scheme, Scheme::Http);
        assert_eq!(url.host.as_deref(), Some("example.com"));
        assert_eq!(url.path, "/");
    }

    #[test]
    fn https_default_path_is_slash() {
        let url = Url::parse("https://example.com").unwrap();
        assert_eq!(url.scheme, Scheme::Https);
        assert_eq!(url.path, "/");
    }

    #[test]
    fn host_and_scheme_are_lowercased() {
        let url = Url::parse("HTTP://Example.COM/A").unwrap();
        assert_eq!(url.scheme, Scheme::Http);
        assert_eq!(url.host.as_deref(), Some("example.com"));
        assert_eq!(url.path, "/A"); // path case is preserved
    }

    #[test]
    fn port_is_parsed() {
        let url = Url::parse("http://example.com:8080/index.html").unwrap();
        assert_eq!(url.port, Some(8080));
        assert_eq!(url.effective_port(), Some(8080));
        assert_eq!(url.authority().as_deref(), Some("example.com:8080"));
    }

    #[test]
    fn default_effective_port_from_scheme() {
        assert_eq!(
            Url::parse("http://example.com").unwrap().effective_port(),
            Some(80)
        );
        assert_eq!(
            Url::parse("https://example.com").unwrap().effective_port(),
            Some(443)
        );
    }

    #[test]
    fn invalid_port_is_rejected() {
        let error = Url::parse("http://example.com:notaport/").unwrap_err();
        assert!(matches!(error, MochaError::InvalidUrl(_)));
    }

    #[test]
    fn query_is_preserved_and_in_request_target() {
        let url = Url::parse("http://example.com/search?q=mocha").unwrap();
        assert_eq!(url.query.as_deref(), Some("q=mocha"));
        assert_eq!(url.request_target(), "/search?q=mocha");
    }

    #[test]
    fn fragment_is_parsed_but_excluded_from_request_target() {
        let url = Url::parse("http://example.com/a#section").unwrap();
        assert_eq!(url.fragment.as_deref(), Some("section"));
        assert_eq!(url.request_target(), "/a");
        assert!(!url.normalized().contains('#'));
    }

    #[test]
    fn missing_host_is_rejected() {
        let error = Url::parse("http:///index.html").unwrap_err();
        match error {
            MochaError::InvalidUrl(message) => assert!(message.contains("host")),
            other => panic!("expected InvalidUrl, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_scheme_is_rejected() {
        for input in [
            "ftp://example.com",
            "javascript:alert(1)",
            "data:text/html,x",
        ] {
            assert!(matches!(
                Url::parse(input).unwrap_err(),
                MochaError::InvalidUrl(_)
            ));
        }
    }

    #[test]
    fn empty_input_is_rejected() {
        assert!(matches!(
            Url::parse("   ").unwrap_err(),
            MochaError::InvalidUrl(_)
        ));
    }

    #[test]
    fn join_absolute_url() {
        let base = Url::parse("http://example.com/a/b.html").unwrap();
        let joined = base.join("http://other.com/x").unwrap();
        assert_eq!(joined.host.as_deref(), Some("other.com"));
        assert_eq!(joined.path, "/x");
    }

    #[test]
    fn join_absolute_path() {
        let base = Url::parse("http://example.com/a/b.html").unwrap();
        let joined = base.join("/c/d.html").unwrap();
        assert_eq!(joined.host.as_deref(), Some("example.com"));
        assert_eq!(joined.path, "/c/d.html");
    }

    #[test]
    fn join_relative_path_against_directory() {
        let base = Url::parse("http://example.com/a/b.html").unwrap();
        let joined = base.join("c.html").unwrap();
        assert_eq!(joined.path, "/a/c.html");
    }

    #[test]
    fn join_resolves_dot_and_dotdot_segments() {
        let base = Url::parse("http://example.com/a/b/page.html").unwrap();
        assert_eq!(base.join("../style.css").unwrap().path, "/a/style.css");
        assert_eq!(base.join("./style.css").unwrap().path, "/a/b/style.css");
        assert_eq!(base.join("../../x.css").unwrap().path, "/x.css");
        // An absolute-path reference ignores the base directory.
        assert_eq!(base.join("/style.css").unwrap().path, "/style.css");
    }

    #[test]
    fn join_resolves_relative_against_file_base() {
        let base = Url::parse("examples/resources/page.html").unwrap();
        assert_eq!(
            base.join("style.css").unwrap().path,
            "examples/resources/style.css"
        );
        assert_eq!(
            base.join("../style.css").unwrap().path,
            "examples/style.css"
        );
    }

    #[test]
    fn normalized_is_a_stable_cache_key() {
        let url = Url::parse("http://example.com:80/a?x=1#frag").unwrap();
        assert_eq!(url.normalized(), "http://example.com:80/a?x=1");
    }
}
