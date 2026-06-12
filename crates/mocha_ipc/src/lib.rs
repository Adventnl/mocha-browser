//! Typed IPC protocol for Mocha's M17 multi-process prototype.
//!
//! The transport is deliberately simple: one newline-delimited frame per
//! message, with tab-separated fields and hex-encoded strings. This avoids a
//! serialization dependency while still giving the browser and renderer a typed,
//! versioned protocol. It is a prototype protocol, not a production IPC system.

use std::io::{BufRead, Write};

use mocha_error::{MochaError, MochaResult};

/// Protocol version understood by this build.
pub const IPC_PROTOCOL_VERSION: u32 = 1;

/// Maximum frame size accepted by M17 (16 MiB).
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Browser-to-renderer messages.
#[derive(Debug, Clone, PartialEq)]
pub enum BrowserToRenderer {
    Ping {
        id: u64,
    },
    RenderDocument {
        id: u64,
        input: String,
        viewport_width: u32,
        viewport_height: u32,
    },
    RenderHtml {
        id: u64,
        html: String,
        base_url: Option<String>,
        viewport_width: u32,
        viewport_height: u32,
    },
    SetSandboxPolicy {
        allow_direct_document_loads: bool,
    },
    RenderPreparedDocument {
        id: u64,
        document: PreparedDocument,
        viewport_width: u32,
        viewport_height: u32,
    },
    Shutdown,
    CrashForTest,
}

/// Document bytes prepared by the browser process for a restricted renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDocument {
    pub final_url: Option<String>,
    pub html: String,
}

/// Lightweight renderer output. The full DOM/layout/page is not serialized.
#[derive(Debug, Clone, PartialEq)]
pub struct RendererPageSnapshot {
    pub final_url: Option<String>,
    pub document_height: f32,
    pub display_list_len: usize,
    pub console_output: Vec<String>,
}

/// Renderer-to-browser messages.
#[derive(Debug, Clone, PartialEq)]
pub enum RendererToBrowser {
    Pong { id: u64 },
    Rendered { id: u64, page: RendererPageSnapshot },
    Error { id: u64, message: String },
    Log { message: String },
    Goodbye,
}

/// Write one browser-to-renderer frame.
pub fn write_browser_message<W: Write>(
    writer: &mut W,
    message: &BrowserToRenderer,
) -> MochaResult<()> {
    write_frame(writer, &encode_browser(message))
}

/// Write one renderer-to-browser frame.
pub fn write_renderer_message<W: Write>(
    writer: &mut W,
    message: &RendererToBrowser,
) -> MochaResult<()> {
    write_frame(writer, &encode_renderer(message))
}

/// Read one browser-to-renderer frame.
pub fn read_browser_message<R: BufRead>(reader: &mut R) -> MochaResult<Option<BrowserToRenderer>> {
    read_frame(reader)?
        .map(|line| decode_browser(&line))
        .transpose()
}

/// Read one renderer-to-browser frame.
pub fn read_renderer_message<R: BufRead>(reader: &mut R) -> MochaResult<Option<RendererToBrowser>> {
    read_frame(reader)?
        .map(|line| decode_renderer(&line))
        .transpose()
}

fn write_frame<W: Write>(writer: &mut W, line: &str) -> MochaResult<()> {
    if line.len() > MAX_FRAME_SIZE {
        return Err(MochaError::Network("IPC frame is too large".to_string()));
    }
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn read_frame<R: BufRead>(reader: &mut R) -> MochaResult<Option<String>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    if line.len() > MAX_FRAME_SIZE {
        return Err(MochaError::Network("IPC frame is too large".to_string()));
    }
    while line.ends_with(['\n', '\r']) {
        line.pop();
    }
    Ok(Some(line))
}

