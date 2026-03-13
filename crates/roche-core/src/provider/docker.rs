use crate::provider::{ProviderError, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo};

/// Docker-based sandbox provider.
///
/// Uses the Docker CLI to manage containers with AI-optimized
/// security defaults (no network, readonly filesystem, timeout).
pub struct DockerProvider;

impl DockerProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DockerProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxProvider for DockerProvider {
    async fn create(&self, _config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        todo!("docker create implementation")
    }

    async fn exec(
        &self,
        _id: &SandboxId,
        _request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        todo!("docker exec implementation")
    }

    async fn destroy(&self, _id: &SandboxId) -> Result<(), ProviderError> {
        todo!("docker destroy implementation")
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        todo!("docker list implementation")
    }
}
