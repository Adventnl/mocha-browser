//! Internal browser pages rendered without touching the network.
//!
//! Milestone 13 added the new-tab page; the Windows app packaging added the
//! load-error page. Both are plain HTML in Mocha's supported subset, rendered
//! through the normal pipeline as in-memory documents (no base URL, so they
//! never issue a request).

/// An internally generated page. These render from a fixed HTML string and have
/// no external URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalPage {
    /// The blank new-tab / home page.
    NewTab,
}

/// The title shown for a new tab.
pub const NEW_TAB_TITLE: &str = "New Tab";

/// The HTML rendered for the new-tab / home page (also shown when the app is
/// launched with no argument). Uses only Mocha's supported subset (`doctype`,
/// `html`, `body`, `h1`, `p`).
pub const NEW_TAB_HTML: &str = r#"<!doctype html>
<html>
  <body>
    <h1>Mocha Browser</h1>
    <p>Experimental browser engine.</p>
    <p>Enter a local path or URL in the address bar.</p>
  </body>
</html>"#;

/// The tab title shown when the initial document fails to load.
pub const LOAD_ERROR_TITLE: &str = "Problem loading page";

/// Build the internal error page shown when a document fails to load. Uses
/// only Mocha's supported subset; `input` and `message` pass through
/// [`sanitize_for_page`] so they can never inject markup.
pub fn load_error_html(input: &str, message: &str) -> String {
    format!(
        r#"<!doctype html>
<html>
  <body>
    <h1>Problem loading page</h1>
    <p>{}</p>
    <p>{}</p>
    <p>Enter a local path or URL in the address bar.</p>
  </body>
</html>"#,
        sanitize_for_page(input),
        sanitize_for_page(message)
    )
}

/// Make untrusted text safe to embed in an internal page. Mocha's tokenizer
/// has no character-reference (entity) support, so `&lt;`-style escaping would
/// render literally; instead the only markup-significant characters are
/// substituted: `<` becomes `(` and `>` becomes `)` (both in the raster debug
/// font). Error text like `tag <head> is not supported` renders as
/// `tag (head) is not supported`.
fn sanitize_for_page(text: &str) -> String {
    text.replace('<', "(").replace('>', ")")
}

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
    fn error_page_contains_input_and_message() {
        let html = load_error_html("examples/missing.html", "io error: not found");
        assert!(html.contains("examples/missing.html"));
        assert!(html.contains("io error: not found"));
        assert!(html.contains("Problem loading page"));
    }

    #[test]
    fn error_page_neutralizes_markup_in_untrusted_text() {
        let html = load_error_html(
            "<script>boom</script>",
            "unsupported feature: tag <head> is not supported",
        );
        // The angle brackets are substituted, so the page contains no tags
        // beyond the fixed template.
        assert!(html.contains("(script)boom(/script)"));
        assert!(html.contains("tag (head) is not supported"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("<head"));
    }

    #[test]
    fn error_page_renders_through_the_engine() {
        // The sanitized page must actually parse and paint its text.
        let html = load_error_html("https://example.com/", "tag <head> is not supported");
        let page = mocha_engine::render_html(&html, &mocha_engine::RenderOptions::default())
            .expect("error page renders");
        let painted = mocha_paint::format_display_list(&page.display_list);
        assert!(painted.contains("(head)"));
        assert!(painted.contains("https://example.com/"));
    }
}
