// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use serde::{Deserialize, Serialize};

/// Controls the level of detail captured in execution traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceLevel {
    Off = 0,
    Summary = 1,
    Standard = 2,
    Full = 3,
}

impl Default for TraceLevel {
    fn default() -> Self {
        Self::Standard
    }
}

impl PartialOrd for TraceLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TraceLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

impl TraceLevel {
    /// Convert from a protobuf i32 value to TraceLevel.
    pub fn from_proto(value: i32) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::Summary,
            2 => Self::Standard,
            3 => Self::Full,
            _ => Self::Standard,
        }
    }
}

/// Full execution trace captured during a sandbox command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub duration_secs: f64,
    pub resource_usage: ResourceUsage,
    #[serde(default)]
    pub file_accesses: Vec<FileAccess>,
    #[serde(default)]
    pub network_attempts: Vec<NetworkAttempt>,
    #[serde(default)]
    pub blocked_ops: Vec<BlockedOperation>,
    #[serde(default)]
    pub syscalls: Vec<SyscallEvent>,
    #[serde(default)]
    pub resource_timeline: Vec<ResourceSnapshot>,
}

/// Aggregate resource usage during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub peak_memory_bytes: u64,
    pub cpu_time_secs: f64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            peak_memory_bytes: 0,
            cpu_time_secs: 0.0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
        }
    }
}

/// Type of file operation observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOp {
    Read,
    Write,
    Create,
    Delete,
}

/// A single file access event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    pub path: String,
    pub op: FileOp,
    pub size_bytes: Option<u64>,
}

/// A network connection attempt observed during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAttempt {
    pub address: String,
    pub protocol: String,
    pub allowed: bool,
}

/// An operation that was blocked by sandbox policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedOperation {
    pub op_type: String,
    pub detail: String,
}

/// A single syscall event captured during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallEvent {
    pub name: String,
    pub args: Vec<String>,
    pub result: String,
    pub timestamp_ms: u64,
}

/// A point-in-time resource usage snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub timestamp_ms: u64,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
}
