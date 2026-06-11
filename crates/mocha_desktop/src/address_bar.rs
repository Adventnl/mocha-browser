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
        }
    }

    /// Set the current URL and reset the draft.
    pub fn set_current_url(&mut self, url: Option<Url>) {
        self.current_url = url.clone();
        self.draft_text = url.map(|u| u.normalized()).unwrap_or_default();
        self.focused = false;
    }

    /// User focused the address bar: prepare to edit.
    pub fn focus(&mut self) {
        if !self.focused {
            self.focused = true;
            // Draft starts as the current URL.
            self.draft_text = self
                .current_url
                .as_ref()
                .map(|u| u.normalized())
                .unwrap_or_default();
        }
    }

    /// User blurred the address bar: cancel editing.
    pub fn blur(&mut self) {
        self.focused = false;
        self.draft_text = self
            .current_url
            .as_ref()
            .map(|u| u.normalized())
            .unwrap_or_default();
    }

    /// User typed a character.
    pub fn input_char(&mut self, c: char) {
        if self.focused {
            self.draft_text.push(c);
        }
    }

    /// User pressed backspace.
    pub fn backspace(&mut self) {
        if self.focused {
            self.draft_text.pop();
        }
    }

    /// User pressed Enter: attempt to parse the draft as a URL.
    pub fn submit(&mut self) -> Option<Url> {
        if !self.focused || self.draft_text.is_empty() {
            return None;
        }
        self.focused = false;

        let url = Url::parse(&self.draft_text).ok()?;
        self.current_url = Some(url.clone());
        Some(url)
    }

    /// User pressed Escape: cancel editing, restore the current URL.
    pub fn cancel(&mut self) {
        self.focused = false;
        self.draft_text = self
            .current_url
            .as_ref()
            .map(|u| u.normalized())
            .unwrap_or_default();
    }
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
