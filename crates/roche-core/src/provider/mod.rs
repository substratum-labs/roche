pub mod docker;

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
}