fn encode_browser(message: &BrowserToRenderer) -> String {
    match message {
        BrowserToRenderer::Ping { id } => format!("{}\tPing\t{id}", IPC_PROTOCOL_VERSION),
        BrowserToRenderer::RenderDocument {
            id,
            input,
            viewport_width,
            viewport_height,
        } => format!(
            "{}\tRenderDocument\t{id}\t{}\t{viewport_width}\t{viewport_height}",
            IPC_PROTOCOL_VERSION,
            hex(input)
        ),
        BrowserToRenderer::RenderHtml {
            id,
            html,
            base_url,
            viewport_width,
            viewport_height,
        } => format!(
            "{}\tRenderHtml\t{id}\t{}\t{}\t{viewport_width}\t{viewport_height}",
            IPC_PROTOCOL_VERSION,
            hex(html),
            base_url
                .as_deref()
                .map(hex)
                .unwrap_or_else(|| "-".to_string())
        ),
        BrowserToRenderer::SetSandboxPolicy {
            allow_direct_document_loads,
        } => format!(
            "{}\tSetSandboxPolicy\t{}",
            IPC_PROTOCOL_VERSION,
            if *allow_direct_document_loads {
                "1"
            } else {
                "0"
            }
        ),
        BrowserToRenderer::RenderPreparedDocument {
            id,
            document,
            viewport_width,
            viewport_height,
        } => format!(
            "{}\tRenderPreparedDocument\t{id}\t{}\t{}\t{viewport_width}\t{viewport_height}",
            IPC_PROTOCOL_VERSION,
            document
                .final_url
                .as_deref()
                .map(hex)
                .unwrap_or_else(|| "-".to_string()),
            hex(&document.html)
        ),
        BrowserToRenderer::Shutdown => format!("{}\tShutdown", IPC_PROTOCOL_VERSION),
        BrowserToRenderer::CrashForTest => format!("{}\tCrashForTest", IPC_PROTOCOL_VERSION),
    }
}

fn decode_browser(line: &str) -> MochaResult<BrowserToRenderer> {
    let parts = split(line)?;
    match parts.as_slice() {
        [_, "Ping", id] => Ok(BrowserToRenderer::Ping {
            id: parse(id, "id")?,
        }),
        [_, "RenderDocument", id, input, width, height] => Ok(BrowserToRenderer::RenderDocument {
            id: parse(id, "id")?,
            input: unhex(input)?,
            viewport_width: parse(width, "viewport_width")?,
            viewport_height: parse(height, "viewport_height")?,
        }),
        [_, "RenderHtml", id, html, base_url, width, height] => Ok(BrowserToRenderer::RenderHtml {
            id: parse(id, "id")?,
            html: unhex(html)?,
            base_url: if *base_url == "-" {
                None
            } else {
                Some(unhex(base_url)?)
            },
            viewport_width: parse(width, "viewport_width")?,
            viewport_height: parse(height, "viewport_height")?,
        }),
        [_, "SetSandboxPolicy", allow] => Ok(BrowserToRenderer::SetSandboxPolicy {
            allow_direct_document_loads: match *allow {
                "0" => false,
                "1" => true,
                other => {
                    return Err(MochaError::Network(format!(
                        "invalid sandbox policy allow flag: {other}"
                    )))
                }
            },
        }),
        [_, "RenderPreparedDocument", id, final_url, html, width, height] => {
            Ok(BrowserToRenderer::RenderPreparedDocument {
                id: parse(id, "id")?,
                document: PreparedDocument {
                    final_url: if *final_url == "-" {
                        None
                    } else {
                        Some(unhex(final_url)?)
                    },
                    html: unhex(html)?,
                },
                viewport_width: parse(width, "viewport_width")?,
                viewport_height: parse(height, "viewport_height")?,
            })
        }
        [_, "Shutdown"] => Ok(BrowserToRenderer::Shutdown),
        [_, "CrashForTest"] => Ok(BrowserToRenderer::CrashForTest),
        _ => Err(MochaError::Network(format!(
            "unknown browser IPC message: {line}"
        ))),
    }
}

fn encode_renderer(message: &RendererToBrowser) -> String {
    match message {
        RendererToBrowser::Pong { id } => format!("{}\tPong\t{id}", IPC_PROTOCOL_VERSION),
        RendererToBrowser::Rendered { id, page } => format!(
            "{}\tRendered\t{id}\t{}\t{}\t{}\t{}",
            IPC_PROTOCOL_VERSION,
            page.final_url
                .as_deref()
                .map(hex)
                .unwrap_or_else(|| "-".to_string()),
            page.document_height,
            page.display_list_len,
            hex(&page.console_output.join("\u{1f}"))
        ),
        RendererToBrowser::Error { id, message } => {
            format!("{}\tError\t{id}\t{}", IPC_PROTOCOL_VERSION, hex(message))
        }
        RendererToBrowser::Log { message } => {
            format!("{}\tLog\t{}", IPC_PROTOCOL_VERSION, hex(message))
        }
        RendererToBrowser::Goodbye => format!("{}\tGoodbye", IPC_PROTOCOL_VERSION),
    }
}

