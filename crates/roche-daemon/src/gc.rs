// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use roche_core::provider::docker::DockerProvider;
use roche_core::provider::SandboxLifecycle;

/// Runs garbage collection every 60 seconds on all providers.
pub async fn run_gc_loop() {
    let docker = DockerProvider::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;
        match docker.gc().await {
            Ok(ids) => {
                if !ids.is_empty() {
                    tracing::info!("GC destroyed {} sandbox(es)", ids.len());
                }
            }
            Err(e) => {
                tracing::warn!("GC error (docker): {e}");
            }
        }

        #[cfg(target_os = "linux")]
        {
            use roche_core::provider::firecracker::FirecrackerProvider;
            if let Ok(fc) = FirecrackerProvider::new() {
                match fc.gc().await {
                    Ok(ids) => {
                        if !ids.is_empty() {
                            tracing::info!("GC (firecracker) destroyed {} sandbox(es)", ids.len());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("GC error (firecracker): {e}");
                    }
                }
            }
        }
    }
}
