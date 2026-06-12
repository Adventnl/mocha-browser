//! Address bar state and input handling.

use mocha_url::Url;

/// The address bar editing state.
#[derive(Debug, Clone)]
pub struct AddressBarState {
    /// The currently displayed URL (what was last navigated to).
    pub current_url: Option<Url>,
    /// The text being edited in the address bar.
    pub draft_text: String,
    /// Whether the address bar is currently focused/editing.
    pub focused: bool,
    /// When focused, the whole draft is "selected": the next character typed (or
    /// backspace) replaces it, like a real browser's Ctrl+L / click-to-focus.
    pub select_all: bool,
}

impl AddressBarState {
    pub fn new(url: Option<Url>) -> Self {
        let current_url = url.clone();
        let draft_text = current_url
            .as_ref()
            .map(|u| u.normalized())
            .unwrap_or_default();
        Self {
            current_url,
            draft_text,
            focused: false,
            select_all: false,
        }
    }

    /// Set the current URL and reset the draft.
    pub fn set_current_url(&mut self, url: Option<Url>) {
        self.current_url = url.clone();
        self.draft_text = url.map(|u| u.normalized()).unwrap_or_default();
        self.focused = false;
        self.select_all = false;
    }

    /// User focused the address bar: prepare to edit with the URL selected.
    pub fn focus(&mut self) {
        if !self.focused {
            self.focused = true;
            // Draft starts as the current URL, fully selected (replace on type).
            self.draft_text = self
                .current_url
                .as_ref()
                .map(|u| u.normalized())
                .unwrap_or_default();
            self.select_all = !self.draft_text.is_empty();
        }
    }

    /// User blurred the address bar: cancel editing.
    pub fn blur(&mut self) {
        self.focused = false;
        self.select_all = false;
        self.draft_text = self
            .current_url
            .as_ref()
            .map(|u| u.normalized())
            .unwrap_or_default();
    }

    /// User typed a character (replacing the selection if the draft is selected).
    pub fn input_char(&mut self, c: char) {
        if self.focused {
            if self.select_all {
                self.draft_text.clear();
                self.select_all = false;
            }
            self.draft_text.push(c);
        }
    }

    /// User pressed backspace (clears a selected draft, else deletes one char).
    pub fn backspace(&mut self) {
        if self.focused {
            if self.select_all {
                self.draft_text.clear();
                self.select_all = false;
            } else {
                self.draft_text.pop();
            }
        }
    }

    /// User pressed Enter: resolve the draft as a URL or a web search (see
    /// [`resolve_query`]). Returns the URL to navigate to.
    pub fn submit(&mut self) -> Option<Url> {
        if !self.focused || self.draft_text.trim().is_empty() {
            return None;
        }
        self.focused = false;
        self.select_all = false;

        let url = resolve_query(&self.draft_text)?;
        self.current_url = Some(url.clone());
        Some(url)
    }

    /// User pressed Escape: cancel editing, restore the current URL.
    pub fn cancel(&mut self) {
        self.focused = false;
        self.select_all = false;
        self.draft_text = self
            .current_url
            .as_ref()
            .map(|u| u.normalized())
            .unwrap_or_default();
    }
}

/// The search engine the address bar uses when the draft is not a URL.
/// `{query}` is replaced with the percent-encoded search terms.
const SEARCH_URL_TEMPLATE: &str = "https://www.google.com/search?q={query}";

/// Turn an address-bar entry into a navigation target, like a browser omnibox:
///
/// 1. An entry with an explicit scheme (`http://`, `https://`, `file://`) is
///    parsed as-is.
/// 2. An entry that looks like a bare host (`example.com`, `localhost:8080`,
///    `example.com/path`) gets an `https://` scheme and is parsed as a URL.
/// 3. Anything else (it has spaces, or no host-like dotted name) becomes a web
///    search: the text is percent-encoded into [`SEARCH_URL_TEMPLATE`].
pub fn resolve_query(input: &str) -> Option<Url> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        return Url::parse(trimmed).ok();
    }
    if looks_like_host(trimmed) {
        return Url::parse(&format!("https://{trimmed}")).ok();
    }
    let target = SEARCH_URL_TEMPLATE.replace("{query}", &percent_encode_query(trimmed));
    Url::parse(&target).ok()
}

/// Whether `input` (with no scheme) looks like a host to visit rather than a
/// search query: a single token whose authority is `localhost` or a dotted name
/// ending in an alphabetic TLD label.
fn looks_like_host(input: &str) -> bool {
    if input.split_whitespace().count() != 1 {
        return false;
    }
    // The authority is everything before the first path/query/fragment.
    let authority = input.split(['/', '?', '#']).next().unwrap_or(input);
    let host = authority.split(':').next().unwrap_or(authority);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|label| label.is_empty()) {
        return false;
    }
    let tld = labels[labels.len() - 1];
    tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

