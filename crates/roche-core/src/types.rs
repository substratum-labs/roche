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
}

fn default_image() -> String {
    "python:3.12-slim".to_string()
}

fn default_timeout() -> u64 {
    300
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
        }
    }
}

/// Runtime status of a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SandboxStatus {
    Running,
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
}

/// Request to execute a command inside a sandbox.
#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub command: Vec<String>,
    pub timeout_secs: Option<u64>,
}

/// Output from executing a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}
