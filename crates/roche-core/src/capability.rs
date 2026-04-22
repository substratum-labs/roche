// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

//! SandboxGrant — the agent sandbox specification.
//!
//! A SandboxGrant declares what an agent is allowed to do inside a sandbox.
//! It is Roche's equivalent of the OCI runtime spec, but at the capability
//! level rather than the resource level.
//!
//! Docker isolates processes (namespaces + cgroups = resource boundaries).
//! Roche isolates agents (capability wallet = what the agent can *do*).
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │              Capability Wallet                │
//! │                                              │
//! │  network:                                     │
//! │    allowed_hosts: [api.openai.com, pypi.org] │
//! │    max_egress_bytes: 10MB                    │
//! │                                              │
//! │  filesystem:                                  │
//! │    writable_paths: [/tmp, /output]           │
//! │    max_write_bytes: 100MB                    │
//! │                                              │
//! │  compute:                                     │
//! │    max_exec_count: 50                        │
//! │    max_duration_secs: 300                    │
//! │    max_memory_bytes: 512MB                   │
//! │                                              │
//! │  secrets:                                     │
//! │    allowed_env_keys: [OPENAI_API_KEY]        │
//! │                                              │
//! │  output:                                      │
//! │    max_stdout_bytes: 10MB                    │
//! │    max_stderr_bytes: 1MB                     │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! The wallet is created by the orchestration layer (Castor/Orrery) and
//! passed to Roche. Roche enforces it during execution and returns a
//! UsageReport showing what was actually consumed.

use serde::{Deserialize, Serialize};

/// A capability wallet — declares what an agent can do inside a sandbox.
///
/// Zero values mean "unlimited" for limits, or "disabled" for booleans.
/// This follows the principle of least privilege: everything is off by default,
/// explicitly enabled capabilities are the only things allowed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxGrant {
    /// Network capabilities.
    #[serde(default)]
    pub network: NetworkCapability,

    /// Filesystem capabilities.
    #[serde(default)]
    pub filesystem: FilesystemCapability,

    /// Compute budget.
    #[serde(default)]
    pub compute: ComputeCapability,

    /// Secret/environment variable access.
    #[serde(default)]
    pub secrets: SecretsCapability,

    /// Output limits.
    #[serde(default)]
    pub output: OutputCapability,

    /// Provider preference (empty = auto-select).
    #[serde(default)]
    pub provider: String,

    /// Container image override (empty = auto-select from language).
    #[serde(default)]
    pub image: String,

    /// Arbitrary metadata (agent PID, session ID, etc.)
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Network access capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkCapability {
    /// Whether network is enabled at all.
    #[serde(default)]
    pub enabled: bool,

    /// Allowed outbound hosts. Empty + enabled = unrestricted.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,

    /// Max outbound bytes. 0 = unlimited.
    #[serde(default)]
    pub max_egress_bytes: u64,
}

/// Filesystem write capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilesystemCapability {
    /// Whether writes are enabled at all.
    #[serde(default)]
    pub writable: bool,

    /// Paths where writes are allowed. Empty + writable = everywhere.
    #[serde(default)]
    pub writable_paths: Vec<String>,

    /// Max total write bytes. 0 = unlimited.
    #[serde(default)]
    pub max_write_bytes: u64,
}

/// Compute budget.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComputeCapability {
    /// Max number of exec calls. 0 = unlimited.
    #[serde(default)]
    pub max_exec_count: u32,

    /// Max total execution time in seconds. 0 = unlimited.
    #[serde(default)]
    pub max_duration_secs: u64,

    /// Max memory in bytes. 0 = provider default.
    #[serde(default)]
    pub max_memory_bytes: u64,

    /// Max CPUs. 0 = provider default.
    #[serde(default)]
    pub max_cpus: f64,
}

/// Secret/environment variable access.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretsCapability {
    /// Env var keys the agent is allowed to access.
    /// Empty = no secrets. The values are NOT stored here —
    /// the runtime resolves them from the host environment.
    #[serde(default)]
    pub allowed_env_keys: Vec<String>,
}

/// Output limits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputCapability {
    /// Max stdout bytes per exec. 0 = unlimited.
    #[serde(default)]
    pub max_stdout_bytes: u64,

    /// Max stderr bytes per exec. 0 = unlimited.
    #[serde(default)]
    pub max_stderr_bytes: u64,
}

// ---------------------------------------------------------------------------
// Usage Report — what actually happened
// ---------------------------------------------------------------------------

