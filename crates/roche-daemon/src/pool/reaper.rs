use super::PoolManagerInner;
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

        tracing::info!(
            "pool reaper: destroying {} idle sandboxes",
            to_destroy.len()
        );

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
