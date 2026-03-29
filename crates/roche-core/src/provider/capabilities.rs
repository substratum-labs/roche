// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::provider::ProviderError;
use crate::types::SandboxConfig;
use serde::{Deserialize, Serialize};

/// How a provider handles a particular configuration field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldSupport {
    /// Provider supports this field in any valid state.
    Supported,
    /// Field has no meaning for this provider (silently ignored).
    NotApplicable,
    /// Provider requires this field to be set.
    Required,
    /// Provider does not support this field; setting it is an error.
    Unsupported,
}

/// Declares what a provider can and cannot do.
///
/// This is a lightweight capability declaration designed to evolve into
/// the full v0.2 Capability Spec (`domain:action:scope`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Provider name (for error messages).
    pub name: String,

    // --- Config field support ---
    /// Whether `writable: true` is supported.
    pub writable_true: FieldSupport,
    /// Whether `writable: false` (read-only filesystem) is supported.
    pub writable_false: FieldSupport,
    /// Whether network configuration is supported.
    pub network: FieldSupport,
    /// Whether volume mounts are supported.
    pub mounts: FieldSupport,
    /// Whether custom memory limits are supported.
    pub memory: FieldSupport,
    /// Whether custom CPU limits are supported.
    pub cpus: FieldSupport,
    /// Whether kernel path is supported/required.
    pub kernel: FieldSupport,
    /// Whether rootfs path is supported/required.
    pub rootfs: FieldSupport,
    /// Whether network allowlist is supported.
    pub network_allowlist: FieldSupport,
    /// Whether filesystem path whitelist is supported.
    pub fs_paths: FieldSupport,

    // --- Operational capabilities ---
    pub pause: bool,
    pub unpause: bool,
    pub copy_to: bool,
    pub copy_from: bool,
}

