use crate::provider::{ProviderError, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo, SandboxStatus};
use tokio::process::Command;

/// Docker-based sandbox provider.
///
/// Uses the Docker CLI to manage containers with AI-optimized
/// security defaults (no network, readonly filesystem, timeout).
pub struct DockerProvider;

impl DockerProvider {
    pub fn new() -> Self {
        Self
    }

    /// Check that Docker is installed and the daemon is running.
    async fn check_available() -> Result<(), ProviderError> {
        let output = Command::new("docker")
            .arg("info")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map_err(|_| {
                ProviderError::Unavailable(
                    "Docker is not installed or not in PATH".into(),
                )
            })?;

        if !output.success() {
            return Err(ProviderError::Unavailable(
                "Docker daemon is not running".into(),
            ));
        }
        Ok(())
    }
}

impl Default for DockerProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the argument list for `docker create`.
fn build_create_args(config: &SandboxConfig) -> Vec<String> {
    let mut args = vec!["create".to_string()];

    // Network isolation (default: none)
    if !config.network {
        args.extend(["--network".into(), "none".into()]);
    }

    // Filesystem isolation (default: read-only)
    if !config.writable {
        args.push("--read-only".into());
    }

    // Resource limits
    if let Some(ref memory) = config.memory {
        args.extend(["--memory".into(), memory.clone()]);
    }
    if let Some(cpus) = config.cpus {
        args.extend(["--cpus".into(), cpus.to_string()]);
    }

    // Security hardening
    args.extend([
        "--pids-limit".into(),
        "256".into(),
        "--security-opt".into(),
        "no-new-privileges".into(),
    ]);

    // Roche management labels
    args.extend(["--label".into(), "roche.managed=true".into()]);

    // Environment variables
    for (k, v) in &config.env {
        args.extend(["-e".into(), format!("{k}={v}")]);
    }

    // Image + keep-alive command
    args.push(config.image.clone());
    args.extend(["sleep".into(), "infinity".into()]);

    args
}

impl SandboxProvider for DockerProvider {
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        Self::check_available().await?;

        let args = build_create_args(config);
        let output = Command::new("docker")
            .args(&args)
            .output()
            .await
            .map_err(|e| ProviderError::CreateFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::CreateFailed(stderr.trim().to_string()));
        }

        let container_id: String = String::from_utf8_lossy(&output.stdout)
            .trim()
            .chars()
            .take(12)
            .collect();

        // Start the container
        let start = Command::new("docker")
            .args(["start", &container_id])
            .output()
            .await
            .map_err(|e| ProviderError::CreateFailed(e.to_string()))?;

        if !start.status.success() {
            let stderr = String::from_utf8_lossy(&start.stderr);
            return Err(ProviderError::CreateFailed(stderr.trim().to_string()));
        }

        Ok(container_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SandboxConfig;

    #[test]
    fn test_build_create_args_defaults() {
        let config = SandboxConfig::default();
        let args = build_create_args(&config);

        assert!(args.contains(&"--network".to_string()));
        assert!(args.contains(&"none".to_string()));
        assert!(args.contains(&"--read-only".to_string()));
        assert!(args.contains(&"--pids-limit".to_string()));
        let label_pos = args.iter().position(|a| a == "--label").unwrap();
        assert_eq!(args[label_pos + 1], "roche.managed=true");
        assert!(args.contains(&"python:3.12-slim".to_string()));
        assert!(!args.contains(&"--memory".to_string()));
        assert!(!args.contains(&"--cpus".to_string()));
    }

    #[test]
    fn test_build_create_args_with_network_and_writable() {
        let config = SandboxConfig {
            network: true,
            writable: true,
            memory: Some("512m".to_string()),
            cpus: Some(1.5),
            ..Default::default()
        };
        let args = build_create_args(&config);

        // Should NOT have --network none
        let has_network_none = args.windows(2).any(|w| w[0] == "--network" && w[1] == "none");
        assert!(!has_network_none);
        assert!(!args.contains(&"--read-only".to_string()));
        assert!(args.contains(&"--memory".to_string()));
        assert!(args.contains(&"512m".to_string()));
        assert!(args.contains(&"--cpus".to_string()));
        assert!(args.contains(&"1.5".to_string()));
    }

    #[test]
    fn test_build_create_args_with_env() {
        let mut config = SandboxConfig::default();
        config.env.insert("FOO".into(), "bar".into());
        let args = build_create_args(&config);

        let env_pos = args.iter().position(|a| a == "-e").unwrap();
        assert_eq!(args[env_pos + 1], "FOO=bar");
    }
}
