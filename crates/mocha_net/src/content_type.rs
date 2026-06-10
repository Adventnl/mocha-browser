//! Content-type classification used to decide what a response *is*.

use mocha_url::Url;

/// A coarse classification of a loaded resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// `text/html` (or a missing type on a `.html`/`.htm` URL).
    Html,
    /// `text/css`.
    Css,
    /// `text/plain`.
    Text,
    /// A known-binary type (`application/octet-stream`, `image/*`).
    Binary,
    /// Anything else, or an unrecognised extension with no content type.
    Unknown,
}

/// Classify a resource from its `Content-Type` (if any) and URL.
///
/// The MIME's parameters (e.g. `; charset=utf-8`) are ignored. When the content
/// type is absent, the URL's file extension is used as a fallback.
pub fn classify(content_type: Option<&str>, url: &Url) -> ResourceType {
    if let Some(content_type) = content_type {
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        return match mime.as_str() {
            "text/html" => ResourceType::Html,
            "text/css" => ResourceType::Css,
            "text/plain" => ResourceType::Text,
            "application/octet-stream" => ResourceType::Binary,
            other if other.starts_with("image/") => ResourceType::Binary,
            "" => classify_by_extension(url),
            _ => ResourceType::Unknown,
        };
    }
    classify_by_extension(url)
}

fn classify_by_extension(url: &Url) -> ResourceType {
    let path = url.path.to_ascii_lowercase();
    if path.ends_with(".html") || path.ends_with(".htm") {
        ResourceType::Html
    } else if path.ends_with(".css") {
        ResourceType::Css
    } else if path.ends_with(".txt") {
        ResourceType::Text
    } else {
        ResourceType::Unknown
    }
}

/// The default content type for a local file, inferred from its extension.
pub(crate) fn content_type_for_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html"
    } else if lower.ends_with(".css") {
        "text/css"
    } else if lower.ends_with(".txt") {
        "text/plain"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn html_with_charset_is_html() {
        assert_eq!(
            classify(Some("text/html; charset=utf-8"), &url("http://x/a")),
            ResourceType::Html
        );
    }

    #[test]
    fn plain_text_is_text() {
        assert_eq!(
            classify(Some("text/plain"), &url("http://x/a")),
            ResourceType::Text
        );
    }

    #[test]
    fn octet_stream_is_binary() {
        assert_eq!(
            classify(Some("application/octet-stream"), &url("http://x/a")),
            ResourceType::Binary
        );
    }

    #[test]
    fn missing_type_falls_back_to_extension() {
        assert_eq!(
            classify(None, &url("http://x/page.html")),
            ResourceType::Html
        );
        assert_eq!(
            classify(None, &url("http://x/data.bin")),
            ResourceType::Unknown
        );
    }

    #[test]
    fn extension_content_types() {
        assert_eq!(content_type_for_path("a/b.html"), "text/html");
        assert_eq!(content_type_for_path("a/b.css"), "text/css");
        assert_eq!(content_type_for_path("a/b.bin"), "application/octet-stream");
    }
}