/// Validate a SandboxConfig against provider capabilities.
///
/// Returns all violations at once rather than stopping at the first,
/// so the user can fix everything in one pass.
pub fn validate_config(
    config: &SandboxConfig,
    caps: &ProviderCapabilities,
) -> Result<(), ProviderError> {
    let mut violations: Vec<String> = Vec::new();

    // writable field
    if config.writable {
        if caps.writable_true == FieldSupport::Unsupported {
            violations.push(format!(
                "{} does not support writable filesystem",
                caps.name
            ));
        }
    } else if caps.writable_false == FieldSupport::Unsupported {
        violations.push(format!(
            "{} does not support read-only filesystem (set writable=true)",
            caps.name
        ));
    }

    // mounts
    if !config.mounts.is_empty() && caps.mounts == FieldSupport::Unsupported {
        violations.push(format!("{} does not support volume mounts", caps.name));
    }

    // memory
    if config.memory.is_some() && caps.memory == FieldSupport::Unsupported {
        violations.push(format!(
            "{} does not support custom memory limits",
            caps.name
        ));
    }

    // cpus
    if config.cpus.is_some() && caps.cpus == FieldSupport::Unsupported {
        violations.push(format!("{} does not support custom CPU limits", caps.name));
    }

    // kernel (Required check)
    if caps.kernel == FieldSupport::Required && config.kernel.is_none() {
        violations.push(format!("{} requires --kernel path", caps.name));
    }

    // rootfs (Required check)
    if caps.rootfs == FieldSupport::Required && config.rootfs.is_none() {
        violations.push(format!("{} requires --rootfs path", caps.name));
    }

    // network_allowlist
    if !config.network_allowlist.is_empty() && caps.network_allowlist == FieldSupport::Unsupported {
        violations.push(format!(
            "{} does not support network allowlist",
            caps.name
        ));
    }

    // fs_paths
    if !config.fs_paths.is_empty() && caps.fs_paths == FieldSupport::Unsupported {
        violations.push(format!(
            "{} does not support filesystem path whitelist",
            caps.name
        ));
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(ProviderError::Unsupported(violations.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MountConfig, SandboxConfig};

    fn docker_caps() -> ProviderCapabilities {
        ProviderCapabilities {
            name: "docker".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Supported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Supported,
            memory: FieldSupport::Supported,
            cpus: FieldSupport::Supported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
            network_allowlist: FieldSupport::Supported,
            fs_paths: FieldSupport::Supported,
        }
    }

    fn e2b_caps() -> ProviderCapabilities {
        ProviderCapabilities {
            name: "e2b".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Unsupported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Unsupported,
            cpus: FieldSupport::Unsupported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
            network_allowlist: FieldSupport::Unsupported,
            fs_paths: FieldSupport::Unsupported,
        }
    }

    fn firecracker_caps() -> ProviderCapabilities {
        ProviderCapabilities {
            name: "firecracker".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Supported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::NotApplicable,
            memory: FieldSupport::Supported,
            cpus: FieldSupport::Supported,
            kernel: FieldSupport::Required,
            rootfs: FieldSupport::Required,
            pause: true,
            unpause: true,
            copy_to: false,
            copy_from: false,
            network_allowlist: FieldSupport::NotApplicable,
            fs_paths: FieldSupport::NotApplicable,
        }
    }

    fn k8s_caps() -> ProviderCapabilities {
        ProviderCapabilities {
            name: "k8s".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Supported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Supported,
            cpus: FieldSupport::Supported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: false,
            unpause: false,
            copy_to: true,
            copy_from: true,
            network_allowlist: FieldSupport::NotApplicable,
            fs_paths: FieldSupport::NotApplicable,
        }
    }

    fn wasm_caps() -> ProviderCapabilities {
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

    // --- Docker accepts everything ---

    #[test]
    fn test_docker_accepts_defaults() {
        let config = SandboxConfig::default();
        assert!(validate_config(&config, &docker_caps()).is_ok());
    }

    #[test]
    fn test_docker_accepts_all_fields() {
        let config = SandboxConfig {
            writable: true,
            network: true,
            memory: Some("512m".into()),
            cpus: Some(2.0),
            mounts: vec![MountConfig {
                host_path: "/tmp".into(),
                container_path: "/data".into(),
                readonly: true,
            }],
            ..Default::default()
        };
        assert!(validate_config(&config, &docker_caps()).is_ok());
    }

    // --- E2B rejects defaults ---

    #[test]
    fn test_e2b_rejects_default_config() {
        let config = SandboxConfig::default();
        let err = validate_config(&config, &e2b_caps()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("read-only"), "expected read-only error, got: {msg}");
    }

    #[test]
    fn test_e2b_rejects_multiple_violations() {
        let config = SandboxConfig {
            writable: false,
            memory: Some("512m".into()),
            mounts: vec![MountConfig {
                host_path: "/tmp".into(),
                container_path: "/data".into(),
                readonly: true,
            }],
            ..Default::default()
        };
        let err = validate_config(&config, &e2b_caps()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("read-only"), "missing read-only: {msg}");
        assert!(msg.contains("volume mounts"), "missing mounts: {msg}");
        assert!(msg.contains("memory"), "missing memory: {msg}");
    }

    #[test]
    fn test_e2b_accepts_writable_true() {
        let config = SandboxConfig {
            writable: true,
            ..Default::default()
        };
        assert!(validate_config(&config, &e2b_caps()).is_ok());
    }

    // --- Firecracker requires kernel/rootfs ---

    #[test]
    fn test_firecracker_rejects_missing_kernel_rootfs() {
        let config = SandboxConfig::default();
        let err = validate_config(&config, &firecracker_caps()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--kernel"), "missing kernel: {msg}");
        assert!(msg.contains("--rootfs"), "missing rootfs: {msg}");
    }

    #[test]
    fn test_firecracker_accepts_with_kernel_rootfs() {
        let config = SandboxConfig {
            kernel: Some("/boot/vmlinux".into()),
            rootfs: Some("/images/rootfs.ext4".into()),
            ..Default::default()
        };
        assert!(validate_config(&config, &firecracker_caps()).is_ok());
    }

    // --- K8s rejects mounts ---

    #[test]
    fn test_k8s_rejects_mounts() {
        let config = SandboxConfig {
            mounts: vec![MountConfig {
                host_path: "/tmp".into(),
                container_path: "/data".into(),
                readonly: true,
            }],
            ..Default::default()
        };
        let err = validate_config(&config, &k8s_caps()).unwrap_err();
        assert!(err.to_string().contains("volume mounts"));
    }

    #[test]
    fn test_k8s_accepts_defaults() {
        let config = SandboxConfig::default();
        assert!(validate_config(&config, &k8s_caps()).is_ok());
    }

    // --- WASM ignores network/memory/cpus ---

    #[test]
    fn test_wasm_accepts_with_irrelevant_fields() {
        let config = SandboxConfig {
            network: true,
            ..Default::default()
        };
        // network is NotApplicable for WASM, should still pass
        assert!(validate_config(&config, &wasm_caps()).is_ok());
    }

    #[test]
    fn test_wasm_accepts_defaults() {
        let config = SandboxConfig::default();
        assert!(validate_config(&config, &wasm_caps()).is_ok());
    }

    // --- Serialization ---

    #[test]
    fn test_capabilities_serde_roundtrip() {
        let caps = docker_caps();
        let json = serde_json::to_string(&caps).unwrap();
        let parsed: ProviderCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "docker");
        assert_eq!(parsed.writable_true, FieldSupport::Supported);
        assert_eq!(parsed.pause, true);
    }
}