/// Usage report returned after execution. Castor uses this to update its budgets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageReport {
    /// Number of exec calls made.
    pub exec_count: u32,

    /// Total execution time in seconds.
    pub duration_secs: f64,

    /// Total stdout bytes produced.
    pub stdout_bytes: u64,

    /// Total stderr bytes produced.
    pub stderr_bytes: u64,

    /// Network hosts actually contacted.
    pub network_hosts_contacted: Vec<String>,

    /// Network bytes sent.
    pub network_egress_bytes: u64,

    /// Filesystem bytes written.
    pub fs_write_bytes: u64,

    /// Filesystem paths written to.
    pub fs_paths_written: Vec<String>,

    /// Peak memory usage in bytes.
    pub peak_memory_bytes: u64,

    /// Violations: attempted actions that were blocked.
    pub violations: Vec<String>,
}

// ---------------------------------------------------------------------------
// Conversions: Wallet ↔ existing types
// ---------------------------------------------------------------------------

impl SandboxGrant {
    /// Convert to SandboxConfig for creating a sandbox.
    pub fn to_sandbox_config(&self) -> crate::types::SandboxConfig {
        use crate::types::SandboxConfig;

        let memory = if self.compute.max_memory_bytes > 0 {
            let mb = self.compute.max_memory_bytes / (1024 * 1024);
            Some(format!("{mb}m"))
        } else {
            None
        };

        let cpus = if self.compute.max_cpus > 0.0 {
            Some(self.compute.max_cpus)
        } else {
            None
        };

        // Filter env to only allowed keys
        let env: std::collections::HashMap<String, String> = self
            .secrets
            .allowed_env_keys
            .iter()
            .filter_map(|key| {
                std::env::var(key).ok().map(|val| (key.clone(), val))
            })
            .collect();

        SandboxConfig {
            provider: if self.provider.is_empty() {
                "docker".to_string()
            } else {
                self.provider.clone()
            },
            image: if self.image.is_empty() {
                "python:3.12-slim".to_string()
            } else {
                self.image.clone()
            },
            memory,
            cpus,
            timeout_secs: if self.compute.max_duration_secs > 0 {
                self.compute.max_duration_secs
            } else {
                300
            },
            network: self.network.enabled,
            writable: self.filesystem.writable,
            env,
            mounts: Vec::new(),
            kernel: None,
            rootfs: None,
            trace_enabled: true,
            network_allowlist: self.network.allowed_hosts.clone(),
            fs_paths: self.filesystem.writable_paths.clone(),
        }
    }

    /// Convert to Session Budget for budget tracking.
    pub fn to_session_budget(&self) -> crate::session::Budget {
        crate::session::Budget {
            max_execs: self.compute.max_exec_count,
            max_total_secs: self.compute.max_duration_secs,
            max_output_bytes: self.output.max_stdout_bytes + self.output.max_stderr_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_wallet_is_fully_locked() {
        let w = SandboxGrant::default();
        assert!(!w.network.enabled);
        assert!(!w.filesystem.writable);
        assert_eq!(w.compute.max_exec_count, 0); // unlimited
        assert!(w.secrets.allowed_env_keys.is_empty());
    }

    #[test]
    fn test_wallet_to_sandbox_config() {
        let w = SandboxGrant {
            network: NetworkCapability {
                enabled: true,
                allowed_hosts: vec!["api.openai.com".into()],
                ..Default::default()
            },
            filesystem: FilesystemCapability {
                writable: true,
                writable_paths: vec!["/tmp".into()],
                ..Default::default()
            },
            compute: ComputeCapability {
                max_memory_bytes: 512 * 1024 * 1024,
                max_duration_secs: 60,
                ..Default::default()
            },
            ..Default::default()
        };

        let config = w.to_sandbox_config();
        assert!(config.network);
        assert_eq!(config.network_allowlist, vec!["api.openai.com"]);
        assert!(config.writable);
        assert_eq!(config.fs_paths, vec!["/tmp"]);
        assert_eq!(config.memory, Some("512m".into()));
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_serde_roundtrip() {
        let w = SandboxGrant {
            network: NetworkCapability {
                enabled: true,
                allowed_hosts: vec!["pypi.org".into()],
                max_egress_bytes: 10_000_000,
            },
            compute: ComputeCapability {
                max_exec_count: 50,
                max_duration_secs: 300,
                ..Default::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&w).unwrap();
        let parsed: SandboxGrant = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.network.allowed_hosts, vec!["pypi.org"]);
        assert_eq!(parsed.compute.max_exec_count, 50);
        assert_eq!(parsed.network.max_egress_bytes, 10_000_000);
    }

    #[test]
    fn test_usage_report_default() {
        let r = UsageReport::default();
        assert_eq!(r.exec_count, 0);
        assert!(r.violations.is_empty());
    }
}
