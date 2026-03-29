// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Opaque sandbox identifier.
pub type SandboxId = String;

/// Configuration for creating a new sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Provider to use (e.g. "docker").
    pub provider: String,

    /// Container image (provider-specific).
    #[serde(default = "default_image")]
    pub image: String,

    /// Memory limit (e.g. "512m").
    pub memory: Option<String>,

    /// CPU limit (e.g. "1.0" = 1 core).
    pub cpus: Option<f64>,

    /// Timeout in seconds. Default: 300.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Enable network access. Default: false (AI-safe).
    #[serde(default)]
    pub network: bool,

    /// Writable filesystem. Default: false (AI-safe).
    #[serde(default)]
    pub writable: bool,

    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Volume mounts.
    #[serde(default)]
    pub mounts: Vec<MountConfig>,

    /// Path to uncompressed Linux kernel (Firecracker only).
    #[serde(default)]
    pub kernel: Option<String>,

    /// Path to ext4 rootfs image (Firecracker only).
    #[serde(default)]
    pub rootfs: Option<String>,

    /// Enable execution tracing. Default: true.
    #[serde(default = "default_true")]
    pub trace_enabled: bool,

    /// Network allowlist: when `network` is true, restrict to these hosts.
    /// Empty = unrestricted. E.g., ["api.openai.com", "cdn.example.com"]
    #[serde(default)]
    pub network_allowlist: Vec<String>,

    /// Filesystem path whitelist: writable paths when filesystem is otherwise read-only.
    /// Empty = default behavior (writable flag controls everything).
    /// E.g., ["/data", "/tmp"]
    #[serde(default)]
    pub fs_paths: Vec<String>,
}

/// Configuration for a volume mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub host_path: String,
    pub container_path: String,
    /// Default: true (readonly, AI-safe).
    pub readonly: bool,
}

fn default_image() -> String {
    "python:3.12-slim".to_string()
}

fn default_timeout() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            provider: "docker".to_string(),
            image: default_image(),
            memory: None,
            cpus: None,
            timeout_secs: default_timeout(),
            network: false,
            writable: false,
            env: HashMap::new(),
            mounts: Vec::new(),
            kernel: None,
            rootfs: None,
            trace_enabled: true,
            network_allowlist: Vec::new(),
            fs_paths: Vec::new(),
        }
    }
}

/// Runtime status of a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SandboxStatus {
    Running,
    Paused,
    Stopped,
    Failed,
}

/// Metadata about an active sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInfo {
    pub id: SandboxId,
    pub status: SandboxStatus,
    pub provider: String,
    pub image: String,
    pub expires_at: Option<u64>,
}

/// Request to execute a command inside a sandbox.
#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub command: Vec<String>,
    pub timeout_secs: Option<u64>,
    /// Optional idempotency key. If set, duplicate execs with the same key
    /// return the cached result instead of re-executing.
    pub idempotency_key: Option<String>,
}

/// Output from executing a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub trace: Option<crate::sensor::ExecutionTrace>,
}

/// A single event in a streaming exec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExecEvent {
    /// A chunk of stdout or stderr output.
    Output {
        stream: String,
        data: Vec<u8>,
    },
    /// Periodic heartbeat with resource snapshot.
    Heartbeat {
        elapsed_ms: u64,
        memory_bytes: u64,
        cpu_percent: f32,
    },
    /// Final result (last event in the stream).
    Result {
        exit_code: i32,
        #[serde(default)]
        trace: Option<crate::sensor::ExecutionTrace>,
    },
}

/// Declarative retry policy for exec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Total attempts (1 = no retry). Default: 1.
    #[serde(default = "default_one")]
    pub max_attempts: u32,
    /// Backoff strategy: "none", "linear", "exponential". Default: "none".
    #[serde(default)]
    pub backoff: String,
    /// Initial retry delay in ms. Default: 1000.
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    /// Conditions that trigger a retry: "timeout", "oom", "nonzero_exit".
    /// Empty = retry on any error.
    #[serde(default)]
    pub retry_on: Vec<String>,
}

fn default_one() -> u32 { 1 }
fn default_initial_delay() -> u64 { 1000 }

/// Output size limits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputLimit {
    /// Max combined stdout+stderr bytes. 0 = unlimited.
    #[serde(default)]
    pub max_bytes: u64,
    /// Action when limit exceeded: "truncate" or "error". Default: "truncate".
    #[serde(default)]
    pub action: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default_has_no_kernel_rootfs() {
        let config = SandboxConfig::default();
        assert!(config.kernel.is_none());
        assert!(config.rootfs.is_none());
    }

    #[test]
    fn test_sandbox_config_with_kernel_rootfs() {
        let config = SandboxConfig {
            kernel: Some("/path/to/vmlinux".to_string()),
            rootfs: Some("/path/to/rootfs.ext4".to_string()),
            ..Default::default()
        };
        assert_eq!(config.kernel.as_deref(), Some("/path/to/vmlinux"));
        assert_eq!(config.rootfs.as_deref(), Some("/path/to/rootfs.ext4"));
    }

    #[test]
    fn test_sandbox_config_serde_roundtrip_with_kernel() {
        let config = SandboxConfig {
            provider: "firecracker".to_string(),
            kernel: Some("/boot/vmlinux".to_string()),
            rootfs: Some("/images/rootfs.ext4".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kernel.as_deref(), Some("/boot/vmlinux"));
        assert_eq!(parsed.rootfs.as_deref(), Some("/images/rootfs.ext4"));
    }
}
