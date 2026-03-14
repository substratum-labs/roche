pub mod docker;
pub mod firecracker;
#[cfg(feature = "wasmtime")]
pub mod wasm;

use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo};

/// Trait that all sandbox providers must implement.
#[allow(async_fn_in_trait)]
pub trait SandboxProvider {
    /// Create a new sandbox, returning its unique ID.
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError>;

    /// Execute a command inside an existing sandbox.
    async fn exec(
        &self,
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError>;

    /// Destroy a sandbox and release all resources.
    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError>;

    /// List all active sandboxes managed by this provider.
    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("sandbox not found: {0}")]
    NotFound(SandboxId),

    #[error("sandbox creation failed: {0}")]
    CreateFailed(String),

    #[error("command execution failed: {0}")]
    ExecFailed(String),

    #[error("provider unavailable: {0}")]
    Unavailable(String),

    #[error("timeout after {0}s")]
    Timeout(u64),

    #[error("operation not supported by this provider: {0}")]
    Unsupported(String),

    #[error("file operation failed: {0}")]
    FileFailed(String),

    #[error("sandbox is paused: {0}")]
    Paused(SandboxId),
}

/// File operations capability — not all providers support this.
#[allow(async_fn_in_trait)]
pub trait SandboxFileOps {
    /// Copy a file from host to sandbox.
    async fn copy_to(
        &self,
        id: &SandboxId,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), ProviderError>;

    /// Copy a file from sandbox to host.
    async fn copy_from(
        &self,
        id: &SandboxId,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), ProviderError>;
}

/// Lifecycle management capability — not all providers support this.
#[allow(async_fn_in_trait)]
pub trait SandboxLifecycle {
    /// Pause a sandbox (freeze all processes).
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError>;

    /// Unpause a sandbox.
    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError>;

    /// Garbage collect: destroy all expired sandboxes. Returns IDs of destroyed sandboxes.
    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError>;
}
