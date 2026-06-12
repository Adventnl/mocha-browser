//! A minimal blocking HTTP/1.1 client over `std::net::TcpStream`.
//!
//! This speaks just enough of the HTTP text protocol to issue a `GET`, read a
//! `Connection: close` response, parse the status line and headers, and follow
//! redirects. It is **not** a general HTTP client: no keep-alive, no chunked
//! transfer decoding, no compression, no TLS. `https` is handled by the caller
//! (returned as unsupported) — TLS is never hand-rolled here.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use mocha_error::{MochaError, MochaResult};
use mocha_url::{Scheme, Url};

use crate::{CookieProvider, Header, ResourceResponse, MAX_REDIRECTS};

const TIMEOUT: Duration = Duration::from_secs(15);

/// Fetch `start`, following redirects (up to [`MAX_REDIRECTS`]), with no cookies.
pub(crate) fn fetch_with_redirects(start: &Url) -> MochaResult<ResourceResponse> {
    fetch_with_redirects_cookies(start, None, 0)
}

/// Fetch `start`, following redirects, optionally attaching a `Cookie` header and
/// storing `Set-Cookie` responses through `cookies` (per hop).
pub(crate) fn fetch_with_redirects_cookies(
    start: &Url,
    mut cookies: Option<&mut dyn CookieProvider>,
    now_ms: i64,
) -> MochaResult<ResourceResponse> {
    let mut current = start.clone();
    let mut redirects = 0;

    loop {
        let cookie_header = match cookies.as_deref_mut() {
            Some(provider) => provider.cookie_header_for_request(&current, now_ms)?,
            None => None,
        };
        let raw = fetch_once(&current, cookie_header.as_deref())?;
        if let Some(provider) = cookies.as_deref_mut() {
            provider.store_response_cookies(&current, &raw.headers, now_ms)?;
        }
        if is_redirect(raw.status) {
            redirects += 1;
            if redirects > MAX_REDIRECTS {
                return Err(MochaError::Network(format!(
                    "too many redirects (limit {MAX_REDIRECTS})"
                )));
            }
            let location = header(&raw.headers, "location").ok_or_else(|| {
                MochaError::Network("redirect response is missing a Location header".to_string())
            })?;
            let target = current.join(location)?;
            match target.scheme {
                Scheme::Http => {}
                Scheme::Https => {
                    return Err(MochaError::UnsupportedFeature(
                        "https loading is not implemented in Milestone 4".to_string(),
                    ))
                }
                Scheme::File => {
                    return Err(MochaError::Network(
                        "refusing to follow a redirect from http to a file:// URL".to_string(),
                    ))
                }
            }
            current = target;
            continue;
        }

        let content_type = header(&raw.headers, "content-type").map(str::to_string);
        return Ok(ResourceResponse {
            final_url: current,
            status: Some(raw.status),
            headers: raw.headers,
            content_type,
            body: raw.body,
            from_cache: false,
        });
    }
}

struct RawResponse {
    status: u16,
    headers: Vec<Header>,
    body: Vec<u8>,
}

/// Perform a single GET (no redirect following), optionally sending a `Cookie`
/// request header.
fn fetch_once(url: &Url, cookie_header: Option<&str>) -> MochaResult<RawResponse> {
    let host = url
        .host
        .as_deref()
        .ok_or_else(|| MochaError::InvalidUrl("http url is missing a host".to_string()))?;
    let port = url.effective_port().unwrap_or(80);
    let authority = url.authority().unwrap_or_else(|| host.to_string());

    let cookie_line = match cookie_header {
        Some(value) => format!("Cookie: {value}\r\n"),
        None => String::new(),
    };
    let request = format!(
        "GET {target} HTTP/1.1\r\n\
         Host: {authority}\r\n\
         User-Agent: mocha-browser/0.1\r\n\
         Accept: */*\r\n\
         {cookie_line}\
         Connection: close\r\n\
         \r\n",
        target = url.request_target(),
    );

    let mut stream = TcpStream::connect((host, port))
        .map_err(|error| MochaError::Network(format!("cannot connect to {authority}: {error}")))?;
    stream.set_read_timeout(Some(TIMEOUT)).ok();
    stream.set_write_timeout(Some(TIMEOUT)).ok();
    stream
        .write_all(request.as_bytes())
        .map_err(|error| MochaError::Network(format!("failed to send request: {error}")))?;

    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| MochaError::Network(format!("failed to read response: {error}")))?;

    parse_response(&bytes)
}

fn parse_response(bytes: &[u8]) -> MochaResult<RawResponse> {
    let separator = find_subsequence(bytes, b"\r\n\r\n").ok_or_else(|| {
        MochaError::Network("malformed http response: no header terminator".to_string())
    })?;
    let header_text = std::str::from_utf8(&bytes[..separator]).map_err(|_| {
        MochaError::Network("http response headers are not valid UTF-8".to_string())
    })?;
    let body = bytes[separator + 4..].to_vec();

    let mut lines = header_text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| MochaError::Network("empty http response".to_string()))?;
    let status = parse_status(status_line)?;

    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push(Header {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            });
        }
    }

    if let Some(encoding) = header(&headers, "transfer-encoding") {
        if encoding.to_ascii_lowercase().contains("chunked") {
            return Err(MochaError::UnsupportedFeature(
                "chunked transfer-encoding is not supported in Milestone 4".to_string(),
            ));
        }
    }

    Ok(RawResponse {
        status,
        headers,
        body,
    })
}

fn parse_status(status_line: &str) -> MochaResult<u16> {
    // "HTTP/1.1 200 OK" → 200
    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| MochaError::Network(format!("malformed status line: {status_line:?}")))
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Case-insensitive header lookup.
pub(crate) fn header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
