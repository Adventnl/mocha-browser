//! Navigation controller and history for Mocha Browser.
//!
//! `mocha_nav` owns the back/forward history and turns navigation actions into
//! resource loads via a [`ResourceLoader`]. It does **not** know about the HTTP
//! protocol, HTML, CSS, layout, or painting — rendering a loaded response is the
//! shell's job. History stores each entry's **final** URL (after redirects).

mod default_action;

pub use default_action::{default_action_for_event, DefaultAction};

use mocha_error::{MochaError, MochaResult};
use mocha_net::{LoadRequest, ResourceLoader, ResourceResponse};
use mocha_url::Url;

/// One entry in the navigation history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationEntry {
    /// The entry's URL (the final URL after any redirects).
    pub url: Url,
    /// The document title, if known (always `None` today — no `<title>` yet).
    pub title: Option<String>,
}

/// A navigation request the controller can dispatch.
#[derive(Debug, Clone)]
pub enum NavigationAction {
    /// Navigate to a new URL.
    Navigate(Url),
    /// Go to the previous history entry.
    Back,
    /// Go to the next history entry.
    Forward,
    /// Reload the current entry, bypassing the cache.
    Reload,
}

/// A back/forward history stack over a [`ResourceLoader`].
#[derive(Debug)]
pub struct NavigationController<L: ResourceLoader> {
    loader: L,
    entries: Vec<NavigationEntry>,
    current: Option<usize>,
}

impl<L: ResourceLoader> NavigationController<L> {
    /// Create a controller with an empty history.
    pub fn new(loader: L) -> NavigationController<L> {
        NavigationController {
            loader,
            entries: Vec::new(),
            current: None,
        }
    }

    /// Dispatch a [`NavigationAction`].
    pub fn dispatch(&mut self, action: NavigationAction) -> MochaResult<ResourceResponse> {
        match action {
            NavigationAction::Navigate(url) => self.navigate(url),
            NavigationAction::Back => self.back(),
            NavigationAction::Forward => self.forward(),
            NavigationAction::Reload => self.reload(),
        }
    }

    /// Navigate to `url`, adding a history entry on success.
    ///
    /// A failed load leaves the history untouched. On success any forward entries
    /// are discarded and the entry stores the response's final URL.
    pub fn navigate(&mut self, url: Url) -> MochaResult<ResourceResponse> {
        let response = self.loader.load(LoadRequest::get(url))?;
        self.push_entry(&response.final_url);
        Ok(response)
    }

    /// Navigate to `url`, bypassing the cache (used by `--no-cache`).
    pub fn navigate_no_cache(&mut self, url: Url) -> MochaResult<ResourceResponse> {
        let response = self.loader.load(LoadRequest::get_no_cache(url))?;
        self.push_entry(&response.final_url);
        Ok(response)
    }

    /// Go back one entry and load it. Errors if there is no previous entry; the
    /// current position only moves after a successful load.
    pub fn back(&mut self) -> MochaResult<ResourceResponse> {
        let current = self
            .current
            .ok_or_else(|| MochaError::Navigation("no current entry".to_string()))?;
        if current == 0 {
            return Err(MochaError::Navigation(
                "no previous entry to go back to".to_string(),
            ));
        }
        self.go_to(current - 1)
    }

    /// Go forward one entry and load it. Errors if there is no next entry.
    pub fn forward(&mut self) -> MochaResult<ResourceResponse> {
        let current = self
            .current
            .ok_or_else(|| MochaError::Navigation("no current entry".to_string()))?;
        if current + 1 >= self.entries.len() {
            return Err(MochaError::Navigation(
                "no next entry to go forward to".to_string(),
            ));
        }
        self.go_to(current + 1)
    }

    /// Reload the current entry, bypassing the cache. History is unchanged.
    pub fn reload(&mut self) -> MochaResult<ResourceResponse> {
        let current = self
            .current
            .ok_or_else(|| MochaError::Navigation("nothing to reload".to_string()))?;
        let url = self.entries[current].url.clone();
        self.loader.load(LoadRequest::get_no_cache(url))
    }

    /// The current history entry, if any.
    pub fn current_entry(&self) -> Option<&NavigationEntry> {
        self.current.map(|index| &self.entries[index])
    }

    /// The current URL, if any.
    pub fn current_url(&self) -> Option<&Url> {
        self.current_entry().map(|entry| &entry.url)
    }

    /// All history entries, oldest first.
    pub fn entries(&self) -> &[NavigationEntry] {
        &self.entries
    }

    /// Whether [`back`](Self::back) would succeed.
    pub fn can_go_back(&self) -> bool {
        matches!(self.current, Some(index) if index > 0)
    }

