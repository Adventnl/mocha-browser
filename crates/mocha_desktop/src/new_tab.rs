//! Internal browser pages rendered without touching the network.
//!
//! Milestone 13 only has the new-tab page. It is plain HTML in Mocha's supported
//! subset, rendered through the normal pipeline as an in-memory document (no base
//! URL, so it never issues a request).

/// An internally generated page. These render from a fixed HTML string and have
/// no external URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalPage {
    /// The blank new-tab page.
    NewTab,
}

/// The title shown for a new tab.
pub const NEW_TAB_TITLE: &str = "New Tab";

/// The HTML rendered for the new-tab page. Uses only Mocha's supported subset
/// (`doctype`, `html`, `body`, `h1`, `p`).
pub const NEW_TAB_HTML: &str = r#"<!doctype html>
<html>
  <body>
    <h1>Mocha Browser</h1>
    <p>Enter a local path or http:// URL in the address bar.</p>
  </body>
</html>"#;

impl InternalPage {
    /// The HTML source for this internal page.
    pub fn html(self) -> &'static str {
        match self {
            InternalPage::NewTab => NEW_TAB_HTML,
        }
    }

    /// The tab title for this internal page.
    pub fn title(self) -> &'static str {
        match self {
            InternalPage::NewTab => NEW_TAB_TITLE,
        }
    }
}
