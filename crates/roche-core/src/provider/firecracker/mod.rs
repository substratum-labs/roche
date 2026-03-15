// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

pub mod api_client;
pub mod state;
pub mod vsock_exec;

use crate::provider::{ProviderError, SandboxLifecycle, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo, SandboxStatus};
use state::{StateManager, VmMetadata};

pub struct FirecrackerProvider {
    state: StateManager,
}

impl FirecrackerProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            state: StateManager::new()?,
        })
    }

    /// For testing: create with a custom state directory.
    #[cfg(test)]
    pub fn with_state(state: StateManager) -> Self {
        Self { state }
    }

    /// Check that we're on Linux.
    fn check_platform() -> Result<(), ProviderError> {
        if cfg!(not(target_os = "linux")) {
            return Err(ProviderError::Unavailable(
                "Firecracker requires Linux with KVM".into(),
            ));
        }
        Ok(())
    }

    /// Validate that kernel and rootfs are provided and exist.
    fn validate_config(config: &SandboxConfig) -> Result<(&str, &str), ProviderError> {
        let kernel = config.kernel.as_deref().ok_or_else(|| {
            ProviderError::CreateFailed("--kernel is required for Firecracker provider".into())
        })?;
        let rootfs = config.rootfs.as_deref().ok_or_else(|| {
            ProviderError::CreateFailed("--rootfs is required for Firecracker provider".into())
        })?;

        if !std::path::Path::new(kernel).exists() {
            return Err(ProviderError::CreateFailed(format!(
                "kernel not found: {kernel}"
            )));
        }
        if !std::path::Path::new(rootfs).exists() {
            return Err(ProviderError::CreateFailed(format!(
                "rootfs not found: {rootfs}"
            )));
        }

        Ok((kernel, rootfs))
    }

    /// Parse memory string (e.g. "512m") to MiB. Defaults to 128 MiB.
    fn parse_memory_mib(memory: &Option<String>) -> u64 {
        match memory.as_deref() {
            Some(s) => {
                let s = s.trim().to_lowercase();
                if let Some(num) = s.strip_suffix('m') {
                    num.parse::<u64>().unwrap_or(128)
                } else if let Some(num) = s.strip_suffix('g') {
                    num.parse::<u64>().unwrap_or(1) * 1024
                } else {
                    s.parse::<u64>().unwrap_or(128)
                }
            }
            None => 128,
        }
    }

    /// Allocate a unique CID for vsock. CIDs 0-2 are reserved.
    fn allocate_cid() -> u32 {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        ((ts % (u32::MAX as u128 - 3)) + 3) as u32
    }

    /// Check if a process with the given PID is still alive.
    #[cfg(target_os = "linux")]
    fn is_process_alive(pid: u32) -> bool {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
    }

    #[cfg(not(target_os = "linux"))]
    fn is_process_alive(_pid: u32) -> bool {
        false
    }

    /// Kill a process by PID.
    #[cfg(target_os = "linux")]
    fn kill_process(pid: u32) -> Result<(), ProviderError> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let pid = Pid::from_raw(pid as i32);
        kill(pid, Signal::SIGKILL)
            .map_err(|e| ProviderError::ExecFailed(format!("failed to kill process: {e}")))?;

        for _ in 0..10 {
            if kill(pid, None).is_err() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    fn kill_process(_pid: u32) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported(
            "process management requires Linux".into(),
        ))
    }
}

impl Default for FirecrackerProvider {
    fn default() -> Self {
        Self::new().expect("failed to initialize FirecrackerProvider")
    }
}

impl SandboxProvider for FirecrackerProvider {
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        Self::check_platform()?;
        let (kernel, rootfs) = Self::validate_config(config)?;

        let id = uuid::Uuid::new_v4().to_string();
        let cid = Self::allocate_cid();

        // Create state directory and copy rootfs
        self.state.create_vm_dir(&id)?;
        let rootfs_copy = self.state.copy_rootfs(&id, std::path::Path::new(rootfs))?;

