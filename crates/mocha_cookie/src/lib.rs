//! A minimal HTTP cookie model (Milestone 15).
//!
//! This parses `Set-Cookie`, keeps an in-memory [`CookieJar`], and builds the
//! `Cookie` request header with domain/path/secure/expiry matching. It is **not**
//! a complete RFC 6265bis implementation: no public-suffix list, no third-party
//! or partitioned-cookie policy, no `__Secure-`/`__Host-` prefixes, and no full
//! same-site enforcement (the `SameSite` attribute is parsed and stored but not
//! enforced against a request's site, since Mocha has no navigation site model).
//!
//! Since HTTPS is unsupported, `Secure` cookies are effectively never sent (they
//! require an `https` request). Session cookies (no `Max-Age`/`Expires`) have no
//! expiry time and are treated as non-expiring by the jar.

mod date;

use mocha_error::{MochaError, MochaResult};
use mocha_url::{Scheme, Url};

pub use date::parse_http_date_ms;

/// The `SameSite` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Lax,
    Strict,
    None,
    /// The attribute was absent.
    Unspecified,
}

impl SameSite {
    /// A stable lowercase serialization (for persistence).
    pub fn as_str(self) -> &'static str {
        match self {
            SameSite::Lax => "lax",
            SameSite::Strict => "strict",
            SameSite::None => "none",
            SameSite::Unspecified => "unspecified",
        }
    }

    /// Parse a serialized `SameSite` (from [`SameSite::as_str`]); unknown values
    /// read as `Unspecified`.
    pub fn from_storage(s: &str) -> SameSite {
        match s {
            "lax" => SameSite::Lax,
            "strict" => SameSite::Strict,
            "none" => SameSite::None,
            _ => SameSite::Unspecified,
        }
    }
}

/// A parsed cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// Absolute expiry in epoch ms (from `Max-Age` or `Expires`), or `None` for a
    /// session cookie.
    pub expires_ms: Option<i64>,
    /// The raw `Max-Age` in seconds, if given.
    pub max_age: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    /// `true` when no `Domain` attribute was given: the cookie matches only the
    /// exact request host.
    pub host_only: bool,
    pub created_ms: i64,
}

impl Cookie {
    /// Whether the cookie is expired at `now_ms`.
    pub fn is_expired(&self, now_ms: i64) -> bool {
        matches!(self.expires_ms, Some(t) if now_ms >= t)
    }

    /// Whether this cookie may be sent for `url` at `now_ms` (domain, path,
    /// secure, and expiry checks).
    pub fn matches_request(&self, url: &Url, now_ms: i64) -> bool {
        if self.is_expired(now_ms) {
            return false;
        }
        if self.secure && url.scheme != Scheme::Https {
            return false;
        }
        let Some(host) = url.host.as_deref() else {
            return false;
        };
        if !domain_matches(host, &self.domain, self.host_only) {
            return false;
        }
        path_matches(&request_path(url), &self.path)
    }
}

