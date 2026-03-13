use crate::provider::{ProviderError, SandboxFileOps, SandboxLifecycle, SandboxProvider};
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
                ProviderError::Unavailable("Docker is not installed or not in PATH".into())
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

    // Expiry timestamp
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + config.timeout_secs;
    args.extend(["--label".into(), format!("roche.expires={expires}")]);

    // Environment variables
    for (k, v) in &config.env {
        args.extend(["-e".into(), format!("{k}={v}")]);
    }

    // Volume mounts
    for mount in &config.mounts {
        let mode = if mount.readonly { "ro" } else { "rw" };
        args.extend([
            "-v".into(),
            format!("{}:{}:{}", mount.host_path, mount.container_path, mode),
        ]);
    }

    // Image + keep-alive command
    args.push(config.image.clone());
    args.extend(["sleep".into(), "infinity".into()]);

    args
}

/// Build the argument list for `docker exec`.
fn build_exec_args(id: &SandboxId, request: &ExecRequest) -> Vec<String> {
    let mut args = vec!["exec".to_string(), id.clone()];
    args.extend(request.command.clone());
    args
}

/// Map Docker container state string to SandboxStatus.
fn parse_status(state: &str) -> SandboxStatus {
    match state {
        "running" => SandboxStatus::Running,
        "paused" => SandboxStatus::Paused,
        "exited" | "created" => SandboxStatus::Stopped,
        _ => SandboxStatus::Failed,
    }
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
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        let args = build_exec_args(id, request);
        let timeout_secs = request.timeout_secs.unwrap_or(300);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            Command::new("docker").args(&args).output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
                if exit_code != 0 && stderr_str.contains("is paused") {
                    return Err(ProviderError::Paused(id.clone()));
                }
                Ok(ExecOutput {
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: stderr_str,
                })
            }
            Ok(Err(e)) => Err(ProviderError::ExecFailed(e.to_string())),
            Err(_) => Err(ProviderError::Timeout(timeout_secs)),
        }
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError> {
        // Graceful stop first (ignore errors — container may already be stopped)
        let _ = Command::new("docker")
            .args(["stop", "-t", "5", id])
            .output()
            .await;

        // Force remove
        let output = Command::new("docker")
            .args(["rm", "-f", id])
            .output()
            .await
            .map_err(|e| ProviderError::ExecFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                return Err(ProviderError::NotFound(id.clone()));
            }
            return Err(ProviderError::ExecFailed(stderr.trim().to_string()));
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                "label=roche.managed=true",
                "--format",
                "{{.ID}}\t{{.State}}\t{{.Image}}\t{{index .Labels \"roche.expires\"}}",
            ])
            .output()
            .await
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::Unavailable(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let sandboxes = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                SandboxInfo {
                    id: parts.first().unwrap_or(&"").to_string(),
                    status: parse_status(parts.get(1).unwrap_or(&"unknown")),
                    provider: "docker".to_string(),
                    image: parts.get(2).unwrap_or(&"").to_string(),
                    expires_at: parts.get(3).and_then(|s| s.parse::<u64>().ok()),
                }
            })
            .collect();

        Ok(sandboxes)
    }
}

impl SandboxLifecycle for DockerProvider {
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        let output = Command::new("docker")
            .args(["pause", id])
            .output()
            .await
            .map_err(|e| ProviderError::ExecFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                return Err(ProviderError::NotFound(id.clone()));
            }
            return Err(ProviderError::ExecFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        let output = Command::new("docker")
            .args(["unpause", id])
            .output()
            .await
            .map_err(|e| ProviderError::ExecFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                return Err(ProviderError::NotFound(id.clone()));
            }
            return Err(ProviderError::ExecFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError> {
        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                "label=roche.managed=true",
                "--format",
                "{{.ID}}\t{{index .Labels \"roche.expires\"}}",
            ])
            .output()
            .await
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::Unavailable(stderr.trim().to_string()));
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut destroyed = Vec::new();

