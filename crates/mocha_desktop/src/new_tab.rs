//! Internal page placeholders.
//!
//! Tabs showing a native view ([`crate::tab::InternalView`], drawn by
//! [`crate::views`]) still own a rendered document so the rest of the shell
//! (resize, display-list accessors) keeps working; that document is this blank
//! placeholder. It never issues a request (no base URL) and is never visible —
//! the native view paints over the whole viewport.

/// An internally generated page. These render from a fixed HTML string and have
/// no external URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalPage {
    /// The blank placeholder behind native views (new tab, load error).
    NewTab,
}

/// The title shown for a new tab.
pub const NEW_TAB_TITLE: &str = "New Tab";

/// The tab title shown when a document fails to load.
pub const LOAD_ERROR_TITLE: &str = "Problem loading page";

/// The placeholder document behind native views (blank; the native view is
/// the visible content). Uses only Mocha's supported subset.
pub const NEW_TAB_HTML: &str = "<!doctype html>\n<html>\n  <body>\n  </body>\n</html>";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_parses_and_renders_blank() {
        let page = mocha_engine::render_html(
            InternalPage::NewTab.html(),
            &mocha_engine::RenderOptions::default(),
        )
        .expect("placeholder renders");
        // No text is painted: the native view is the visible content.
        let painted = mocha_paint::format_display_list(&page.display_list);
        assert!(!painted.contains("DrawText"));
    }

    #[test]
    fn titles_are_stable() {
        assert_eq!(InternalPage::NewTab.title(), "New Tab");
        assert_eq!(LOAD_ERROR_TITLE, "Problem loading page");
    }
}
