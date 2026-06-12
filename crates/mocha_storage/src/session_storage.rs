//! In-memory, origin-keyed `sessionStorage` (Milestone 15).
//!
//! Unlike [`LocalStorageStore`](crate::LocalStorageStore), `sessionStorage` is
//! **never persisted**: it belongs to a tab/session and is dropped when that
//! session ends. It carries no database. Different origins are isolated; in a
//! real browser each tab has its own `sessionStorage`, so an embedder keeps one
//! `SessionStorage` per tab.

use std::collections::HashMap;

use mocha_origin::Origin;

/// Per-session, origin-keyed key/value storage held entirely in memory.
#[derive(Debug, Default, Clone)]
pub struct SessionStorage {
    entries: HashMap<Origin, HashMap<String, String>>,
}

impl SessionStorage {
    pub fn new() -> SessionStorage {
        SessionStorage::default()
    }

    /// Get an item for `origin`, or `None`.
    pub fn get_item(&self, origin: &Origin, key: &str) -> Option<String> {
        self.entries.get(origin).and_then(|m| m.get(key)).cloned()
    }

    /// Set an item for `origin`.
    pub fn set_item(&mut self, origin: &Origin, key: &str, value: &str) {
        self.entries
            .entry(origin.clone())
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    /// Remove a single item for `origin`.
    pub fn remove_item(&mut self, origin: &Origin, key: &str) {
        if let Some(map) = self.entries.get_mut(origin) {
            map.remove(key);
        }
    }

    /// Remove all items for `origin`.
    pub fn clear_origin(&mut self, origin: &Origin) {
        self.entries.remove(origin);
    }

    /// Remove everything (e.g. when the tab/session closes).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_url::Url;

    fn origin(s: &str) -> Origin {
        Origin::from_url(&Url::parse(s).unwrap()).unwrap()
    }

    #[test]
    fn set_get_remove() {
        let mut s = SessionStorage::new();
        let o = origin("http://a.com/");
        s.set_item(&o, "k", "v");
        assert_eq!(s.get_item(&o, "k").as_deref(), Some("v"));
        s.remove_item(&o, "k");
        assert_eq!(s.get_item(&o, "k"), None);
    }

    #[test]
    fn origins_isolated() {
        let mut s = SessionStorage::new();
        let a = origin("http://a.com/");
        let b = origin("http://b.com/");
        s.set_item(&a, "k", "1");
        assert_eq!(s.get_item(&b, "k"), None);
    }

    #[test]
    fn separate_instances_do_not_share() {
        // Two tabs => two SessionStorage instances => no sharing.
        let o = origin("http://a.com/");
        let mut tab1 = SessionStorage::new();
        let tab2 = SessionStorage::new();
        tab1.set_item(&o, "k", "v");
        assert_eq!(tab2.get_item(&o, "k"), None);
    }

    #[test]
    fn clear_empties_session() {
        let mut s = SessionStorage::new();
        let o = origin("http://a.com/");
        s.set_item(&o, "k", "v");
        s.clear();
        assert_eq!(s.get_item(&o, "k"), None);
    }
}
