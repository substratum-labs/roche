use crate::provider::ProviderError;
use crate::types::SandboxId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent metadata for a Firecracker VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMetadata {
    pub id: SandboxId,
    pub provider: String,
    pub image: String,
    pub pid: u32,
    pub cid: u32,
    pub kernel: String,
    pub expires_at: Option<u64>,
    pub created_at: u64,
}

/// Manages per-VM state directories under `~/.roche/firecracker/`.
pub struct StateManager {
    base_dir: PathBuf,
}

impl StateManager {
    /// Create a new StateManager. Uses `~/.roche/firecracker/` by default.
    pub fn new() -> Result<Self, ProviderError> {
        let home = dirs::home_dir()
            .ok_or_else(|| ProviderError::Unavailable("cannot determine home directory".into()))?;
        let base_dir = home.join(".roche").join("firecracker");
        Ok(Self { base_dir })
    }

    /// Create with a custom base directory (for testing).
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Path to a specific VM's state directory.
    pub fn vm_dir(&self, id: &str) -> PathBuf {
        self.base_dir.join(id)
    }

    /// Path to the API socket for a VM.
    pub fn socket_path(&self, id: &str) -> PathBuf {
        self.vm_dir(id).join("firecracker.sock")
    }

    /// Path to the rootfs copy for a VM.
    pub fn rootfs_path(&self, id: &str) -> PathBuf {
        self.vm_dir(id).join("rootfs.ext4")
    }

    /// Path to the metadata file for a VM.
    pub fn metadata_path(&self, id: &str) -> PathBuf {
        self.vm_dir(id).join("metadata.json")
    }

    /// Create the state directory for a new VM.
    pub fn create_vm_dir(&self, id: &str) -> Result<PathBuf, ProviderError> {
        let dir = self.vm_dir(id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| ProviderError::CreateFailed(format!("failed to create state dir: {e}")))?;
        Ok(dir)
    }

    /// Copy the rootfs image into the VM's state directory.
    pub fn copy_rootfs(&self, id: &str, source: &Path) -> Result<PathBuf, ProviderError> {
        let dest = self.rootfs_path(id);
        std::fs::copy(source, &dest)
            .map_err(|e| ProviderError::CreateFailed(format!("failed to copy rootfs: {e}")))?;
        Ok(dest)
    }

    /// Write VM metadata to disk.
    pub fn write_metadata(&self, metadata: &VmMetadata) -> Result<(), ProviderError> {
        let path = self.metadata_path(&metadata.id);
        let json = serde_json::to_string_pretty(metadata)
            .map_err(|e| ProviderError::CreateFailed(format!("failed to serialize metadata: {e}")))?;
        std::fs::write(&path, json)
            .map_err(|e| ProviderError::CreateFailed(format!("failed to write metadata: {e}")))?;
        Ok(())
    }

    /// Read VM metadata from disk.
    pub fn read_metadata(&self, id: &str) -> Result<VmMetadata, ProviderError> {
        let path = self.metadata_path(id);
        let json = std::fs::read_to_string(&path)
            .map_err(|_| ProviderError::NotFound(id.to_string()))?;
        serde_json::from_str(&json)
            .map_err(|e| ProviderError::ExecFailed(format!("corrupt metadata: {e}")))
    }

    /// Remove the entire state directory for a VM.
    pub fn remove_vm_dir(&self, id: &str) -> Result<(), ProviderError> {
        let dir = self.vm_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| {
                ProviderError::ExecFailed(format!("failed to remove state dir: {e}"))
            })?;
        }
        Ok(())
    }

    /// List all VM IDs by scanning subdirectories.
    pub fn list_vm_ids(&self) -> Result<Vec<String>, ProviderError> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&self.base_dir)
            .map_err(|e| ProviderError::Unavailable(format!("cannot read state dir: {e}")))?;

        let mut ids = Vec::new();
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_state_manager() -> (StateManager, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = StateManager::with_base_dir(tmp.path().to_path_buf());
        (mgr, tmp)
    }

    #[test]
    fn test_create_and_remove_vm_dir() {
        let (mgr, _tmp) = temp_state_manager();
        let dir = mgr.create_vm_dir("test-vm-1").unwrap();
        assert!(dir.exists());
        mgr.remove_vm_dir("test-vm-1").unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn test_remove_nonexistent_vm_dir_is_ok() {
        let (mgr, _tmp) = temp_state_manager();
        assert!(mgr.remove_vm_dir("does-not-exist").is_ok());
    }

    #[test]
    fn test_write_and_read_metadata() {
        let (mgr, _tmp) = temp_state_manager();
        mgr.create_vm_dir("vm-meta").unwrap();
        let meta = VmMetadata {
            id: "vm-meta".to_string(),
            provider: "firecracker".to_string(),
            image: "custom".to_string(),
            pid: 12345,
            cid: 3,
            kernel: "/boot/vmlinux".to_string(),
            expires_at: Some(9999999999),
            created_at: 1000000000,
        };
        mgr.write_metadata(&meta).unwrap();
        let read_back = mgr.read_metadata("vm-meta").unwrap();
        assert_eq!(read_back.id, "vm-meta");
        assert_eq!(read_back.pid, 12345);
        assert_eq!(read_back.cid, 3);
    }

    #[test]
    fn test_read_metadata_not_found() {
        let (mgr, _tmp) = temp_state_manager();
        let result = mgr.read_metadata("no-such-vm");
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_rootfs() {
        let (mgr, tmp) = temp_state_manager();
        mgr.create_vm_dir("vm-rootfs").unwrap();
        let src_dir = tmp.path().join("source");
        fs::create_dir_all(&src_dir).unwrap();
        let src_file = src_dir.join("rootfs.ext4");
        fs::write(&src_file, b"fake rootfs content").unwrap();
        let dest = mgr.copy_rootfs("vm-rootfs", &src_file).unwrap();
        assert!(dest.exists());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "fake rootfs content");
    }

    #[test]
    fn test_list_vm_ids_empty() {
        let (mgr, _tmp) = temp_state_manager();
        let ids = mgr.list_vm_ids().unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn test_list_vm_ids() {
        let (mgr, _tmp) = temp_state_manager();
        mgr.create_vm_dir("vm-a").unwrap();
        mgr.create_vm_dir("vm-b").unwrap();
        let mut ids = mgr.list_vm_ids().unwrap();
        ids.sort();
        assert_eq!(ids, vec!["vm-a", "vm-b"]);
    }

    #[test]
    fn test_path_helpers() {
        let (mgr, tmp) = temp_state_manager();
        let base = tmp.path();
        assert_eq!(mgr.socket_path("vm1"), base.join("vm1").join("firecracker.sock"));
        assert_eq!(mgr.rootfs_path("vm1"), base.join("vm1").join("rootfs.ext4"));
        assert_eq!(mgr.metadata_path("vm1"), base.join("vm1").join("metadata.json"));
    }
}