/// Percent-encode search terms for a query string: ASCII alphanumerics and
/// `-`, `.`, `_`, `~` pass through, spaces become `+`, every other byte becomes
/// `%XX` (UTF-8 for non-ASCII).
fn percent_encode_query(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.trim().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push('+'),
            other => {
                encoded.push('%');
                encoded.push(
                    char::from_digit((other >> 4) as u32, 16)
                        .unwrap()
                        .to_ascii_uppercase(),
                );
                encoded.push(
                    char::from_digit((other & 0x0f) as u32, 16)
                        .unwrap()
                        .to_ascii_uppercase(),
                );
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_bar_initializes_with_url() {
        let url = Url::parse("http://example.com/").ok();
        let state = AddressBarState::new(url.clone());
        assert_eq!(state.current_url, url);
        assert!(!state.focused);
    }

    #[test]
    fn focus_enables_editing() {
        let mut state = AddressBarState::new(None);
        state.focus();
        assert!(state.focused);
    }

    #[test]
    fn input_char_when_focused() {
        let mut state = AddressBarState::new(None);
        state.focus();
        state.input_char('h');
        assert_eq!(state.draft_text, "h");
    }

    #[test]
    fn backspace_deletes_char() {
        let mut state = AddressBarState::new(None);
        state.focus();
        state.input_char('a');
        state.input_char('b');
        state.backspace();
        assert_eq!(state.draft_text, "a");
    }

    #[test]
    fn submit_parses_url() {
        let mut state = AddressBarState::new(None);
        state.focus();
        state.draft_text = "http://example.com/".to_string();
        let url = state.submit();
        assert!(url.is_some());
        assert!(state.current_url.is_some());
    }

    #[test]
    fn submit_when_not_focused_returns_none() {
        let mut state = AddressBarState::new(None);
        let url = state.submit();
        assert!(url.is_none());
    }

    #[test]
    fn submit_a_bare_domain_gets_https() {
        let mut state = AddressBarState::new(None);
        state.focus();
        state.draft_text = "example.com".to_string();
        let url = state.submit().unwrap();
        assert_eq!(url.normalized(), "https://example.com/");
    }

    #[test]
    fn submit_a_search_query_goes_to_google() {
        let mut state = AddressBarState::new(None);
        state.focus();
        state.draft_text = "hello world".to_string();
        let url = state.submit().unwrap();
        assert_eq!(
            url.normalized(),
            "https://www.google.com/search?q=hello+world"
        );
    }

    #[test]
    fn resolve_query_classifies_input() {
        // Explicit schemes are kept verbatim.
        assert_eq!(
            resolve_query("https://news.example/path")
                .unwrap()
                .normalized(),
            "https://news.example/path"
        );
        // Bare hosts (with a path / port) become https URLs.
        assert_eq!(
            resolve_query("example.com/a/b").unwrap().normalized(),
            "https://example.com/a/b"
        );
        assert_eq!(
            resolve_query("localhost:8080").unwrap().normalized(),
            "https://localhost:8080/"
        );
        // Single words and dotless text are searches, not hosts.
        assert!(resolve_query("google")
            .unwrap()
            .normalized()
            .starts_with("https://www.google.com/search?q="));
        // A word with a space is always a search even if it contains a dot.
        assert_eq!(
            resolve_query("what is rust 1.0").unwrap().normalized(),
            "https://www.google.com/search?q=what+is+rust+1.0"
        );
    }

    #[test]
    fn search_query_percent_encodes_special_characters() {
        let url = resolve_query("rust & c++").unwrap();
        assert_eq!(
            url.normalized(),
            "https://www.google.com/search?q=rust+%26+c%2B%2B"
        );
    }

    #[test]
    fn looks_like_host_distinguishes_hosts_from_searches() {
        assert!(looks_like_host("example.com"));
        assert!(looks_like_host("sub.example.co.uk/path"));
        assert!(looks_like_host("localhost"));
        assert!(!looks_like_host("just text"));
        assert!(!looks_like_host("single"));
        assert!(!looks_like_host("ends.in.123")); // numeric TLD => search
    }

    #[test]
    fn cancel_restores_current_url() {
        let url = Url::parse("http://example.com/").ok();
        let mut state = AddressBarState::new(url.clone());
        state.focus();
        state.draft_text = "garbage".to_string();
        state.cancel();
        assert!(!state.focused);
        assert_eq!(state.draft_text, "http://example.com/");
    }
}
