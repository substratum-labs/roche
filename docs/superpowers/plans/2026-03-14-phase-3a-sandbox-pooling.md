# Phase 3A: Sandbox Pooling Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add sandbox pooling to roche-daemon so that `create` requests can return pre-warmed sandboxes in ~10ms instead of ~1s.

**Architecture:** PoolManager sits between gRPC handlers and providers. It maintains per-(provider, image) pools of idle sandboxes, with background replenish and reaper tasks. Sandboxes are use-and-discard (never returned to pool). WASM is excluded. Pool config comes from `~/.roche/pool.toml` or `--pool` CLI args.

**Tech Stack:** Rust, tokio (Notify, Mutex, interval), serde/toml for config, tonic/prost for new RPCs

---

## File Structure

```
crates/roche-daemon/src/
├── main.rs              # MODIFY: add --pool arg, parse pool config, init PoolManager, spawn tasks, shutdown drain
├── server.rs            # MODIFY: add pool_manager field, integrate into create/destroy, add pool RPC handlers
├── gc.rs                # UNCHANGED
└── pool/
    ├── mod.rs           # CREATE: PoolManager, PoolManagerInner, SandboxPool, PoolKey, IdleSandbox, try_acquire, on_destroy, shutdown
    ├── config.rs        # CREATE: PoolConfig, PoolFileConfig, parse_pool_toml, parse_pool_arg
    ├── replenish.rs     # CREATE: run_replenish_loop (Notify-based background task)
    └── reaper.rs        # CREATE: run_reaper_loop (60s interval background task)

crates/roche-daemon/proto/roche/v1/sandbox.proto  # MODIFY: add PoolStatus/PoolWarmup/PoolDrain RPCs + messages
crates/roche-daemon/Cargo.toml                     # MODIFY: add toml dependency
crates/roche-core/src/provider/docker.rs           # MODIFY: skip roche.expires label when timeout_secs=0
crates/roche-cli/src/main.rs                       # MODIFY: add Pool subcommand with status/warmup/drain
```

---

## Chunk 1: Foundation — Config, Data Structures, Docker Fix

### Task 1: Fix Docker provider to skip expiry label when timeout_secs=0

Pool sandboxes are created with `timeout_secs=0` so they don't get GC'd. Currently docker.rs always writes `roche.expires` label.

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs:76-82`

- [ ] **Step 1: Write test for timeout_secs=0 skipping expiry label**

In `crates/roche-core/src/provider/docker.rs`, add to existing tests (or create test module):

```rust
#[test]
fn test_build_create_args_no_expiry_when_timeout_zero() {
    let config = SandboxConfig {
        provider: "docker".to_string(),
        image: "python:3.12-slim".to_string(),
        memory: None,
        cpus: None,
        timeout_secs: 0,
        network: false,
        writable: false,
        env: std::collections::HashMap::new(),
        mounts: vec![],
        kernel: None,
        rootfs: None,
    };
    let args = build_create_args(&config);
    // Should have roche.managed but NOT roche.expires
    assert!(args.iter().any(|a| a == "roche.managed=true"));
    assert!(!args.iter().any(|a| a.starts_with("roche.expires=")));
}

