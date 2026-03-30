// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

pub mod engine;
pub mod sandbox_state;

use crate::provider::{ProviderError, SandboxLifecycle, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo};
use engine::WasmEngine;
use sandbox_state::{SandboxRegistry, WasmSandbox};

/// WASM-based sandbox provider using Wasmtime + WASI.
///
/// Each sandbox is a pre-compiled WASM module. `exec()` creates a fresh
/// WASI instance per call (no persistent process). Sub-millisecond startup.
pub struct WasmProvider {
    engine: WasmEngine,
    registry: SandboxRegistry,
}

impl WasmProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            engine: WasmEngine::new()?,
            registry: SandboxRegistry::new(),
        })
    }
}

impl Default for WasmProvider {
    fn default() -> Self {
        Self::new().expect("failed to initialize WasmProvider")
    }
}

impl SandboxProvider for WasmProvider {
    fn capabilities(&self) -> crate::provider::capabilities::ProviderCapabilities {
        use crate::provider::capabilities::{FieldSupport, ProviderCapabilities};
        ProviderCapabilities {
            name: "wasm".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Supported,
            network: FieldSupport::NotApplicable,
            mounts: FieldSupport::Supported,
            memory: FieldSupport::NotApplicable,
            cpus: FieldSupport::NotApplicable,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: false,
            unpause: false,
            copy_to: false,
            copy_from: false,
            network_allowlist: FieldSupport::NotApplicable,
            fs_paths: FieldSupport::NotApplicable,
        }
    }

    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        crate::provider::capabilities::validate_config(config, &self.capabilities())?;
        let module = self.engine.compile(&config.image)?;
        let id = uuid::Uuid::new_v4().to_string();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let sandbox = WasmSandbox {
            id: id.clone(),
            module,
            config: config.clone(),
            created_at: now,
            expires_at: Some(now + config.timeout_secs),
        };

        self.registry.insert(sandbox);
        Ok(id)
    }

    async fn exec(
        &self,
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        let (module, config) = self
            .registry
            .get_module_and_config(id)
            .ok_or_else(|| ProviderError::NotFound(id.clone()))?;

        let timeout_secs = request.timeout_secs.unwrap_or(300);

        let engine = WasmEngine::new()?;
        let req = request.clone();
        let cfg = config.clone();
        let mod_clone = module.clone();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking(move || engine.execute(&mod_clone, &cfg, &req)),
        )
        .await;

        match result {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => Err(ProviderError::ExecFailed(format!("task join error: {e}"))),
            Err(_) => Err(ProviderError::Timeout(timeout_secs)),
        }
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError> {
        if self.registry.remove(id) {
            Ok(())
        } else {
            Err(ProviderError::NotFound(id.clone()))
        }
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        Ok(self.registry.list())
    }
}

impl SandboxLifecycle for WasmProvider {
    async fn pause(&self, _id: &SandboxId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported(
            "WASM sandboxes cannot be paused".into(),
        ))
    }

    async fn unpause(&self, _id: &SandboxId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported(
            "WASM sandboxes cannot be unpaused".into(),
        ))
    }

    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError> {
        Ok(self.registry.gc())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_invalid_module_path() {
        let provider = WasmProvider::new().unwrap();
        let config = SandboxConfig {
            provider: "wasm".to_string(),
            image: "/nonexistent/module.wasm".to_string(),
            ..Default::default()
        };
        let result = provider.create(&config).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::CreateFailed(_)
        ));
    }

    #[tokio::test]
    async fn test_exec_not_found() {
        let provider = WasmProvider::new().unwrap();
        let request = ExecRequest {
            command: vec!["test".to_string()],
            timeout_secs: None,
            idempotency_key: None,
        };
        let result = provider.exec(&"no-such-id".to_string(), &request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProviderError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_destroy_not_found() {
        let provider = WasmProvider::new().unwrap();
        let result = provider.destroy(&"ghost".to_string()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProviderError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_list_empty() {
        let provider = WasmProvider::new().unwrap();
        let list = provider.list().await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_pause_unsupported() {
        let provider = WasmProvider::new().unwrap();
        let result = provider.pause(&"any".to_string()).await;
        assert!(matches!(result.unwrap_err(), ProviderError::Unsupported(_)));
    }

    #[tokio::test]
    async fn test_unpause_unsupported() {
        let provider = WasmProvider::new().unwrap();
        let result = provider.unpause(&"any".to_string()).await;
        assert!(matches!(result.unwrap_err(), ProviderError::Unsupported(_)));
    }
}
