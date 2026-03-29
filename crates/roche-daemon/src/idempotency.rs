// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::proto;

/// Default TTL for cached idempotency results (10 minutes).
const DEFAULT_TTL: Duration = Duration::from_secs(600);

/// Maximum number of cached entries before forced eviction.
const MAX_ENTRIES: usize = 10_000;

struct CacheEntry {
    response: proto::ExecResponse,
    inserted_at: Instant,
}

/// In-memory TTL cache for idempotent exec results.
///
/// When an agent retries an exec with the same idempotency key,
/// the cached result is returned without re-executing.
pub struct IdempotencyCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
    ttl: Duration,
}

impl IdempotencyCache {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: DEFAULT_TTL,
        }
    }

    /// Look up a cached response by idempotency key.
    /// Returns `None` if not found or expired.
    pub fn get(&self, key: &str) -> Option<proto::ExecResponse> {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Some(entry.response.clone());
            }
            // Expired — remove it
            entries.remove(key);
        }
        None
    }

    /// Store a response for the given idempotency key.
    pub fn put(&self, key: String, response: proto::ExecResponse) {
        let mut entries = self.entries.lock().unwrap();
        // Evict expired entries if we're at capacity
        if entries.len() >= MAX_ENTRIES {
            let now = Instant::now();
            let ttl = self.ttl;
            entries.retain(|_, e| now.duration_since(e.inserted_at) < ttl);
        }
        entries.insert(
            key,
            CacheEntry {
                response,
                inserted_at: Instant::now(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(stdout: &str) -> proto::ExecResponse {
        proto::ExecResponse {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
            trace: None,
        }
    }

    #[test]
    fn test_cache_hit() {
        let cache = IdempotencyCache::new();
        cache.put("key1".into(), make_response("hello"));
        let cached = cache.get("key1");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().stdout, "hello");
    }

    #[test]
    fn test_cache_miss() {
        let cache = IdempotencyCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_expiry() {
        let cache = IdempotencyCache {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_millis(1),
        };
        cache.put("key1".into(), make_response("hello"));
        std::thread::sleep(Duration::from_millis(5));
        assert!(cache.get("key1").is_none());
    }

    #[test]
    fn test_cache_overwrite() {
        let cache = IdempotencyCache::new();
        cache.put("key1".into(), make_response("first"));
        cache.put("key1".into(), make_response("second"));
        let cached = cache.get("key1").unwrap();
        assert_eq!(cached.stdout, "second");
    }

    #[test]
    fn test_eviction_at_capacity() {
        let cache = IdempotencyCache {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_millis(1),
        };
        // Fill to MAX_ENTRIES with expired entries
        {
            let mut entries = cache.entries.lock().unwrap();
            for i in 0..MAX_ENTRIES {
                entries.insert(
                    format!("old_{i}"),
                    CacheEntry {
                        response: make_response("old"),
                        inserted_at: Instant::now() - Duration::from_secs(3600),
                    },
                );
            }
        }
        // This should trigger eviction
        cache.put("new_key".into(), make_response("new"));
        let entries = cache.entries.lock().unwrap();
        // All expired entries should be gone, only the new one remains
        assert_eq!(entries.len(), 1);
        assert!(entries.contains_key("new_key"));
    }
}