/// Parse a single `Set-Cookie` header value against the request URL.
///
/// Errors ([`MochaError::Security`]) on an empty/invalid cookie name or a
/// `Domain` attribute that does not domain-match the request host.
pub fn parse_set_cookie(header: &str, request_url: &Url, now_ms: i64) -> MochaResult<Cookie> {
    let mut parts = header.split(';');
    let pair = parts
        .next()
        .ok_or_else(|| MochaError::Security("empty Set-Cookie header".to_string()))?;
    let (name, value) = pair
        .split_once('=')
        .ok_or_else(|| MochaError::Security("Set-Cookie has no name=value".to_string()))?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || !is_valid_cookie_name(name) {
        return Err(MochaError::Security(format!(
            "invalid cookie name {name:?}"
        )));
    }

    let request_host = request_url
        .host
        .as_deref()
        .ok_or_else(|| MochaError::Security("request URL has no host for a cookie".to_string()))?
        .to_ascii_lowercase();

    let mut domain: Option<String> = None;
    let mut path: Option<String> = None;
    let mut max_age: Option<i64> = None;
    let mut expires_date_ms: Option<i64> = None;
    let mut secure = false;
    let mut http_only = false;
    let mut same_site = SameSite::Unspecified;

    for attr in parts {
        let attr = attr.trim();
        if attr.is_empty() {
            continue;
        }
        let (key, val) = match attr.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (attr, ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "domain" => {
                let d = val.trim_start_matches('.').to_ascii_lowercase();
                if !d.is_empty() {
                    domain = Some(d);
                }
            }
            "path" if val.starts_with('/') => path = Some(val.to_string()),
            "max-age" => max_age = val.parse::<i64>().ok(),
            "expires" => expires_date_ms = parse_http_date_ms(val),
            "secure" => secure = true,
            "httponly" => http_only = true,
            "samesite" => {
                same_site = match val.to_ascii_lowercase().as_str() {
                    "strict" => SameSite::Strict,
                    "lax" => SameSite::Lax,
                    "none" => SameSite::None,
                    _ => SameSite::Unspecified,
                }
            }
            _ => {}
        }
    }

    // Resolve domain + host-only flag.
    let (domain, host_only) = match domain {
        Some(d) => {
            if !domain_matches(&request_host, &d, false) {
                return Err(MochaError::Security(format!(
                    "cookie Domain {d:?} does not match request host {request_host:?}"
                )));
            }
            (d, false)
        }
        None => (request_host.clone(), true),
    };

    let path = path.unwrap_or_else(|| default_path(&request_path(request_url)));

    // Max-Age wins over Expires (RFC 6265). Max-Age <= 0 expires immediately.
    let expires_ms = match max_age {
        Some(seconds) => Some(now_ms + seconds.saturating_mul(1000)),
        None => expires_date_ms,
    };

    Ok(Cookie {
        name: name.to_string(),
        value: value.to_string(),
        domain,
        path,
        expires_ms,
        max_age,
        secure,
        http_only,
        same_site,
        host_only,
        created_ms: now_ms,
    })
}

/// An in-memory cookie jar.
#[derive(Debug, Default, Clone)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new() -> CookieJar {
        CookieJar::default()
    }

    /// Build a jar from already-constructed cookies (e.g. loaded from storage).
    pub fn from_cookies(cookies: Vec<Cookie>) -> CookieJar {
        CookieJar { cookies }
    }

    /// Insert or replace a cookie, keyed by `(name, domain, path)`. An
    /// already-expired cookie removes any existing match instead of being stored.
    pub fn set_cookie(&mut self, cookie: Cookie, now_ms: i64) {
        self.cookies.retain(|c| !same_identity(c, &cookie));
        if !cookie.is_expired(now_ms) {
            self.cookies.push(cookie);
        }
    }

    /// Parse a `Set-Cookie` header and store the result.
    pub fn store_set_cookie(
        &mut self,
        header: &str,
        request_url: &Url,
        now_ms: i64,
    ) -> MochaResult<()> {
        let cookie = parse_set_cookie(header, request_url, now_ms)?;
        self.set_cookie(cookie, now_ms);
        Ok(())
    }

    /// All cookies that may be sent for `url` at `now_ms`, in send order
    /// (longest path first, then earliest created — deterministic).
    pub fn cookies_for_request(&self, url: &Url, now_ms: i64) -> Vec<Cookie> {
        let mut matched: Vec<Cookie> = self
            .cookies
            .iter()
            .filter(|c| c.matches_request(url, now_ms))
            .cloned()
            .collect();
        matched.sort_by(|a, b| {
            b.path
                .len()
                .cmp(&a.path.len())
                .then(a.created_ms.cmp(&b.created_ms))
                .then(a.name.cmp(&b.name))
        });
        matched
    }

    /// The `Cookie` request header value for `url`, or `None` if no cookies apply.
    pub fn cookie_header_for_request(&self, url: &Url, now_ms: i64) -> Option<String> {
        let cookies = self.cookies_for_request(url, now_ms);
        if cookies.is_empty() {
            return None;
        }
        Some(
            cookies
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    /// All stored cookies (for persistence).
    pub fn cookies(&self) -> &[Cookie] {
        &self.cookies
    }

    /// Drop every expired cookie.
    pub fn remove_expired(&mut self, now_ms: i64) {
        self.cookies.retain(|c| !c.is_expired(now_ms));
    }

    /// Remove all cookies.
    pub fn clear(&mut self) {
        self.cookies.clear();
    }
}

fn same_identity(a: &Cookie, b: &Cookie) -> bool {
    a.name == b.name && a.domain == b.domain && a.path == b.path
}

/// A cookie name must be a non-empty token: no control chars, whitespace, or
/// separators.
fn is_valid_cookie_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| {
            !c.is_control()
                && !c.is_whitespace()
                && !matches!(
                    c,
                    '(' | ')'
                        | '<'
                        | '>'
                        | '@'
                        | ','
                        | ';'
                        | ':'
                        | '\\'
                        | '"'
                        | '/'
                        | '['
                        | ']'
                        | '?'
                        | '='
                        | '{'
                        | '}'
                )
        })
}

