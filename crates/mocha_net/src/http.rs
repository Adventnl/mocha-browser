//! A minimal blocking HTTP/1.1 client over `std::net::TcpStream`.
//!
//! This speaks just enough of the HTTP text protocol to issue a `GET`, read a
//! `Connection: close` response, parse the status line and headers, decode
//! `Transfer-Encoding: chunked` framing and `Content-Encoding: gzip` bodies
//! (via the from-scratch `mocha_gzip` crate), and follow redirects. `https`
//! runs the same protocol over a rustls stream ([`crate::tls`]) — TLS is never
//! hand-rolled here. It is still **not** a general HTTP client: no keep-alive,
//! no pipelining, and no encodings beyond gzip (clear errors, never guesses).

use std::cmp::Ordering;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use mocha_error::{MochaError, MochaResult};
use mocha_url::{Scheme, Url};

use crate::tls::TlsClient;
use crate::{CookieProvider, Header, ResourceResponse, MAX_REDIRECTS};

const TIMEOUT: Duration = Duration::from_secs(15);

/// Fetch `start`, following redirects (up to [`MAX_REDIRECTS`]), with no cookies.
pub(crate) fn fetch_with_redirects(start: &Url, tls: &TlsClient) -> MochaResult<ResourceResponse> {
    fetch_with_redirects_cookies(start, tls, None, 0)
}

/// Fetch `start`, following redirects, optionally attaching a `Cookie` header and
/// storing `Set-Cookie` responses through `cookies` (per hop).
pub(crate) fn fetch_with_redirects_cookies(
    start: &Url,
    tls: &TlsClient,
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
        let raw = fetch_once(&current, cookie_header.as_deref(), tls)?;
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
                // http→https upgrades and https→http downgrades both follow,
                // like real browsers; mixed-content policy for subresources
                // lives in mocha_security, not here.
                Scheme::Http | Scheme::Https => {}
                Scheme::File => {
                    return Err(MochaError::Network(
                        "refusing to follow a redirect from http(s) to a file:// URL".to_string(),
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
/// request header. `http` goes over a plain `TcpStream`; `https` goes through
/// the rustls client.
fn fetch_once(url: &Url, cookie_header: Option<&str>, tls: &TlsClient) -> MochaResult<RawResponse> {
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
         Accept-Encoding: gzip, deflate\r\n\
         {cookie_line}\
         Connection: close\r\n\
         \r\n",
        target = url.request_target(),
    );

    crate::net_log::trace(format!(
        "{} {}://{}{}",
        "GET",
        url.scheme.as_str(),
        authority,
        url.request_target()
    ));
    // DNS: resolve the host to its IP address(es) (only when tracing).
    if crate::net_log::is_on() {
        match (host, port).to_socket_addrs() {
            Ok(addrs) => {
                let ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
                crate::net_log::trace(format!("  DNS {host} -> {}", ips.join(", ")));
            }
            Err(error) => crate::net_log::trace(format!("  DNS {host} failed: {error}")),
        }
    }

    let bytes = match url.scheme {
        Scheme::Https => tls.exchange(host, port, request.as_bytes())?,
        _ => {
            let mut stream = TcpStream::connect((host, port)).map_err(|error| {
                MochaError::Network(format!("cannot connect to {authority}: {error}"))
            })?;
            crate::net_log::trace(format!(
                "  TCP connected to {}",
                stream
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| authority.clone())
            ));
            stream.set_read_timeout(Some(TIMEOUT)).ok();
            stream.set_write_timeout(Some(TIMEOUT)).ok();
            stream
                .write_all(request.as_bytes())
                .map_err(|error| MochaError::Network(format!("failed to send request: {error}")))?;
            read_response_bytes(&mut stream)?
        }
    };

    let response = parse_response(&bytes)?;
    crate::net_log::trace(format!(
        "  <- {} ({} bytes, {})",
        response.status,
        response.body.len(),
        header(&response.headers, "content-type").unwrap_or("?")
    ));
    Ok(response)
}

/// Read a `Connection: close` response to its end.
///
/// `UnexpectedEof` is treated as end-of-stream rather than an error: TLS peers
/// frequently close without a `close_notify` alert. The HTTP framing checks in
/// [`parse_response`] (`Content-Length` / chunked) still catch real truncation.
pub(crate) fn read_response_bytes<S: Read>(stream: &mut S) -> MochaResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => bytes.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => {
                return Err(MochaError::Network(format!(
                    "failed to read response: {error}"
                )))
            }
        }
    }
    Ok(bytes)
}

