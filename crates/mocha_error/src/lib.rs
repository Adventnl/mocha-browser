//! Shared error types for Mocha Browser.
//!
//! Every crate in the workspace returns [`MochaResult`] so that errors compose
//! cleanly across the rendering pipeline. There is intentionally no `From`
//! conversion between [`MochaError`] variants: each crate constructs the variant
//! that matches its own responsibility, which keeps error messages specific.

use std::error::Error;
use std::fmt;

/// Convenience alias used throughout the workspace.
pub type MochaResult<T> = Result<T, MochaError>;

/// The single error type shared by all Mocha crates.
///
/// Each variant carries a human-readable message. Unsupported behaviour for the
/// current milestone is reported through [`MochaError::UnsupportedFeature`] or
/// [`MochaError::NotImplemented`] rather than being silently ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MochaError {
    /// An I/O failure, typically while reading a local file.
    Io(String),
    /// A parsing failure (HTML tokenizer or tree builder).
    Parse(String),
    /// A URL that is syntactically invalid or missing required parts.
    InvalidUrl(String),
    /// A feature that exists in real browsers but is intentionally out of scope.
    UnsupportedFeature(String),
    /// A feature planned for a later milestone that has no implementation yet.
    NotImplemented(String),
    /// A DOM-tree invariant violation (for example an invalid node id).
    Dom(String),
    /// A layout failure.
    Layout(String),
    /// A paint / display-list failure.
    Paint(String),
    /// An image decoding/format failure.
    Image(String),
    /// A compressed-data (gzip/DEFLATE) decoding failure.
    Decompression(String),
    /// A network/resource-loading failure (connection, protocol, redirect).
    Network(String),
    /// A navigation/history failure (e.g. no previous entry to go back to).
    Navigation(String),
    /// A JavaScript runtime error (undefined variable, bad call, step limit, …).
    JavaScript(String),
    /// A profile/storage failure (database, migration, or persistence).
    Storage(String),
    /// A security/origin-policy failure (e.g. storage on an opaque origin).
    Security(String),
    /// A failure in the command-line shell that wires the pipeline together.
    Shell(String),
}

impl fmt::Display for MochaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MochaError::Io(message) => write!(f, "io error: {message}"),
            MochaError::Parse(message) => write!(f, "parse error: {message}"),
            MochaError::InvalidUrl(message) => write!(f, "invalid url: {message}"),
            MochaError::UnsupportedFeature(message) => {
                write!(f, "unsupported feature: {message}")
            }
            MochaError::NotImplemented(message) => {
                write!(f, "not implemented: {message}")
            }
            MochaError::Dom(message) => write!(f, "dom error: {message}"),
            MochaError::Layout(message) => write!(f, "layout error: {message}"),
            MochaError::Paint(message) => write!(f, "paint error: {message}"),
            MochaError::Image(message) => write!(f, "image error: {message}"),
            MochaError::Decompression(message) => {
                write!(f, "decompression error: {message}")
            }
            MochaError::Network(message) => write!(f, "network error: {message}"),
            MochaError::Navigation(message) => write!(f, "navigation error: {message}"),
            MochaError::JavaScript(message) => write!(f, "javascript error: {message}"),
            MochaError::Storage(message) => write!(f, "storage error: {message}"),
            MochaError::Security(message) => write!(f, "security error: {message}"),
            MochaError::Shell(message) => write!(f, "shell error: {message}"),
        }
    }
}

impl Error for MochaError {}

impl From<std::io::Error> for MochaError {
    fn from(error: std::io::Error) -> Self {
        MochaError::Io(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_converts_into_mocha_io_variant() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing file");
        let mocha: MochaError = io.into();
        match mocha {
            MochaError::Io(message) => assert!(message.contains("missing file")),
            other => panic!("expected Io variant, got {other:?}"),
        }
    }

    #[test]
    fn display_message_is_useful() {
        let error = MochaError::Parse("mismatched closing tag".to_string());
        assert_eq!(error.to_string(), "parse error: mismatched closing tag");
    }

    #[test]
    fn unsupported_feature_message_is_clear() {
        let error = MochaError::UnsupportedFeature("tag <img> is not supported".to_string());
        let text = error.to_string();
        assert!(text.starts_with("unsupported feature:"));
        assert!(text.contains("<img>"));
    }

    #[test]
    fn error_is_usable_as_std_error_trait_object() {
        let error: Box<dyn Error> = Box::new(MochaError::Shell("boom".to_string()));
        assert_eq!(error.to_string(), "shell error: boom");
    }
}