    /// Whether [`forward`](Self::forward) would succeed.
    pub fn can_go_forward(&self) -> bool {
        matches!(self.current, Some(index) if index + 1 < self.entries.len())
    }

    fn push_entry(&mut self, final_url: &Url) {
        let insert_at = match self.current {
            Some(index) => index + 1,
            None => 0,
        };
        self.entries.truncate(insert_at); // discard any forward entries
        self.entries.push(NavigationEntry {
            url: final_url.clone(),
            title: None,
        });
        self.current = Some(self.entries.len() - 1);
    }

    fn go_to(&mut self, target: usize) -> MochaResult<ResourceResponse> {
        let url = self.entries[target].url.clone();
        let response = self.loader.load(LoadRequest::get(url))?;
        self.current = Some(target); // only move after a successful load
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_net::Header;

    /// A loader that returns canned successful responses and records requests.
    struct MockLoader {
        loads: Vec<Url>,
        bypasses: Vec<bool>,
        fail: bool,
        redirect_to: Option<Url>,
    }

    impl MockLoader {
        fn new() -> MockLoader {
            MockLoader {
                loads: Vec::new(),
                bypasses: Vec::new(),
                fail: false,
                redirect_to: None,
            }
        }
    }

    impl ResourceLoader for MockLoader {
        fn load(&mut self, request: LoadRequest) -> MochaResult<ResourceResponse> {
            self.loads.push(request.url.clone());
            self.bypasses.push(request.bypass_cache);
            if self.fail {
                return Err(MochaError::Network("boom".to_string()));
            }
            let final_url = self.redirect_to.clone().unwrap_or(request.url);
            Ok(ResourceResponse {
                final_url,
                status: Some(200),
                headers: Vec::<Header>::new(),
                content_type: Some("text/html".to_string()),
                body: b"<html></html>".to_vec(),
                from_cache: false,
            })
        }
    }

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn navigate_adds_entry() {
        let mut nav = NavigationController::new(MockLoader::new());
        nav.navigate(url("http://a/")).unwrap();
        assert_eq!(nav.entries().len(), 1);
        assert_eq!(nav.current_url(), Some(&url("http://a/")));
    }

    #[test]
    fn back_and_forward_move_through_history() {
        let mut nav = NavigationController::new(MockLoader::new());
        nav.navigate(url("http://a/")).unwrap();
        nav.navigate(url("http://b/")).unwrap();
        assert!(nav.can_go_back());
        nav.back().unwrap();
        assert_eq!(nav.current_url(), Some(&url("http://a/")));
        assert!(nav.can_go_forward());
        nav.forward().unwrap();
        assert_eq!(nav.current_url(), Some(&url("http://b/")));
    }

    #[test]
    fn back_at_start_errors() {
        let mut nav = NavigationController::new(MockLoader::new());
        nav.navigate(url("http://a/")).unwrap();
        assert!(matches!(nav.back().unwrap_err(), MochaError::Navigation(_)));
    }

    #[test]
    fn reload_keeps_entry_and_bypasses_cache() {
        let mut nav = NavigationController::new(MockLoader::new());
        nav.navigate(url("http://a/")).unwrap();
        nav.reload().unwrap();
        assert_eq!(nav.entries().len(), 1);
        // The last load (reload) must have requested a cache bypass.
        assert_eq!(nav.loader.bypasses.last(), Some(&true));
    }

    #[test]
    fn new_navigation_after_back_clears_forward() {
        let mut nav = NavigationController::new(MockLoader::new());
        nav.navigate(url("http://a/")).unwrap();
        nav.navigate(url("http://b/")).unwrap();
        nav.back().unwrap(); // now at a, with b forward
        nav.navigate(url("http://c/")).unwrap();
        assert!(!nav.can_go_forward());
        let urls: Vec<&str> = nav
            .entries()
            .iter()
            .map(|e| e.url.host.as_deref().unwrap())
            .collect();
        assert_eq!(urls, vec!["a", "c"]);
    }

    #[test]
    fn failed_navigation_does_not_corrupt_history() {
        let mut nav = NavigationController::new(MockLoader::new());
        nav.navigate(url("http://a/")).unwrap();
        nav.loader.fail = true;
        assert!(nav.navigate(url("http://b/")).is_err());
        assert_eq!(nav.entries().len(), 1);
        assert_eq!(nav.current_url(), Some(&url("http://a/")));
    }

    #[test]
    fn redirect_final_url_is_stored() {
        let mut loader = MockLoader::new();
        loader.redirect_to = Some(url("http://a/final"));
        let mut nav = NavigationController::new(loader);
        nav.navigate(url("http://a/start")).unwrap();
        assert_eq!(nav.current_url(), Some(&url("http://a/final")));
    }
}