fn parse_response(bytes: &[u8]) -> MochaResult<RawResponse> {
    let separator = find_subsequence(bytes, b"\r\n\r\n").ok_or_else(|| {
        MochaError::Network("malformed http response: no header terminator".to_string())
    })?;
    let header_text = std::str::from_utf8(&bytes[..separator]).map_err(|_| {
        MochaError::Network("http response headers are not valid UTF-8".to_string())
    })?;
    let mut body = bytes[separator + 4..].to_vec();

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

    // Message framing: chunked transfer-encoding wins over Content-Length
    // (RFC 9112 §6.3); otherwise Content-Length bounds the close-delimited body.
    if let Some(encoding) = header(&headers, "transfer-encoding") {
        let encoding = encoding.trim().to_ascii_lowercase();
        match encoding.as_str() {
            "chunked" => body = decode_chunked(&body)?,
            "identity" => {}
            other => {
                return Err(MochaError::UnsupportedFeature(format!(
                    "transfer-encoding '{other}' is not supported (only chunked)"
                )))
            }
        }
    } else if let Some(declared) = header(&headers, "content-length") {
        let declared: usize = declared.trim().parse().map_err(|_| {
            MochaError::Network(format!("malformed Content-Length value {declared:?}"))
        })?;
        match body.len().cmp(&declared) {
            Ordering::Less => {
                return Err(MochaError::Network(format!(
                    "truncated response body: got {} of {} bytes",
                    body.len(),
                    declared
                )))
            }
            // The message ends at Content-Length; ignore close-delimited extras.
            Ordering::Greater => body.truncate(declared),
            Ordering::Equal => {}
        }
    }

    // Content-Encoding is decoded after transfer framing (RFC 9110 §8.4).
    if let Some(encoding) = header(&headers, "content-encoding") {
        let encoding = encoding.trim().to_ascii_lowercase();
        match encoding.as_str() {
            "gzip" | "x-gzip" => body = mocha_gzip::gzip_decompress(&body)?,
            "deflate" => body = mocha_gzip::zlib_decompress(&body)?,
            "identity" => {}
            other => {
                return Err(MochaError::UnsupportedFeature(format!(
                    "content-encoding '{other}' is not supported (only gzip/deflate/identity)"
                )))
            }
        }
    }

    Ok(RawResponse {
        status,
        headers,
        body,
    })
}

/// Decode `Transfer-Encoding: chunked` framing (RFC 9112 §7.1). Trailer fields
/// are parsed for framing but discarded; malformed or truncated framing is a
/// clear error.
fn decode_chunked(body: &[u8]) -> MochaResult<Vec<u8>> {
    let malformed = |what: &str| MochaError::Network(format!("malformed chunked body: {what}"));
    let mut output = Vec::new();
    let mut position = 0;

    loop {
        let line_end = find_subsequence(&body[position..], b"\r\n")
            .ok_or_else(|| malformed("missing chunk-size line"))?;
        let line = std::str::from_utf8(&body[position..position + line_end])
            .map_err(|_| malformed("chunk-size line is not valid UTF-8"))?;
        // Chunk extensions (";name=value") are allowed and ignored.
        let size_text = line.split(';').next().unwrap_or(line).trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| malformed("chunk size is not hexadecimal"))?;
        position += line_end + 2;

        if size == 0 {
            // Trailer section: zero or more field lines, then an empty line.
            let mut rest = &body[position..];
            loop {
                let line_end = find_subsequence(rest, b"\r\n")
                    .ok_or_else(|| malformed("unterminated trailer section"))?;
                let is_blank = line_end == 0;
                rest = &rest[line_end + 2..];
                if is_blank {
                    if !rest.is_empty() {
                        return Err(malformed("data after the final chunk"));
                    }
                    return Ok(output);
                }
            }
        }

        let end = position
            .checked_add(size)
            .filter(|&end| end <= body.len())
            .ok_or_else(|| malformed("chunk data is truncated"))?;
        output.extend_from_slice(&body[position..end]);
        position = end;
        if body.get(position..position + 2) != Some(b"\r\n") {
            return Err(malformed("chunk data is not CRLF-terminated"));
        }
        position += 2;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_body_decodes() {
        let body = b"4\r\nmoch\r\n1\r\na\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(body).unwrap(), b"mocha");
    }

    #[test]
    fn chunked_with_extension_and_trailers_decodes() {
        let body = b"5;ext=1\r\nhello\r\n0\r\nX-Trailer: ignored\r\n\r\n";
        assert_eq!(decode_chunked(body).unwrap(), b"hello");
    }

    #[test]
    fn chunked_truncated_data_errors() {
        let body = b"10\r\nshort\r\n0\r\n\r\n";
        let error = decode_chunked(body).unwrap_err();
        assert!(error.to_string().contains("chunk data is truncated"));
    }

    #[test]
    fn chunked_missing_terminator_errors() {
        let body = b"5\r\nhello\r\n";
        assert!(decode_chunked(body).is_err());
    }

    #[test]
    fn chunked_bad_size_errors() {
        let body = b"zz\r\nhello\r\n0\r\n\r\n";
        let error = decode_chunked(body).unwrap_err();
        assert!(error.to_string().contains("hexadecimal"));
    }

    #[test]
    fn chunked_garbage_after_final_chunk_errors() {
        let body = b"1\r\na\r\n0\r\n\r\nextra";
        let error = decode_chunked(body).unwrap_err();
        assert!(error.to_string().contains("after the final chunk"));
    }
}
