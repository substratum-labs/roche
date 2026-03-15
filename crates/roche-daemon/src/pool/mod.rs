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
                tracing::warn!(
                    "WASM pooling is unnecessary, skipping pool config for {}",
                    cfg.image
                );
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
                key,
                cfg.min_idle,
                cfg.max_idle,
                cfg.max_total
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
    /// Returns Some(id) on pool hit, None on bypass (caller should create directly).
    /// On miss, creates sandbox via provider and tracks it in active set.
    pub async fn try_acquire(&self, config: &SandboxConfig) -> Option<SandboxId> {
        // WASM bypass
        let provider = if config.provider.is_empty() {
            "docker"
        } else {
            &config.provider
        };
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
            tracing::debug!(
                "pool bypass: non-default config (network={}, writable={}, env={}, mounts={})",
                config.network,
                config.writable,
                config.env.len(),
                config.mounts.len()
            );
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
        let mut found_key = None;
        for (key, pool) in inner.pools.iter_mut() {
            if pool.active.remove(sandbox_id) {
                tracing::debug!("pool: removed {sandbox_id} from active set of {key}");
                found_key = Some(key.clone());
                break;
            }
        }
        if let Some(key) = found_key {
            inner.pending_replenish.insert(key);
            drop(inner);
            self.replenish_notify.notify_one();
        }
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

    fn make_config(
        provider: &str,
        image: &str,
        min: usize,
        max: usize,
        total: usize,
    ) -> PoolConfig {
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
            ..Default::default()
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_bypass_non_default_network() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 10)];
        let pm = PoolManager::new(configs);
        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            network: true,
            ..Default::default()
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_bypass_non_default_writable() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 10)];
        let pm = PoolManager::new(configs);
        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            writable: true,
            ..Default::default()
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_bypass_with_memory() {
        let configs = vec![make_config("docker", "python:3.12-slim", 0, 5, 10)];
        let pm = PoolManager::new(configs);
        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            memory: Some("512m".to_string()),
            ..Default::default()
        };
        assert!(pm.try_acquire(&config).await.is_none());
    }

    #[tokio::test]
    async fn test_try_acquire_no_pool_configured() {
        let pm = PoolManager::new(vec![]);
        let config = SandboxConfig {
            provider: "docker".to_string(),
            image: "python:3.12-slim".to_string(),
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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

    #[tokio::test]
    async fn test_pool_status() {
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
            pool.idle.push_back(IdleSandbox {
                id: "sb-2".to_string(),
                created_at: Instant::now(),
            });
        }

        let statuses = pm.status().await;
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].idle_count, 1);
        assert_eq!(statuses[0].active_count, 1);
        assert_eq!(statuses[0].max_idle, 5);
        assert_eq!(statuses[0].max_total, 10);
    }
}
