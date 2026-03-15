use super::{IdleSandbox, PoolKey, PoolManagerInner};
use roche_core::provider::SandboxProvider;
use roche_core::types::SandboxConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;

/// Background task that replenishes pools when idle count drops below min_idle.
pub async fn run_replenish_loop(inner: Arc<Mutex<PoolManagerInner>>, notify: Arc<Notify>) {
    loop {
        notify.notified().await;

        // Drain pending keys
        let keys: Vec<PoolKey> = {
            let mut state = inner.lock().await;
            state.pending_replenish.drain().collect()
        };

        for key in keys {
            // Calculate deficit
            let deficit = {
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

                (min_idle - current_idle).min(max_total.saturating_sub(current_total))
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

                let handle = tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();

                    let config = SandboxConfig {
                        provider: key_clone.provider.clone(),
                        image: key_clone.image.clone(),
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

                    let create_result = {
                        let state = inner_clone.lock().await;
                        match key_clone.provider.as_str() {
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
                        }
                    };

                    match create_result {
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