#[test]
fn test_build_create_args_has_expiry_when_timeout_nonzero() {
    let config = SandboxConfig {
        provider: "docker".to_string(),
        image: "python:3.12-slim".to_string(),
        memory: None,
        cpus: None,
        timeout_secs: 300,
        network: false,
        writable: false,
        env: std::collections::HashMap::new(),
        mounts: vec![],
        kernel: None,
        rootfs: None,
    };
    let args = build_create_args(&config);
    assert!(args.iter().any(|a| a.starts_with("roche.expires=")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p roche-core test_build_create_args_no_expiry`
Expected: FAIL — roche.expires label is present even with timeout_secs=0

- [ ] **Step 3: Make expiry label conditional on timeout_secs > 0**

In `crates/roche-core/src/provider/docker.rs`, replace lines 76-82:

```rust
    // Expiry timestamp (only if timeout > 0; pool sandboxes use timeout=0 for no expiry)
    if config.timeout_secs > 0 {
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + config.timeout_secs;
        args.extend(["--label".into(), format!("roche.expires={expires}")]);
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p roche-core test_build_create_args`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "fix: skip roche.expires label when timeout_secs=0 (for pool sandboxes)"
```

---

### Task 2: Add toml dependency to roche-daemon

**Files:**
- Modify: `crates/roche-daemon/Cargo.toml`

- [ ] **Step 1: Add toml to dependencies**

Add after the `tracing-subscriber` line in `[dependencies]`:

```toml
toml = "0.8"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p roche-daemon`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/roche-daemon/Cargo.toml
git commit -m "chore: add toml dependency to roche-daemon for pool config"
```

---

### Task 3: Create pool config module

**Files:**
- Create: `crates/roche-daemon/src/pool/config.rs`

- [ ] **Step 1: Write tests for pool config parsing**

Create `crates/roche-daemon/src/pool/config.rs`:

```rust
use serde::Deserialize;

/// Configuration for a single sandbox pool.
#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    pub provider: String,
    pub image: String,
    #[serde(default)]
    pub min_idle: usize,
    #[serde(default = "default_max_idle")]
    pub max_idle: usize,
    #[serde(default = "default_max_total")]
    pub max_total: usize,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
}

fn default_max_idle() -> usize { 5 }
fn default_max_total() -> usize { 20 }
fn default_idle_timeout_secs() -> u64 { 600 }

/// Top-level structure for pool.toml.
#[derive(Debug, Deserialize)]
pub struct PoolFileConfig {
    #[serde(default)]
    pub pool: Vec<PoolConfig>,
}

/// Load pool configs from ~/.roche/pool.toml (if exists).
pub fn load_pool_toml() -> Vec<PoolConfig> {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".roche").join("pool.toml"),
        None => return vec![],
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    match toml::from_str::<PoolFileConfig>(&content) {
        Ok(cfg) => cfg.pool,
        Err(e) => {
            tracing::warn!("failed to parse pool.toml: {e}");
            vec![]
        }
    }
}

/// Parse a CLI --pool arg: "provider/image?key=value&key=value"
/// Example: "docker/python:3.12-slim?min=3&max=10&total=20&idle_timeout=600"
pub fn parse_pool_arg(arg: &str) -> Result<PoolConfig, String> {
    let (prefix, query) = match arg.split_once('?') {
        Some((p, q)) => (p, q),
        None => (arg, ""),
    };

    let (provider, image) = prefix
        .split_once('/')
        .ok_or_else(|| format!("invalid pool arg: expected provider/image, got '{prefix}'"))?;

    let mut config = PoolConfig {
        provider: provider.to_string(),
        image: image.to_string(),
        min_idle: 0,
        max_idle: default_max_idle(),
        max_total: default_max_total(),
        idle_timeout_secs: default_idle_timeout_secs(),
    };

    if !query.is_empty() {
        for pair in query.split('&') {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| format!("invalid pool param: '{pair}'"))?;
            match k {
                "min" => config.min_idle = v.parse().map_err(|_| format!("invalid min: {v}"))?,
                "max" => config.max_idle = v.parse().map_err(|_| format!("invalid max: {v}"))?,
                "total" => config.max_total = v.parse().map_err(|_| format!("invalid total: {v}"))?,
                "idle_timeout" => config.idle_timeout_secs = v.parse().map_err(|_| format!("invalid idle_timeout: {v}"))?,
                other => return Err(format!("unknown pool param: '{other}'")),
            }
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pool_arg_full() {
        let cfg = parse_pool_arg("docker/python:3.12-slim?min=3&max=10&total=20&idle_timeout=300").unwrap();
        assert_eq!(cfg.provider, "docker");
        assert_eq!(cfg.image, "python:3.12-slim");
        assert_eq!(cfg.min_idle, 3);
        assert_eq!(cfg.max_idle, 10);
        assert_eq!(cfg.max_total, 20);
        assert_eq!(cfg.idle_timeout_secs, 300);
    }

    #[test]
    fn test_parse_pool_arg_minimal() {
        let cfg = parse_pool_arg("docker/node:20-slim").unwrap();
        assert_eq!(cfg.provider, "docker");
        assert_eq!(cfg.image, "node:20-slim");
        assert_eq!(cfg.min_idle, 0);
        assert_eq!(cfg.max_idle, 5);
        assert_eq!(cfg.max_total, 20);
        assert_eq!(cfg.idle_timeout_secs, 600);
    }

    #[test]
    fn test_parse_pool_arg_partial_params() {
        let cfg = parse_pool_arg("docker/python:3.12-slim?min=2").unwrap();
        assert_eq!(cfg.min_idle, 2);
        assert_eq!(cfg.max_idle, 5); // default
    }

    #[test]
    fn test_parse_pool_arg_no_slash() {
        assert!(parse_pool_arg("docker-python").is_err());
    }

    #[test]
    fn test_parse_pool_arg_unknown_param() {
        assert!(parse_pool_arg("docker/img?foo=1").is_err());
    }

    #[test]
    fn test_toml_deserialization() {
        let toml_str = r#"
[[pool]]
provider = "docker"
image = "python:3.12-slim"
min_idle = 3
max_idle = 10

[[pool]]
provider = "docker"
image = "node:20-slim"
"#;
        let cfg: PoolFileConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.pool.len(), 2);
        assert_eq!(cfg.pool[0].min_idle, 3);
        assert_eq!(cfg.pool[0].max_idle, 10);
        assert_eq!(cfg.pool[0].max_total, 20); // default
        assert_eq!(cfg.pool[1].min_idle, 0);   // default
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p roche-daemon pool::config`
Expected: PASS (after creating mod.rs in next task)

---

### Task 4: Create pool module skeleton (mod.rs)

**Files:**
- Create: `crates/roche-daemon/src/pool/mod.rs`

- [ ] **Step 1: Create pool/mod.rs with data structures and core logic**

```rust
pub mod config;
pub mod reaper;
pub mod replenish;

use config::PoolConfig;
use roche_core::provider::docker::DockerProvider;
use roche_core::provider::SandboxProvider;
use roche_core::types::{SandboxConfig, SandboxId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PoolKey {
    pub provider: String,
    pub image: String,
}

impl std::fmt::Display for PoolKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.provider, self.image)
    }
}

pub struct IdleSandbox {
    pub id: SandboxId,
    pub created_at: Instant,
}

pub struct SandboxPool {
    pub config: PoolConfig,
    pub idle: VecDeque<IdleSandbox>,
    pub active: HashSet<SandboxId>,
}

pub struct PoolManagerInner {
    pub pools: HashMap<PoolKey, SandboxPool>,
    pub pending_replenish: HashSet<PoolKey>,
    pub docker: DockerProvider,
    #[cfg(target_os = "linux")]
    pub firecracker: Option<roche_core::provider::firecracker::FirecrackerProvider>,
}

pub struct PoolManager {
    pub inner: Arc<Mutex<PoolManagerInner>>,
    pub replenish_notify: Arc<Notify>,
}

impl PoolManager {
    pub fn new(configs: Vec<PoolConfig>) -> Self {
        let mut pools = HashMap::new();

        for cfg in &configs {
            // Skip WASM — pooling unnecessary
            if cfg.provider == "wasm" {
                tracing::warn!("WASM pooling is unnecessary, skipping pool config for {}", cfg.image);
                continue;
            }

            // Platform validation
            #[cfg(not(target_os = "linux"))]
            if cfg.provider == "firecracker" {
                tracing::warn!("Firecracker pool config skipped: requires Linux with KVM");
                continue;
            }

            if cfg.provider != "docker" && cfg.provider != "firecracker" {
                tracing::error!("unknown provider '{}', skipping pool config", cfg.provider);
                continue;
            }

            let key = PoolKey {
                provider: cfg.provider.clone(),
                image: cfg.image.clone(),
            };

            tracing::info!(
                "pool configured: {} (min={}, max={}, total={})",
                key, cfg.min_idle, cfg.max_idle, cfg.max_total
            );

            pools.insert(
                key,
                SandboxPool {
                    config: cfg.clone(),
                    idle: VecDeque::new(),
                    active: HashSet::new(),
                },
            );
        }

        let replenish_notify = Arc::new(Notify::new());
        let inner = Arc::new(Mutex::new(PoolManagerInner {
            pools,
            pending_replenish: HashSet::new(),
            docker: DockerProvider::new(),
            #[cfg(target_os = "linux")]
            firecracker: roche_core::provider::firecracker::FirecrackerProvider::new().ok(),
        }));

        PoolManager {
            inner,
            replenish_notify,
        }
    }

    /// Try to acquire a sandbox from the pool.
    /// Returns Some(id) on pool hit, None on bypass or miss.
    /// On miss, creates sandbox via provider and tracks it in active set.
    pub async fn try_acquire(&self, config: &SandboxConfig) -> Option<SandboxId> {
        // WASM bypass
        let provider = if config.provider.is_empty() { "docker" } else { &config.provider };
        if provider == "wasm" {
            return None;
        }

        // Non-default config bypass
        if config.network
            || config.writable
            || !config.env.is_empty()
            || !config.mounts.is_empty()
            || config.memory.is_some()
            || config.cpus.is_some()
        {
            tracing::debug!("pool bypass: non-default config (network={}, writable={}, env={}, mounts={})",
                config.network, config.writable, config.env.len(), config.mounts.len());
            return None;
        }

        let key = PoolKey {
            provider: provider.to_string(),
            image: config.image.clone(),
        };

        let mut inner = self.inner.lock().await;
        let pool = match inner.pools.get_mut(&key) {
            Some(p) => p,
            None => return None, // No pool configured for this key
        };

        // Capacity bypass
        if pool.idle.len() + pool.active.len() >= pool.config.max_total {
            tracing::debug!("pool bypass: at capacity for {key}");
            return None;
        }

        // Pool hit
        if let Some(idle_sb) = pool.idle.pop_front() {
            pool.active.insert(idle_sb.id.clone());
            let remaining = pool.idle.len();
            tracing::debug!("pool hit: {key} ({remaining} idle remaining)");

            // Trigger replenish
            inner.pending_replenish.insert(key);
            drop(inner);
            self.replenish_notify.notify_one();

            return Some(idle_sb.id);
        }

        // Pool miss — create via provider and track in active
        tracing::debug!("pool miss: {key} (0 idle)");

        // Build pool sandbox config (AI-safe defaults, no timeout)
        let pool_config = SandboxConfig {
            provider: provider.to_string(),
            image: config.image.clone(),
            memory: None,
            cpus: None,
            timeout_secs: config.timeout_secs,
            network: false,
            writable: false,
            env: HashMap::new(),
            mounts: vec![],
            kernel: None,
            rootfs: None,
        };

        // Create via provider (need to release lock first)
        let create_result = match provider {
            "docker" => inner.docker.create(&pool_config).await,
            #[cfg(target_os = "linux")]
            "firecracker" => {
                if let Some(ref fc) = inner.firecracker {
                    fc.create(&pool_config).await
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        match create_result {
            Ok(id) => {
                let pool = inner.pools.get_mut(&key).unwrap();
                pool.active.insert(id.clone());

                // Trigger replenish
                inner.pending_replenish.insert(key);
                drop(inner);
                self.replenish_notify.notify_one();

                Some(id)
            }
            Err(e) => {
                tracing::warn!("pool miss create failed for {key}: {e}");
                None
            }
        }
    }

    /// Notify pool that a sandbox was destroyed. Removes from active set if present.
    pub async fn on_destroy(&self, sandbox_id: &SandboxId) {
        let mut inner = self.inner.lock().await;
        for (key, pool) in inner.pools.iter_mut() {
            if pool.active.remove(sandbox_id) {
                tracing::debug!("pool: removed {sandbox_id} from active set of {key}");
                inner.pending_replenish.insert(key.clone());
                drop(inner);
                self.replenish_notify.notify_one();
                return;
            }
        }
        // sandbox_id not in any pool — no-op (bypass or non-pool sandbox)
    }

    /// Drain all idle sandboxes (for shutdown).
    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        let mut to_destroy: Vec<(String, SandboxId)> = Vec::new();

        for (key, pool) in inner.pools.iter_mut() {
            while let Some(idle_sb) = pool.idle.pop_front() {
                to_destroy.push((key.provider.clone(), idle_sb.id));
            }
        }

        let count = to_destroy.len();
        if count == 0 {
            return;
        }

        // Destroy via providers
        for (provider, id) in &to_destroy {
            let result = match provider.as_str() {
                "docker" => inner.docker.destroy(id).await,
                #[cfg(target_os = "linux")]
                "firecracker" => {
                    if let Some(ref fc) = inner.firecracker {
                        fc.destroy(id).await
                    } else {
                        Ok(())
                    }
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                tracing::warn!("pool shutdown: failed to destroy {id}: {e}");
            }
        }

        tracing::info!("draining pool: destroyed {count} idle sandboxes");
    }

    /// Get pool status for all pools.
    pub async fn status(&self) -> Vec<PoolStatus> {
        let inner = self.inner.lock().await;
        inner
            .pools
            .iter()
            .map(|(key, pool)| PoolStatus {
                provider: key.provider.clone(),
                image: key.image.clone(),
                idle_count: pool.idle.len() as u32,
                active_count: pool.active.len() as u32,
                max_idle: pool.config.max_idle as u32,
                max_total: pool.config.max_total as u32,
            })
            .collect()
    }

    /// Trigger warmup for all pools.
    pub async fn warmup(&self) {
        let mut inner = self.inner.lock().await;
        let keys: Vec<PoolKey> = inner.pools.keys().cloned().collect();
        for key in keys {
            inner.pending_replenish.insert(key);
        }
        drop(inner);
        self.replenish_notify.notify_one();
    }

    /// Drain all idle sandboxes and return count destroyed.
    pub async fn drain(&self) -> u32 {
        let mut inner = self.inner.lock().await;
        let mut to_destroy: Vec<(String, SandboxId)> = Vec::new();

        for (key, pool) in inner.pools.iter_mut() {
            while let Some(idle_sb) = pool.idle.pop_front() {
                to_destroy.push((key.provider.clone(), idle_sb.id));
            }
        }

        let count = to_destroy.len() as u32;

        for (provider, id) in &to_destroy {
            let result = match provider.as_str() {
                "docker" => inner.docker.destroy(id).await,
                #[cfg(target_os = "linux")]
                "firecracker" => {
                    if let Some(ref fc) = inner.firecracker {
                        fc.destroy(id).await
                    } else {
                        Ok(())
                    }
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                tracing::warn!("pool drain: failed to destroy {id}: {e}");
            }
        }

        count
    }

    /// Initial warmup at startup — add all pools with min_idle > 0 to pending_replenish.
    pub async fn initial_warmup(&self) {
        let mut inner = self.inner.lock().await;
        let keys: Vec<PoolKey> = inner
            .pools
            .iter()
            .filter(|(_, pool)| pool.config.min_idle > 0)
            .map(|(key, _)| key.clone())
            .collect();

        if keys.is_empty() {
            return;
        }

        for key in keys {
            inner.pending_replenish.insert(key);
        }
        drop(inner);
        self.replenish_notify.notify_one();
    }
}

pub struct PoolStatus {
    pub provider: String,
    pub image: String,
    pub idle_count: u32,
    pub active_count: u32,
    pub max_idle: u32,
    pub max_total: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(provider: &str, image: &str, min: usize, max: usize, total: usize) -> PoolConfig {
        PoolConfig {
            provider: provider.to_string(),
            image: image.to_string(),
            min_idle: min,
            max_idle: max,
            max_total: total,
            idle_timeout_secs: 600,
        }
    }

    #[test]
    fn test_pool_manager_new_skips_wasm() {
        let configs = vec![make_config("wasm", "test.wasm", 1, 5, 10)];
        let pm = PoolManager::new(configs);
        let inner = pm.inner.blocking_lock();
        assert!(inner.pools.is_empty());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_pool_manager_new_skips_firecracker_on_non_linux() {
        let configs = vec![make_config("firecracker", "/path/rootfs", 1, 5, 10)];
        let pm = PoolManager::new(configs);
        let inner = pm.inner.blocking_lock();
        assert!(inner.pools.is_empty());
    }

    #[test]
    fn test_pool_manager_new_skips_unknown_provider() {
        let configs = vec![make_config("kubernetes", "img", 1, 5, 10)];
        let pm = PoolManager::new(configs);
        let inner = pm.inner.blocking_lock();
        assert!(inner.pools.is_empty());
    }

    #[test]
    fn test_pool_manager_new_creates_docker_pool() {
        let configs = vec![make_config("docker", "python:3.12-slim", 2, 5, 10)];
        let pm = PoolManager::new(configs);
        let inner = pm.inner.blocking_lock();
        assert_eq!(inner.pools.len(), 1);
        let key = PoolKey {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
        };
        assert!(inner.pools.contains_key(&key));
    }

    #[tokio::test]
    async fn test_try_acquire_bypass_wasm() {
        let pm = PoolManager::new(vec![]);
        let config = SandboxConfig {
            provider: "wasm".to_string(),
            image: "test.wasm".to_string(),
            memory: None,
            cpus: None,
            timeout_secs: 300,
            network: false,
            writable: false,
            env: HashMap::new(),
            mounts: vec![],
            kernel: None,
            rootfs: None,
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_bypass_non_default_config() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 10)];
        let pm = PoolManager::new(configs);
        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            memory: None,
            cpus: None,
            timeout_secs: 300,
            network: true, // non-default
            writable: false,
            env: HashMap::new(),
            mounts: vec![],
            kernel: None,
            rootfs: None,
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_no_pool_configured() {
        let pm = PoolManager::new(vec![]);
        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            memory: None,
            cpus: None,
            timeout_secs: 300,
            network: false,
            writable: false,
            env: HashMap::new(),
            mounts: vec![],
            kernel: None,
            rootfs: None,
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_pool_hit() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 10)];
        let pm = PoolManager::new(configs);

        // Manually inject an idle sandbox
        {
            let mut inner = pm.inner.lock().await;
            let key = PoolKey {
                provider: "docker".to_string(),
                image: "python:3.12-slim".to_string(),
            };
            let pool = inner.pools.get_mut(&key).unwrap();
            pool.idle.push_back(IdleSandbox {
                id: "test-sandbox-id".to_string(),
                created_at: Instant::now(),
            });
        }

        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            memory: None,
            cpus: None,
            timeout_secs: 300,
            network: false,
            writable: false,
            env: HashMap::new(),
            mounts: vec![],
            kernel: None,
            rootfs: None,
        };
        let result = pm.try_acquire(&config).await;
        assert_eq!(result, Some("test-sandbox-id".to_string()));

        // Verify it moved to active
        let inner = pm.inner.lock().await;
        let key = PoolKey {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
        };
        let pool = &inner.pools[&key];
        assert!(pool.idle.is_empty());
        assert!(pool.active.contains("test-sandbox-id"));
    }

    #[tokio::test]
    async fn test_try_acquire_bypass_at_capacity() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 2)];
        let pm = PoolManager::new(configs);

        // Fill to capacity with active sandboxes
        {
            let mut inner = pm.inner.lock().await;
            let key = PoolKey {
                provider: "docker".to_string(),
                image: "python:3.12-slim".to_string(),
            };
            let pool = inner.pools.get_mut(&key).unwrap();
            pool.active.insert("sb-1".to_string());
            pool.active.insert("sb-2".to_string());
        }

        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            memory: None,
            cpus: None,
            timeout_secs: 300,
            network: false,
            writable: false,
            env: HashMap::new(),
            mounts: vec![],
            kernel: None,
            rootfs: None,
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_on_destroy_removes_from_active() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 10)];
        let pm = PoolManager::new(configs);

        {
            let mut inner = pm.inner.lock().await;
            let key = PoolKey {
                provider: "docker".to_string(),
                image: "python:3.12-slim".to_string(),
            };
            let pool = inner.pools.get_mut(&key).unwrap();
            pool.active.insert("sb-1".to_string());
        }

        pm.on_destroy(&"sb-1".to_string()).await;

        let inner = pm.inner.lock().await;
        let key = PoolKey {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
        };
        assert!(!inner.pools[&key].active.contains("sb-1"));
    }

    #[tokio::test]
    async fn test_on_destroy_unknown_id_is_noop() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 10)];
        let pm = PoolManager::new(configs);
        // Should not panic
        pm.on_destroy(&"nonexistent".to_string()).await;
    }
}
```

- [ ] **Step 2: Register pool module in main.rs**

Add after `mod server;` line in `crates/roche-daemon/src/main.rs`:

```rust
mod pool;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p roche-daemon pool::`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/roche-daemon/src/pool/
git add crates/roche-daemon/src/main.rs
git commit -m "feat: add pool module with PoolManager, config parsing, and core pool logic"
```