        // Spawn firecracker process
        let socket_path = self.state.socket_path(&id);
        let child = tokio::process::Command::new("firecracker")
            .arg("--api-sock")
            .arg(&socket_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                let _ = self.state.remove_vm_dir(&id);
                ProviderError::CreateFailed(format!("failed to spawn firecracker: {e}"))
            })?;

        let pid = child.id().unwrap_or(0);

        // Wait for socket to appear
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if !socket_path.exists() {
            let _ = Self::kill_process(pid);
            let _ = self.state.remove_vm_dir(&id);
            return Err(ProviderError::CreateFailed(
                "firecracker socket did not appear".into(),
            ));
        }

        // Configure VM via API
        let api = api_client::FirecrackerApiClient::new(socket_path);
        let mem_mib = Self::parse_memory_mib(&config.memory);
        let vcpus = config.cpus.map(|c| c.ceil() as u8).unwrap_or(1);
        let boot_args = "console=ttyS0 reboot=k panic=1 pci=off";

        // Cleanup helper for API errors
        macro_rules! try_api {
            ($expr:expr) => {
                if let Err(e) = $expr {
                    let _ = Self::kill_process(pid);
                    let _ = self.state.remove_vm_dir(&id);
                    return Err(e);
                }
            };
        }

        try_api!(api.put_boot_source(kernel, boot_args).await);
        try_api!(
            api.put_drive(
                "rootfs",
                &rootfs_copy.to_string_lossy(),
                true,
                !config.writable
            )
            .await
        );
        try_api!(api.put_machine_config(vcpus, mem_mib).await);
        try_api!(api.put_vsock(cid).await);
        try_api!(api.start().await);

        // Write metadata
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metadata = VmMetadata {
            id: id.clone(),
            provider: "firecracker".to_string(),
            image: "custom".to_string(),
            pid,
            cid,
            kernel: kernel.to_string(),
            expires_at: Some(now + config.timeout_secs),
            created_at: now,
        };
        self.state.write_metadata(&metadata)?;

        Ok(id)
    }

    async fn exec(
        &self,
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        Self::check_platform()?;
        let metadata = self.state.read_metadata(id)?;

        if !Self::is_process_alive(metadata.pid) {
            return Err(ProviderError::NotFound(id.clone()));
        }

        let timeout = request.timeout_secs.unwrap_or(300);
        vsock_exec::exec_via_vsock(metadata.cid, request, timeout).await
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError> {
        let metadata = self.state.read_metadata(id)?;

        if Self::is_process_alive(metadata.pid) {
            Self::kill_process(metadata.pid)?;
        }

        self.state.remove_vm_dir(id)
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        let ids = self.state.list_vm_ids()?;
        let mut infos = Vec::new();

        for id in ids {
            if let Ok(metadata) = self.state.read_metadata(&id) {
                let status = if Self::is_process_alive(metadata.pid) {
                    SandboxStatus::Running
                } else {
                    SandboxStatus::Stopped
                };

                infos.push(SandboxInfo {
                    id: metadata.id,
                    status,
                    provider: "firecracker".to_string(),
                    image: metadata.image,
                    expires_at: metadata.expires_at,
                });
            }
        }

        Ok(infos)
    }
}

impl SandboxLifecycle for FirecrackerProvider {
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        Self::check_platform()?;
        let metadata = self.state.read_metadata(id)?;
        if !Self::is_process_alive(metadata.pid) {
            return Err(ProviderError::NotFound(id.clone()));
        }

        let api = api_client::FirecrackerApiClient::new(self.state.socket_path(id));
        api.pause().await
    }

    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        Self::check_platform()?;
        let metadata = self.state.read_metadata(id)?;
        if !Self::is_process_alive(metadata.pid) {
            return Err(ProviderError::NotFound(id.clone()));
        }

        let api = api_client::FirecrackerApiClient::new(self.state.socket_path(id));
        api.resume().await
    }

    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError> {
        let infos = self.list().await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut destroyed = Vec::new();
        for info in infos {
            if let Some(exp) = info.expires_at {
                if exp <= now && self.destroy(&info.id).await.is_ok() {
                    destroyed.push(info.id);
                }
            }
        }

        Ok(destroyed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_mib() {
        assert_eq!(FirecrackerProvider::parse_memory_mib(&None), 128);
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("512m".into())),
            512
        );
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("2g".into())),
            2048
        );
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("256".into())),
            256
        );
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("invalid".into())),
            128
        );
    }

    #[test]
    fn test_allocate_cid() {
        let cid = FirecrackerProvider::allocate_cid();
        assert!(cid >= 3, "CID must be >= 3, got {cid}");
    }

    #[test]
    fn test_check_platform() {
        let result = FirecrackerProvider::check_platform();
        if cfg!(target_os = "linux") {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Linux"));
        }
    }

    #[test]
    fn test_validate_config_missing_kernel() {
        let config = SandboxConfig {
            provider: "firecracker".into(),
            rootfs: Some("/tmp/rootfs.ext4".into()),
            ..Default::default()
        };
        let result = FirecrackerProvider::validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--kernel"));
    }

    #[test]
    fn test_validate_config_missing_rootfs() {
        let config = SandboxConfig {
            provider: "firecracker".into(),
            kernel: Some("/tmp/vmlinux".into()),
            ..Default::default()
        };
        let result = FirecrackerProvider::validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--rootfs"));
    }

    #[test]
    fn test_validate_config_nonexistent_kernel() {
        let config = SandboxConfig {
            provider: "firecracker".into(),
            kernel: Some("/nonexistent/vmlinux".into()),
            rootfs: Some("/nonexistent/rootfs.ext4".into()),
            ..Default::default()
        };
        let result = FirecrackerProvider::validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("kernel not found"));
    }

    #[test]
    fn test_list_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let state = StateManager::with_base_dir(tmp.path().to_path_buf());
        let provider = FirecrackerProvider::with_state(state);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let infos = rt.block_on(provider.list()).unwrap();
        assert!(infos.is_empty());
    }

    #[test]
    fn test_destroy_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let state = StateManager::with_base_dir(tmp.path().to_path_buf());
        let provider = FirecrackerProvider::with_state(state);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(provider.destroy(&"nonexistent".to_string()));
        assert!(result.is_err());
    }
}
