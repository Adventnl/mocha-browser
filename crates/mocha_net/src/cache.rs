//! A tiny in-memory response cache.
//!
//! This is intentionally **not** an HTTP cache: there is no `Cache-Control`,
//! validation, or expiration. It simply remembers successful responses by their
//! normalized URL for the lifetime of the process so a repeated GET is cheap.

use std::collections::HashMap;

use crate::ResourceResponse;

/// A process-lifetime map from normalized URL to a stored response.
#[derive(Debug, Default)]
pub struct MemoryCache {
    entries: HashMap<String, ResourceResponse>,
}

impl MemoryCache {
    /// Create an empty cache.
    pub fn new() -> MemoryCache {
        MemoryCache::default()
    }

    /// Look a key up, returning a clone marked `from_cache = true`.
    pub fn get(&self, key: &str) -> Option<ResourceResponse> {
        self.entries.get(key).map(|response| {
            let mut hit = response.clone();
            hit.from_cache = true;
            hit
        })
    }

    /// Store a response under `key` (stored copy always has `from_cache = false`).
    pub fn insert(&mut self, key: String, mut response: ResourceResponse) {
        response.from_cache = false;
        self.entries.insert(key, response);
    }

    /// The number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