fn decode_renderer(line: &str) -> MochaResult<RendererToBrowser> {
    let parts = split(line)?;
    match parts.as_slice() {
        [_, "Pong", id] => Ok(RendererToBrowser::Pong {
            id: parse(id, "id")?,
        }),
        [_, "Rendered", id, final_url, height, display_len, console] => {
            let console_text = unhex(console)?;
            let console_output = if console_text.is_empty() {
                Vec::new()
            } else {
                console_text.split('\u{1f}').map(str::to_string).collect()
            };
            Ok(RendererToBrowser::Rendered {
                id: parse(id, "id")?,
                page: RendererPageSnapshot {
                    final_url: if *final_url == "-" {
                        None
                    } else {
                        Some(unhex(final_url)?)
                    },
                    document_height: parse(height, "document_height")?,
                    display_list_len: parse(display_len, "display_list_len")?,
                    console_output,
                },
            })
        }
        [_, "Error", id, message] => Ok(RendererToBrowser::Error {
            id: parse(id, "id")?,
            message: unhex(message)?,
        }),
        [_, "Log", message] => Ok(RendererToBrowser::Log {
            message: unhex(message)?,
        }),
        [_, "Goodbye"] => Ok(RendererToBrowser::Goodbye),
        _ => Err(MochaError::Network(format!(
            "unknown renderer IPC message: {line}"
        ))),
    }
}

fn split(line: &str) -> MochaResult<Vec<&str>> {
    let parts: Vec<&str> = line.split('\t').collect();
    match parts.first().and_then(|v| v.parse::<u32>().ok()) {
        Some(IPC_PROTOCOL_VERSION) => Ok(parts),
        Some(version) => Err(MochaError::Network(format!(
            "IPC protocol version mismatch: got {version}, expected {IPC_PROTOCOL_VERSION}"
        ))),
        None => Err(MochaError::Network(
            "IPC frame is missing a valid protocol version".to_string(),
        )),
    }
}

fn parse<T: std::str::FromStr>(value: &str, field: &str) -> MochaResult<T> {
    value
        .parse()
        .map_err(|_| MochaError::Network(format!("invalid IPC field {field}: {value}")))
}

fn hex(input: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input.as_bytes() {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn unhex(input: &str) -> MochaResult<String> {
    if !input.len().is_multiple_of(2) {
        return Err(MochaError::Network(
            "hex IPC string has odd length".to_string(),
        ));
    }
    let mut bytes = Vec::with_capacity(input.len() / 2);
    for pair in input.as_bytes().chunks_exact(2) {
        let hi = hex_value(pair[0])?;
        let lo = hex_value(pair[1])?;
        bytes.push((hi << 4) | lo);
    }
    String::from_utf8(bytes).map_err(|_| MochaError::Network("IPC string is not UTF-8".to_string()))
}

fn hex_value(byte: u8) -> MochaResult<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(MochaError::Network(
            "invalid hex digit in IPC string".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn browser_message_round_trips() {
        let message = BrowserToRenderer::RenderHtml {
            id: 7,
            html: "<p>hi</p>".to_string(),
            base_url: Some("http://example.com/".to_string()),
            viewport_width: 800,
            viewport_height: 600,
        };
        let mut bytes = Vec::new();
        write_browser_message(&mut bytes, &message).unwrap();
        let decoded = read_browser_message(&mut Cursor::new(bytes))
            .unwrap()
            .unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn prepared_document_message_round_trips() {
        let message = BrowserToRenderer::RenderPreparedDocument {
            id: 11,
            document: PreparedDocument {
                final_url: Some("file:///tmp/index.html".to_string()),
                html: "<html><body>prepared</body></html>".to_string(),
            },
            viewport_width: 320,
            viewport_height: 240,
        };
        let mut bytes = Vec::new();
        write_browser_message(&mut bytes, &message).unwrap();
        let decoded = read_browser_message(&mut Cursor::new(bytes))
            .unwrap()
            .unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn renderer_message_round_trips() {
        let message = RendererToBrowser::Rendered {
            id: 9,
            page: RendererPageSnapshot {
                final_url: Some("file:///tmp/a.html".to_string()),
                document_height: 42.0,
                display_list_len: 3,
                console_output: vec!["hello".to_string()],
            },
        };
        let mut bytes = Vec::new();
        write_renderer_message(&mut bytes, &message).unwrap();
        let decoded = read_renderer_message(&mut Cursor::new(bytes))
            .unwrap()
            .unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn invalid_json_like_garbage_is_protocol_error_not_panic() {
        let mut cursor = Cursor::new(b"not\tvalid\n".to_vec());
        assert!(read_browser_message(&mut cursor).is_err());
    }

    #[test]
    fn wrong_protocol_version_is_rejected() {
        let mut cursor = Cursor::new(b"999\tPing\t1\n".to_vec());
        assert!(read_browser_message(&mut cursor).is_err());
    }

    #[test]
    fn oversized_frame_is_rejected() {
        let mut line = "1\tPing\t1".to_string();
        line.push_str(&"x".repeat(MAX_FRAME_SIZE));
        line.push('\n');
        let mut cursor = Cursor::new(line.into_bytes());
        assert!(read_browser_message(&mut cursor).is_err());
    }
}