        for line in stdout.lines().filter(|l| !l.is_empty()) {
            let parts: Vec<&str> = line.split('\t').collect();
            let id = parts.first().unwrap_or(&"").to_string();
            let expires = parts.get(1).and_then(|s| s.parse::<u64>().ok());

            if let Some(exp) = expires {
                if exp <= now {
                    if let Ok(()) = self.destroy(&id).await {
                        destroyed.push(id);
                    }
                }
            }
        }

        Ok(destroyed)
    }
}

impl SandboxFileOps for DockerProvider {
    async fn copy_to(
        &self,
        id: &SandboxId,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), ProviderError> {
        let output = Command::new("docker")
            .args(["cp", &src.to_string_lossy(), &format!("{id}:{dest}")])
            .output()
            .await
            .map_err(|e| ProviderError::FileFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                return Err(ProviderError::NotFound(id.clone()));
            }
            return Err(ProviderError::FileFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    async fn copy_from(
        &self,
        id: &SandboxId,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), ProviderError> {
        let output = Command::new("docker")
            .args(["cp", &format!("{id}:{src}"), &dest.to_string_lossy()])
            .output()
            .await
            .map_err(|e| ProviderError::FileFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                return Err(ProviderError::NotFound(id.clone()));
            }
            return Err(ProviderError::FileFailed(stderr.trim().to_string()));
        }
        Ok(())
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
        let has_network_none = args
            .windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none");
        assert!(!has_network_none);
        assert!(!args.contains(&"--read-only".to_string()));
        assert!(args.contains(&"--memory".to_string()));
        assert!(args.contains(&"512m".to_string()));
        assert!(args.contains(&"--cpus".to_string()));
        assert!(args.contains(&"1.5".to_string()));
    }

    #[test]
    fn test_parse_status() {
        assert_eq!(parse_status("running"), SandboxStatus::Running);
        assert_eq!(parse_status("exited"), SandboxStatus::Stopped);
        assert_eq!(parse_status("created"), SandboxStatus::Stopped);
        assert_eq!(parse_status("dead"), SandboxStatus::Failed);
        assert_eq!(parse_status("anything_else"), SandboxStatus::Failed);
    }

    #[test]
    fn test_parse_status_paused() {
        assert_eq!(parse_status("paused"), SandboxStatus::Paused);
    }

    #[test]
    fn test_build_exec_args() {
        let id = "abc123def456".to_string();
        let request = ExecRequest {
            command: vec!["python3".into(), "-c".into(), "print('hi')".into()],
            timeout_secs: None,
        };
        let args = build_exec_args(&id, &request);

        assert_eq!(args[0], "exec");
        assert_eq!(args[1], "abc123def456");
        assert_eq!(args[2], "python3");
        assert_eq!(args[3], "-c");
        assert_eq!(args[4], "print('hi')");
    }

    #[test]
    fn test_build_create_args_with_mounts() {
        use crate::types::MountConfig;
        let config = SandboxConfig {
            mounts: vec![
                MountConfig {
                    host_path: "/host/data".into(),
                    container_path: "/sandbox/data".into(),
                    readonly: true,
                },
                MountConfig {
                    host_path: "/host/out".into(),
                    container_path: "/sandbox/out".into(),
                    readonly: false,
                },
            ],
            ..Default::default()
        };
        let args = build_create_args(&config);

        let v_positions: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "-v")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(v_positions.len(), 2);
        assert_eq!(args[v_positions[0] + 1], "/host/data:/sandbox/data:ro");
        assert_eq!(args[v_positions[1] + 1], "/host/out:/sandbox/out:rw");
    }

    #[test]
    fn test_build_create_args_has_expires_label() {
        let config = SandboxConfig::default();
        let args = build_create_args(&config);

        let label_positions: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "--label")
            .map(|(i, _)| i)
            .collect();

        let expires_label = label_positions
            .iter()
            .find(|&&i| args[i + 1].starts_with("roche.expires="))
            .expect("should have roche.expires label");

        let value: u64 = args[*expires_label + 1]
            .strip_prefix("roche.expires=")
            .unwrap()
            .parse()
            .expect("expires should be a unix timestamp");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(value >= now + 295 && value <= now + 305);
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