fn request_path(url: &Url) -> String {
    if url.path.is_empty() {
        "/".to_string()
    } else {
        url.path.clone()
    }
}

/// RFC 6265 §5.1.4 default-path.
fn default_path(path: &str) -> String {
    if !path.starts_with('/') {
        return "/".to_string();
    }
    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => path[..i].to_string(),
    }
}

/// RFC 6265 §5.1.3 domain-match. With `host_only`, only an exact host matches.
fn domain_matches(host: &str, domain: &str, host_only: bool) -> bool {
    let host = host.to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    if host == domain {
        return true;
    }
    if host_only {
        return false;
    }
    host.ends_with(&format!(".{domain}"))
}

/// RFC 6265 §5.1.4 path-match.
fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if request_path.starts_with(cookie_path) {
        if cookie_path.ends_with('/') {
            return true;
        }
        return request_path.as_bytes().get(cookie_path.len()) == Some(&b'/');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn parse(header: &str, request: &str) -> Cookie {
        parse_set_cookie(header, &url(request), 1000).unwrap()
    }

    #[test]
    fn parse_basic_name_value() {
        let c = parse("sid=abc123", "http://example.com/");
        assert_eq!(c.name, "sid");
        assert_eq!(c.value, "abc123");
        assert_eq!(c.domain, "example.com");
        assert!(c.host_only);
        assert_eq!(c.path, "/");
        assert!(c.expires_ms.is_none(), "session cookie");
    }

    #[test]
    fn parse_domain_path_and_flags() {
        let c = parse(
            "a=b; Domain=example.com; Path=/app; Secure; HttpOnly; SameSite=Lax",
            "http://www.example.com/app/page",
        );
        assert_eq!(c.domain, "example.com");
        assert!(!c.host_only);
        assert_eq!(c.path, "/app");
        assert!(c.secure);
        assert!(c.http_only);
        assert_eq!(c.same_site, SameSite::Lax);
    }

    #[test]
    fn parse_max_age_sets_expiry() {
        let c = parse_set_cookie("a=b; Max-Age=60", &url("http://e.com/"), 1000).unwrap();
        assert_eq!(c.max_age, Some(60));
        assert_eq!(c.expires_ms, Some(1000 + 60_000));
        assert!(!c.is_expired(1000));
        assert!(c.is_expired(61_001));
    }

    #[test]
    fn parse_expires_date() {
        let c = parse(
            "a=b; Expires=Sun, 06 Nov 1994 08:49:37 GMT",
            "http://e.com/",
        );
        // 784111777 seconds since epoch.
        assert_eq!(c.expires_ms, Some(784_111_777_000));
    }

    #[test]
    fn reject_invalid_cookie_name() {
        assert!(parse_set_cookie("=novalue", &url("http://e.com/"), 0).is_err());
        assert!(parse_set_cookie("bad name=1", &url("http://e.com/"), 0).is_err());
        assert!(parse_set_cookie("novalue", &url("http://e.com/"), 0).is_err());
    }

    #[test]
    fn reject_domain_not_matching_request() {
        assert!(parse_set_cookie("a=b; Domain=evil.com", &url("http://good.com/"), 0).is_err());
    }

    #[test]
    fn default_path_is_request_directory() {
        let c = parse("a=b", "http://e.com/foo/bar");
        assert_eq!(c.path, "/foo");
        let c2 = parse("a=b", "http://e.com/foo");
        assert_eq!(c2.path, "/");
    }

    #[test]
    fn host_only_matches_exact_host_only() {
        let mut jar = CookieJar::new();
        jar.store_set_cookie("a=b", &url("http://example.com/"), 0)
            .unwrap();
        // Exact host: sent; subdomain: not sent (host-only cookie).
        assert_eq!(
            jar.cookie_header_for_request(&url("http://example.com/"), 0)
                .as_deref(),
            Some("a=b")
        );
        assert!(jar
            .cookie_header_for_request(&url("http://sub.example.com/"), 0)
            .is_none());
    }

    #[test]
    fn domain_cookie_matches_subdomains() {
        let mut jar = CookieJar::new();
        jar.store_set_cookie("a=b; Domain=example.com", &url("http://example.com/"), 0)
            .unwrap();
        assert!(jar
            .cookie_header_for_request(&url("http://sub.example.com/"), 0)
            .is_some());
        assert!(jar
            .cookie_header_for_request(&url("http://example.com/"), 0)
            .is_some());
    }

    #[test]
    fn path_matching_restricts_sending() {
        let mut jar = CookieJar::new();
        jar.store_set_cookie("a=b; Path=/app", &url("http://e.com/app"), 0)
            .unwrap();
        assert!(jar
            .cookie_header_for_request(&url("http://e.com/app/page"), 0)
            .is_some());
        assert!(jar
            .cookie_header_for_request(&url("http://e.com/other"), 0)
            .is_none());
    }

    #[test]
    fn expired_cookie_is_not_sent() {
        let mut jar = CookieJar::new();
        jar.store_set_cookie("a=b; Max-Age=10", &url("http://e.com/"), 1000)
            .unwrap();
        assert!(jar
            .cookie_header_for_request(&url("http://e.com/"), 1000)
            .is_some());
        assert!(jar
            .cookie_header_for_request(&url("http://e.com/"), 999_999)
            .is_none());
    }

    #[test]
    fn secure_cookie_not_sent_over_http() {
        let mut jar = CookieJar::new();
        jar.store_set_cookie("a=b; Secure", &url("http://e.com/"), 0)
            .unwrap();
        assert!(jar
            .cookie_header_for_request(&url("http://e.com/"), 0)
            .is_none());
    }

    #[test]
    fn replacing_same_identity_keeps_one() {
        let mut jar = CookieJar::new();
        jar.store_set_cookie("a=1", &url("http://e.com/"), 0)
            .unwrap();
        jar.store_set_cookie("a=2", &url("http://e.com/"), 0)
            .unwrap();
        assert_eq!(jar.cookies().len(), 1);
        assert_eq!(
            jar.cookie_header_for_request(&url("http://e.com/"), 0)
                .as_deref(),
            Some("a=2")
        );
    }

    #[test]
    fn cookie_header_ordering_is_deterministic() {
        let mut jar = CookieJar::new();
        // Longer path first, regardless of insertion order.
        jar.store_set_cookie("short=1; Path=/", &url("http://e.com/app/x"), 0)
            .unwrap();
        jar.store_set_cookie("long=1; Path=/app", &url("http://e.com/app/x"), 0)
            .unwrap();
        assert_eq!(
            jar.cookie_header_for_request(&url("http://e.com/app/x"), 0)
                .as_deref(),
            Some("long=1; short=1")
        );
    }

    #[test]
    fn max_age_zero_expires_immediately() {
        let mut jar = CookieJar::new();
        jar.store_set_cookie("a=b; Max-Age=0", &url("http://e.com/"), 1000)
            .unwrap();
        assert!(jar.cookies().is_empty(), "Max-Age=0 is not stored");
    }
}
