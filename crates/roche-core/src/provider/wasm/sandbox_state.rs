//! In-memory sandbox state registry for the WASM provider.

use crate::types::{SandboxConfig, SandboxId, SandboxInfo, SandboxStatus};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Metadata for a single WASM sandbox.
pub struct WasmSandbox {
    pub id: SandboxId,
    pub module: wasmtime::Module,
    pub config: SandboxConfig,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

/// Thread-safe in-memory registry of WASM sandboxes.
#[derive(Clone)]
pub struct SandboxRegistry {
    inner: Arc<Mutex<HashMap<SandboxId, WasmSandbox>>>,
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
}

impl SandboxRegistry {
    /// Create a new, empty registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Insert a sandbox into the registry.
    pub fn insert(&self, sandbox: WasmSandbox) {
        let mut map = self.inner.lock().expect("registry lock poisoned");
        map.insert(sandbox.id.clone(), sandbox);
    }

    /// Retrieve a clone of the module and config for the given sandbox ID.
    pub fn get_module_and_config(
        &self,
        id: &SandboxId,
    ) -> Option<(wasmtime::Module, SandboxConfig)> {
        let map = self.inner.lock().expect("registry lock poisoned");
        let sandbox = map.get(id)?;
        Some((sandbox.module.clone(), sandbox.config.clone()))
    }

    /// Remove a sandbox from the registry. Returns `false` if not found.
    pub fn remove(&self, id: &SandboxId) -> bool {
        let mut map = self.inner.lock().expect("registry lock poisoned");
        map.remove(id).is_some()
    }

    /// List all sandboxes as `SandboxInfo`. Status is always `Running`, provider is `"wasm"`.
    pub fn list(&self) -> Vec<SandboxInfo> {
        let map = self.inner.lock().expect("registry lock poisoned");
        map.values()
            .map(|s| SandboxInfo {
                id: s.id.clone(),
                status: SandboxStatus::Running,
                provider: "wasm".to_string(),
                image: s.config.image.clone(),
                expires_at: s.expires_at,
            })
            .collect()
    }

    /// Garbage-collect expired sandboxes. Returns the IDs of removed sandboxes.
    pub fn gc(&self) -> Vec<SandboxId> {
        let now = now_epoch_secs();
        let mut map = self.inner.lock().expect("registry lock poisoned");
        let expired: Vec<SandboxId> = map
            .iter()
            .filter_map(|(id, s)| match s.expires_at {
                Some(exp) if exp <= now => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in &expired {
            map.remove(id);
        }
        expired
    }

    /// Check whether a sandbox with the given ID exists.
    pub fn contains(&self, id: &SandboxId) -> bool {
        let map = self.inner.lock().expect("registry lock poisoned");
        map.contains_key(id)
    }
}

impl Default for SandboxRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a minimal wasmtime engine and a dummy module for testing.
    fn dummy_module() -> wasmtime::Module {
        let engine = wasmtime::Engine::default();
        // Minimal valid WASM module: magic + version + empty.
        wasmtime::Module::new(&engine, "(module)").expect("failed to compile trivial module")
    }

    fn make_sandbox(id: &str, expires_at: Option<u64>) -> WasmSandbox {
        WasmSandbox {
            id: id.to_string(),
            module: dummy_module(),
            config: SandboxConfig {
                provider: "wasm".to_string(),
                image: format!("{id}.wasm"),
                ..Default::default()
            },
            created_at: 1_000_000,
            expires_at,
        }
    }

    // ── Insert and retrieve ──────────────────────────────────────────

    #[test]
    fn test_insert_and_get_module_and_config() {
        let reg = SandboxRegistry::new();
        reg.insert(make_sandbox("sb-1", None));

        let (module, config) = reg
            .get_module_and_config(&"sb-1".to_string())
            .expect("should find sandbox");
        // Module should be usable — verify we can access its engine.
        let _ = module.engine();
        assert_eq!(config.provider, "wasm");
        assert_eq!(config.image, "sb-1.wasm");
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let reg = SandboxRegistry::new();
        assert!(reg
            .get_module_and_config(&"no-such-id".to_string())
            .is_none());
    }

    // ── Remove ───────────────────────────────────────────────────────

    #[test]
    fn test_remove_existing() {
        let reg = SandboxRegistry::new();
        reg.insert(make_sandbox("sb-rm", None));
        assert!(reg.remove(&"sb-rm".to_string()));
        assert!(!reg.contains(&"sb-rm".to_string()));
    }

    #[test]
    fn test_remove_nonexistent_returns_false() {
        let reg = SandboxRegistry::new();
        assert!(!reg.remove(&"ghost".to_string()));
    }

    // ── List ─────────────────────────────────────────────────────────

    #[test]
    fn test_list_empty() {
        let reg = SandboxRegistry::new();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn test_list_populated() {
        let reg = SandboxRegistry::new();
        reg.insert(make_sandbox("a", None));
        reg.insert(make_sandbox("b", Some(9_999_999_999)));

        let mut infos = reg.list();
        infos.sort_by(|x, y| x.id.cmp(&y.id));

        assert_eq!(infos.len(), 2);

        assert_eq!(infos[0].id, "a");
        assert_eq!(infos[0].status, SandboxStatus::Running);
        assert_eq!(infos[0].provider, "wasm");
        assert_eq!(infos[0].image, "a.wasm");
        assert_eq!(infos[0].expires_at, None);

        assert_eq!(infos[1].id, "b");
        assert_eq!(infos[1].expires_at, Some(9_999_999_999));
    }

    // ── GC ───────────────────────────────────────────────────────────

    #[test]
    fn test_gc_removes_expired() {
        let reg = SandboxRegistry::new();

        // Already expired (epoch 0).
        reg.insert(make_sandbox("old-1", Some(0)));
        reg.insert(make_sandbox("old-2", Some(1)));

        // Far in the future — should survive.
        reg.insert(make_sandbox("fresh", Some(9_999_999_999)));

        // No expiry — should survive.
        reg.insert(make_sandbox("forever", None));

        let mut removed = reg.gc();
        removed.sort();

        assert_eq!(removed, vec!["old-1", "old-2"]);
        assert!(!reg.contains(&"old-1".to_string()));
        assert!(!reg.contains(&"old-2".to_string()));
        assert!(reg.contains(&"fresh".to_string()));
        assert!(reg.contains(&"forever".to_string()));
    }

    #[test]
    fn test_gc_empty_registry() {
        let reg = SandboxRegistry::new();
        assert!(reg.gc().is_empty());
    }

    #[test]
    fn test_gc_nothing_expired() {
        let reg = SandboxRegistry::new();
        reg.insert(make_sandbox("alive", Some(9_999_999_999)));
        reg.insert(make_sandbox("eternal", None));
        assert!(reg.gc().is_empty());
        assert_eq!(reg.list().len(), 2);
    }

    // ── Contains ─────────────────────────────────────────────────────

    #[test]
    fn test_contains_present() {
        let reg = SandboxRegistry::new();
        reg.insert(make_sandbox("exists", None));
        assert!(reg.contains(&"exists".to_string()));
    }

    #[test]
    fn test_contains_absent() {
        let reg = SandboxRegistry::new();
        assert!(!reg.contains(&"nope".to_string()));
    }

    // ── Insert overwrites ────────────────────────────────────────────

    #[test]
    fn test_insert_overwrites_existing() {
        let reg = SandboxRegistry::new();
        reg.insert(make_sandbox("dup", Some(100)));
        reg.insert(make_sandbox("dup", Some(200)));

        let infos = reg.list();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].expires_at, Some(200));
    }
}