---

## Chunk 2: Background Tasks (Replenish + Reaper)

### Task 5: Create replenish background task

**Files:**
- Create: `crates/roche-daemon/src/pool/replenish.rs`

- [ ] **Step 1: Implement replenish loop**

```rust
use super::{IdleSandbox, PoolKey, PoolManagerInner};
use roche_core::provider::SandboxProvider;
use roche_core::types::SandboxConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;

/// Background task that replenishes pools when idle count drops below min_idle.
pub async fn run_replenish_loop(
    inner: Arc<Mutex<PoolManagerInner>>,
    notify: Arc<Notify>,
) {
    loop {
        notify.notified().await;

        // Drain pending keys
        let keys: Vec<PoolKey> = {
            let mut state = inner.lock().await;
            state.pending_replenish.drain().collect()
        };

        for key in keys {
            // Calculate deficit
            let (deficit, image) = {
                let state = inner.lock().await;
                let pool = match state.pools.get(&key) {
                    Some(p) => p,
                    None => continue,
                };
                let current_idle = pool.idle.len();
                let current_total = current_idle + pool.active.len();
                let min_idle = pool.config.min_idle;
                let max_total = pool.config.max_total;

                if current_idle >= min_idle {
                    continue;
                }

                let deficit = (min_idle - current_idle).min(max_total.saturating_sub(current_total));
                (deficit, pool.config.image.clone())
            };

            if deficit == 0 {
                continue;
            }

            tracing::info!("replenishing pool {key}: creating {deficit} sandboxes");

            // Create sandboxes with limited concurrency (max 3 concurrent)
            let semaphore = Arc::new(tokio::sync::Semaphore::new(3));
            let mut handles = Vec::new();

            for _ in 0..deficit {
                let sem = semaphore.clone();
                let inner_clone = inner.clone();
                let key_clone = key.clone();
                let image_clone = image.clone();

                let handle = tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();

                    let config = SandboxConfig {
                        provider: key_clone.provider.clone(),
                        image: image_clone,
                        memory: None,
                        cpus: None,
                        timeout_secs: 0, // No expiry for pool sandboxes
                        network: false,
                        writable: false,
                        env: HashMap::new(),
                        mounts: vec![],
                        kernel: None,
                        rootfs: None,
                    };

                    let state = inner_clone.lock().await;
                    let result = match key_clone.provider.as_str() {
                        "docker" => state.docker.create(&config).await,
                        #[cfg(target_os = "linux")]
                        "firecracker" => {
                            if let Some(ref fc) = state.firecracker {
                                fc.create(&config).await
                            } else {
                                return;
                            }
                        }
                        _ => return,
                    };
                    drop(state);

                    match result {
                        Ok(id) => {
                            let mut state = inner_clone.lock().await;
                            if let Some(pool) = state.pools.get_mut(&key_clone) {
                                pool.idle.push_back(IdleSandbox {
                                    id,
                                    created_at: Instant::now(),
                                });
                            }
                        }
                        Err(e) => {
                            tracing::warn!("pool replenish failed: {key_clone}: {e}");
                        }
                    }
                });

                handles.push(handle);
            }

            for handle in handles {
                let _ = handle.await;
            }

            // Log final state
            let state = inner.lock().await;
            if let Some(pool) = state.pools.get(&key) {
                tracing::info!(
                    "pool {key}: {} idle, {} active",
                    pool.idle.len(),
                    pool.active.len()
                );
            }
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p roche-daemon`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/roche-daemon/src/pool/replenish.rs
git commit -m "feat: add pool replenish background task"
```

---

### Task 6: Create reaper background task

**Files:**
- Create: `crates/roche-daemon/src/pool/reaper.rs`

- [ ] **Step 1: Implement reaper loop**

```rust
use super::{PoolKey, PoolManagerInner};
use roche_core::provider::SandboxProvider;
use roche_core::types::SandboxId;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Background task that destroys expired and excess idle sandboxes every 60 seconds.
pub async fn run_reaper_loop(inner: Arc<Mutex<PoolManagerInner>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        // Collect sandboxes to destroy
        let to_destroy: Vec<(String, SandboxId)> = {
            let mut state = inner.lock().await;
            let mut removals = Vec::new();

            for (key, pool) in state.pools.iter_mut() {
                let timeout = Duration::from_secs(pool.config.idle_timeout_secs);
                let max_idle = pool.config.max_idle;

                // Remove expired idle sandboxes
                let mut i = 0;
                while i < pool.idle.len() {
                    if pool.idle[i].created_at.elapsed() > timeout {
                        let sb = pool.idle.remove(i).unwrap();
                        removals.push((key.provider.clone(), sb.id));
                    } else {
                        i += 1;
                    }
                }

                // Remove excess beyond max_idle (oldest first — front of deque)
                while pool.idle.len() > max_idle {
                    let sb = pool.idle.pop_front().unwrap();
                    removals.push((key.provider.clone(), sb.id));
                }
            }

            removals
        };

        if to_destroy.is_empty() {
            continue;
        }

        tracing::info!("pool reaper: destroying {} idle sandboxes", to_destroy.len());

        let state = inner.lock().await;
        for (provider, id) in &to_destroy {
            let result = match provider.as_str() {
                "docker" => state.docker.destroy(id).await,
                #[cfg(target_os = "linux")]
                "firecracker" => {
                    if let Some(ref fc) = state.firecracker {
                        fc.destroy(id).await
                    } else {
                        Ok(())
                    }
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                tracing::warn!("pool reaper: failed to destroy {id}: {e}");
            }
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p roche-daemon`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/roche-daemon/src/pool/reaper.rs
git commit -m "feat: add pool reaper background task"
```

---

## Chunk 3: Proto Changes and Server Integration

### Task 7: Add pool RPCs to proto

**Files:**
- Modify: `crates/roche-daemon/proto/roche/v1/sandbox.proto`

- [ ] **Step 1: Add pool RPC definitions and messages**

Add the following RPCs to the `SandboxService` service (after the `CopyFrom` line):

```protobuf
  rpc PoolStatus(PoolStatusRequest) returns (PoolStatusResponse);
  rpc PoolWarmup(PoolWarmupRequest) returns (PoolWarmupResponse);
  rpc PoolDrain(PoolDrainRequest) returns (PoolDrainResponse);
```

Add the following messages at the end of the file (after `SandboxInfo`):

```protobuf
// Pool management RPCs

message PoolStatusRequest {}
message PoolStatusResponse {
  repeated PoolInfo pools = 1;
}

message PoolInfo {
  string provider = 1;
  string image = 2;
  uint32 idle_count = 3;
  uint32 active_count = 4;
  uint32 max_idle = 5;
  uint32 max_total = 6;
}

message PoolWarmupRequest {}
message PoolWarmupResponse {}

message PoolDrainRequest {}
message PoolDrainResponse {
  uint32 destroyed_count = 1;
}
```

- [ ] **Step 2: Verify proto compiles**

Run: `cargo build -p roche-daemon`
Expected: success (tonic-build generates new code)

- [ ] **Step 3: Commit**

```bash
git add crates/roche-daemon/proto/roche/v1/sandbox.proto
git commit -m "feat: add PoolStatus/PoolWarmup/PoolDrain RPCs to proto"
```

---

### Task 8: Integrate PoolManager into server.rs

**Files:**
- Modify: `crates/roche-daemon/src/server.rs`

- [ ] **Step 1: Add pool_manager field to SandboxServiceImpl**

Add `use std::sync::Arc;` to imports. Add `use crate::pool::PoolManager;` to imports.

Change `SandboxServiceImpl` struct to:

```rust
pub struct SandboxServiceImpl {
    docker: DockerProvider,
    #[cfg(target_os = "linux")]
    firecracker: Option<FirecrackerProvider>,
    wasm: Option<WasmProvider>,
    pool_manager: Arc<PoolManager>,
}
```

Change `SandboxServiceImpl::new()` to take `pool_manager: Arc<PoolManager>`:

```rust
impl SandboxServiceImpl {
    pub fn new(pool_manager: Arc<PoolManager>) -> Self {
        Self {
            docker: DockerProvider::new(),
            #[cfg(target_os = "linux")]
            firecracker: FirecrackerProvider::new().ok(),
            wasm: WasmProvider::new().ok(),
            pool_manager,
        }
    }
}
```

- [ ] **Step 2: Modify create handler to try pool first**

Replace the `create()` handler body (lines 95-131) to try pool before direct create:

```rust
    async fn create(
        &self,
        request: Request<proto::CreateRequest>,
    ) -> Result<Response<proto::CreateResponse>, Status> {
        let req = request.into_inner();
        let config = types::SandboxConfig {
            provider: req.provider.clone(),
            image: if req.image.is_empty() {
                "python:3.12-slim".to_string()
            } else {
                req.image
            },
            memory: req.memory,
            cpus: req.cpus,
            timeout_secs: default_timeout(req.timeout_secs),
            network: req.network,
            writable: req.writable,
            env: req.env,
            mounts: req
                .mounts
                .into_iter()
                .map(|m| types::MountConfig {
                    host_path: m.host_path,
                    container_path: m.container_path,
                    readonly: m.readonly,
                })
                .collect(),
            kernel: req.kernel,
            rootfs: req.rootfs,
        };

        // Try pool first
        if let Some(id) = self.pool_manager.try_acquire(&config).await {
            return Ok(Response::new(proto::CreateResponse { sandbox_id: id }));
        }

        // Pool miss or bypass — direct create
        let provider_name = default_provider(&config.provider);
        with_provider!(self, provider_name, |p| {
            let id = p.create(&config).await.map_err(provider_error_to_status)?;
            Ok(Response::new(proto::CreateResponse { sandbox_id: id }))
        })
    }
```

- [ ] **Step 3: Modify destroy handler to notify pool**

In the `destroy()` handler, after each successful destroy, add pool notification. Replace the destroy body:

```rust
    async fn destroy(
        &self,
        request: Request<proto::DestroyRequest>,
    ) -> Result<Response<proto::DestroyResponse>, Status> {
        let req = request.into_inner();
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            let targets = if req.all {
                p.list()
                    .await
                    .map_err(provider_error_to_status)?
                    .into_iter()
                    .map(|sb| sb.id)
                    .collect()
            } else {
                req.sandbox_ids
            };
            let mut destroyed = Vec::new();
            for id in &targets {
                if p.destroy(id).await.is_ok() {
                    self.pool_manager.on_destroy(id).await;
                    destroyed.push(id.clone());
                }
            }
            Ok(Response::new(proto::DestroyResponse {
                destroyed_ids: destroyed,
            }))
        })
    }
```

- [ ] **Step 4: Add pool RPC handler implementations**

Add these methods inside the `#[tonic::async_trait] impl ... for SandboxServiceImpl` block, after `copy_from`:

```rust
    async fn pool_status(
        &self,
        _request: Request<proto::PoolStatusRequest>,
    ) -> Result<Response<proto::PoolStatusResponse>, Status> {
        let statuses = self.pool_manager.status().await;
        let pools = statuses
            .into_iter()
            .map(|s| proto::PoolInfo {
                provider: s.provider,
                image: s.image,
                idle_count: s.idle_count,
                active_count: s.active_count,
                max_idle: s.max_idle,
                max_total: s.max_total,
            })
            .collect();
        Ok(Response::new(proto::PoolStatusResponse { pools }))
    }

    async fn pool_warmup(
        &self,
        _request: Request<proto::PoolWarmupRequest>,
    ) -> Result<Response<proto::PoolWarmupResponse>, Status> {
        self.pool_manager.warmup().await;
        Ok(Response::new(proto::PoolWarmupResponse {}))
    }

    async fn pool_drain(
        &self,
        _request: Request<proto::PoolDrainRequest>,
    ) -> Result<Response<proto::PoolDrainResponse>, Status> {
        let destroyed = self.pool_manager.drain().await;
        Ok(Response::new(proto::PoolDrainResponse {
            destroyed_count: destroyed,
        }))
    }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p roche-daemon`
Expected: success

- [ ] **Step 6: Commit**

```bash
git add crates/roche-daemon/src/server.rs
git commit -m "feat: integrate PoolManager into gRPC server handlers"
```

---

### Task 9: Update daemon main.rs for pool lifecycle

**Files:**
- Modify: `crates/roche-daemon/src/main.rs`

- [ ] **Step 1: Add --pool CLI arg and pool lifecycle**

Replace the full `main.rs` content:

```rust
use clap::Parser;
use std::sync::Arc;
use tonic::transport::Server;

pub mod proto {
    tonic::include_proto!("roche.v1");
}

mod gc;
mod pool;
mod server;

#[derive(Parser)]
#[command(name = "roche-daemon", about = "Roche sandbox orchestrator daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "50051")]
    port: u16,

    /// Pool configuration (format: provider/image?min=N&max=N&total=N&idle_timeout=N)
    #[arg(long = "pool")]
    pools: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let addr = format!("127.0.0.1:{}", args.port).parse()?;

    // Load pool configs: CLI args override, then fall back to pool.toml
    let pool_configs = if !args.pools.is_empty() {
        let mut configs = Vec::new();
        for arg in &args.pools {
            match pool::config::parse_pool_arg(arg) {
                Ok(cfg) => configs.push(cfg),
                Err(e) => {
                    tracing::error!("invalid --pool arg '{arg}': {e}");
                    std::process::exit(1);
                }
            }
        }
        configs
    } else {
        pool::config::load_pool_toml()
    };

    // Create pool manager
    let pool_manager = Arc::new(pool::PoolManager::new(pool_configs));

    // Spawn pool background tasks
    let replenish_handle = tokio::spawn(pool::replenish::run_replenish_loop(
        pool_manager.inner.clone(),
        pool_manager.replenish_notify.clone(),
    ));
    let reaper_handle = tokio::spawn(pool::reaper::run_reaper_loop(
        pool_manager.inner.clone(),
    ));

    // Initial warmup
    pool_manager.initial_warmup().await;

    let service = server::SandboxServiceImpl::new(pool_manager.clone());
    let svc = proto::sandbox_service_server::SandboxServiceServer::new(service);

    // Write daemon.json
    let roche_dir = dirs::home_dir()
        .expect("cannot find home directory")
        .join(".roche");
    std::fs::create_dir_all(&roche_dir)?;
    let daemon_json = roche_dir.join("daemon.json");
    let info = serde_json::json!({
        "pid": std::process::id(),
        "port": args.port
    });
    std::fs::write(&daemon_json, serde_json::to_string_pretty(&info)?)?;

    tracing::info!("roche-daemon listening on {}", addr);

    // Spawn background GC
    let gc_handle = tokio::spawn(gc::run_gc_loop());

    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutting down");
    };

    Server::builder()
        .add_service(svc)
        .serve_with_shutdown(addr, shutdown)
        .await?;

    // Shutdown pool — drain idle sandboxes
    pool_manager.shutdown().await;

    gc_handle.abort();
    replenish_handle.abort();
    reaper_handle.abort();

    // Clean up daemon.json
    let _ = std::fs::remove_file(&daemon_json);

    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p roche-daemon`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/roche-daemon/src/main.rs
git commit -m "feat: add pool config loading, background task spawning, and shutdown drain to daemon"
```

---

## Chunk 4: CLI Pool Subcommand

### Task 10: Add `roche pool` CLI subcommand

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Add Pool subcommand to Commands enum**

After the `Daemon` variant in the `Commands` enum, add:

```rust
    /// Manage sandbox pool
    Pool {
        #[command(subcommand)]
        action: PoolAction,
    },
```

Add a new enum:

```rust
#[derive(Subcommand, Clone)]
enum PoolAction {
    /// Show pool status (idle/active counts)
    Status,
    /// Trigger immediate warmup for all pools
    Warmup,
    /// Destroy all idle sandboxes in pools
    Drain,
}
```

- [ ] **Step 2: Add pool dispatch in run_via_grpc**

Add the `Pool` match arm in `run_via_grpc` (before `Commands::Daemon`):

```rust
        Commands::Pool { action } => {
            match action {
                PoolAction::Status => {
                    let resp = client
                        .pool_status(proto::PoolStatusRequest {})
                        .await
                        .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                    let pools = resp.into_inner().pools;
                    if pools.is_empty() {
                        println!("No pools configured.");
                    } else {
                        println!("{:<14} {:<30} {:<6} {:<8} {:<10} {:<10}", "PROVIDER", "IMAGE", "IDLE", "ACTIVE", "MAX_IDLE", "MAX_TOTAL");
                        for p in &pools {
                            println!("{:<14} {:<30} {:<6} {:<8} {:<10} {:<10}", p.provider, p.image, p.idle_count, p.active_count, p.max_idle, p.max_total);
                        }
                    }
                }
                PoolAction::Warmup => {
                    client
                        .pool_warmup(proto::PoolWarmupRequest {})
                        .await
                        .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                    println!("Pool warmup triggered.");
                }
                PoolAction::Drain => {
                    let resp = client
                        .pool_drain(proto::PoolDrainRequest {})
                        .await
                        .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                    println!("Drained {} idle sandboxes.", resp.into_inner().destroyed_count);
                }
            }
        }
```

- [ ] **Step 3: Handle Pool command in direct-mode path**

In the `run()` function, add handling before daemon dispatch (around line 744):

```rust
    // Handle pool subcommand (daemon-only)
    if let Commands::Pool { .. } = cli.command {
        if let Some(result) = try_daemon_dispatch(&cli).await {
            return result;
        }
        eprintln!("Error: pool commands require a running daemon");
        std::process::exit(1);
    }
```

Also add `Commands::Pool { .. } => unreachable!("pool handled earlier"),` to the `run_provider_commands!` macro's match.

- [ ] **Step 4: Rebuild CLI proto (proto file shared)**

The CLI needs the updated proto file. Ensure `crates/roche-cli/proto/roche/v1/sandbox.proto` matches the daemon's proto (they should be the same file or symlinked).

Run: `cargo build -p roche-cli`

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p roche-cli`
Expected: success

- [ ] **Step 6: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat: add roche pool status/warmup/drain CLI subcommands"
```

---

## Chunk 5: Build, Test, Verify

### Task 11: Full build and test verification

- [ ] **Step 1: Run cargo fmt**

Run: `cargo fmt --all`

- [ ] **Step 2: Run cargo clippy**

Run: `cargo clippy --all-targets`
Fix any warnings.

- [ ] **Step 3: Run all tests**

Run: `cargo test --all`
Expected: All tests pass

- [ ] **Step 4: Run cargo build**

Run: `cargo build`
Expected: success

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting for pool feature"
```

---

### Task 12: Sync CLI proto file

The CLI crate has its own copy of the proto file. It needs the pool RPCs added too.

- [ ] **Step 1: Check if proto files are shared or duplicated**

Run: `diff crates/roche-daemon/proto/roche/v1/sandbox.proto crates/roche-cli/proto/roche/v1/sandbox.proto`

If different, copy the daemon's proto to CLI:

```bash
cp crates/roche-daemon/proto/roche/v1/sandbox.proto crates/roche-cli/proto/roche/v1/sandbox.proto
```

- [ ] **Step 2: Rebuild both crates**

Run: `cargo build`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/roche-cli/proto/
git commit -m "chore: sync CLI proto with daemon (add pool RPCs)"
```
