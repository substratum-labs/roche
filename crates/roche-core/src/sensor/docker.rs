// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::provider::ProviderError;
use crate::types::SandboxId;

use super::types::{ExecutionTrace, ResourceUsage, TraceLevel};
use super::SandboxSensor;

/// Docker-based sensor that collects execution traces using Docker CLI introspection.
pub struct DockerSensor;

/// Collects trace data for a running Docker container execution.
pub struct DockerTraceCollector {
    pub(crate) level: TraceLevel,
    pub(crate) container_id: String,
    pub(crate) start_time: std::time::Instant,
}

impl SandboxSensor for DockerSensor {
    async fn start_trace(
        &self,
        id: &SandboxId,
        level: TraceLevel,
    ) -> Result<super::TraceCollectorHandle, ProviderError> {
        let collector = DockerTraceCollector::start(id.clone(), level);
        Ok(super::TraceCollectorHandle::Docker(collector))
    }
}

impl DockerTraceCollector {
    /// Create a new trace collector for the given container.
    pub fn start(container_id: String, level: TraceLevel) -> Self {
        Self {
            level,
            container_id,
            start_time: std::time::Instant::now(),
        }
    }

    /// Finish collecting the trace and return the result.
    pub async fn finish(self) -> Result<ExecutionTrace, ProviderError> {
        let duration_secs = self.start_time.elapsed().as_secs_f64();

        if self.level == TraceLevel::Off {
            return Ok(ExecutionTrace {
                duration_secs,
                resource_usage: ResourceUsage::default(),
                file_accesses: Vec::new(),
                network_attempts: Vec::new(),
                blocked_ops: Vec::new(),
                syscalls: Vec::new(),
                resource_timeline: Vec::new(),
            });
        }

        // Summary level: collect resource usage via docker stats
        let resource_usage = if self.level >= TraceLevel::Summary {
            self.collect_resource_usage().await.unwrap_or_default()
        } else {
            ResourceUsage::default()
        };

        // Standard+: collect file changes via docker diff
        let file_accesses = if self.level >= TraceLevel::Standard {
            self.collect_file_accesses().await.unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(ExecutionTrace {
            duration_secs,
            resource_usage,
            file_accesses,
            network_attempts: Vec::new(),
            blocked_ops: Vec::new(),
            syscalls: Vec::new(),
            resource_timeline: Vec::new(),
        })
    }

    /// Abort tracing without collecting results (MVP: no-op).
    pub async fn abort(self) {
        // Nothing to clean up in the MVP.
    }

    /// Collect resource usage via `docker stats --no-stream`.
    async fn collect_resource_usage(&self) -> Result<ResourceUsage, ProviderError> {
        let output = tokio::process::Command::new("docker")
            .args([
                "stats",
                "--no-stream",
                "--format",
                "{{.MemUsage}}\t{{.NetIO}}",
                &self.container_id,
            ])
            .output()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("docker stats failed: {e}")))?;

        if !output.status.success() {
            return Ok(ResourceUsage::default());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();
        let parts: Vec<&str> = line.split('\t').collect();

        let peak_memory_bytes = parts
            .first()
            .and_then(|s| {
                // Format: "123.4MiB / 7.773GiB" — take the first part
                s.split('/').next().map(|v| parse_memory_bytes(v.trim()))
            })
            .unwrap_or(0);

        let (network_rx_bytes, network_tx_bytes) = parts
            .get(1)
            .map(|s| {
                // Format: "1.23kB / 4.56kB"
                let io_parts: Vec<&str> = s.split('/').collect();
                let rx = io_parts
                    .first()
                    .map(|v| parse_net_rx(v.trim()))
                    .unwrap_or(0);
                let tx = io_parts.get(1).map(|v| parse_net_tx(v.trim())).unwrap_or(0);
                (rx, tx)
            })
            .unwrap_or((0, 0));

        Ok(ResourceUsage {
            peak_memory_bytes,
            cpu_time_secs: 0.0,
            network_rx_bytes,
            network_tx_bytes,
        })
    }

    /// Collect file accesses via `docker diff`.
    async fn collect_file_accesses(&self) -> Result<Vec<super::types::FileAccess>, ProviderError> {
        let output = tokio::process::Command::new("docker")
            .args(["diff", &self.container_id])
            .output()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("docker diff failed: {e}")))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let accesses = parse_docker_diff(&stdout);
        Ok(accesses)
    }
}

/// Parse a memory string like "123.4MiB" or "1.5GiB" into bytes.
pub fn parse_memory_bytes(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }

    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("GiB") {
        (n, 1024u64 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("MiB") {
        (n, 1024u64 * 1024)
    } else if let Some(n) = s.strip_suffix("KiB") {
        (n, 1024u64)
    } else if let Some(n) = s.strip_suffix("GB") {
        (n, 1_000_000_000u64)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1_000_000u64)
    } else if let Some(n) = s.strip_suffix("kB") {
        (n, 1_000u64)
    } else if let Some(n) = s.strip_suffix('B') {
        (n, 1u64)
    } else {
        (s, 1u64)
    };

    num_str
        .trim()
        .parse::<f64>()
        .map(|v| (v * multiplier as f64) as u64)
        .unwrap_or(0)
}

/// Parse a network size string (e.g. "1.23kB") into bytes.
pub fn parse_net_rx(s: &str) -> u64 {
    parse_size_bytes(s)
}

/// Parse a network size string (e.g. "4.56kB") into bytes.
pub fn parse_net_tx(s: &str) -> u64 {
    parse_size_bytes(s)
}

/// Parse a size string like "1.23kB", "4.56MB", "0B" into bytes.
pub fn parse_size_bytes(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }

    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("GB") {
        (n, 1_000_000_000u64)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1_000_000u64)
    } else if let Some(n) = s.strip_suffix("kB") {
        (n, 1_000u64)
    } else if let Some(n) = s.strip_suffix('B') {
        (n, 1u64)
    } else {
        (s, 1u64)
    };

    num_str
        .trim()
        .parse::<f64>()
        .map(|v| (v * multiplier as f64) as u64)
        .unwrap_or(0)
}

/// Parse `docker diff` output into FileAccess entries.
///
/// Each line has format: `<change_type> <path>`
/// where change_type is: A (added), C (changed), D (deleted).
pub fn parse_docker_diff(output: &str) -> Vec<super::types::FileAccess> {
    use super::types::{FileAccess, FileOp};

    output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let line = line.trim();
            if line.len() < 3 {
                return None;
            }
            let change_type = &line[..1];
            let path = line[2..].to_string();
            let op = match change_type {
                "A" => FileOp::Create,
                "C" => FileOp::Write,
                "D" => FileOp::Delete,
                _ => return None,
            };
            Some(FileAccess {
                path,
                op,
                size_bytes: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::types::FileOp;

    #[test]
    fn test_parse_memory_bytes() {
        assert_eq!(parse_memory_bytes("123.4MiB"), 129_394_278);
        assert_eq!(parse_memory_bytes("1.5GiB"), 1_610_612_736);
        assert_eq!(parse_memory_bytes("512KiB"), 524_288);
        assert_eq!(parse_memory_bytes("100B"), 100);
        assert_eq!(parse_memory_bytes(""), 0);
        assert_eq!(parse_memory_bytes("1.5GB"), 1_500_000_000);
        assert_eq!(parse_memory_bytes("100MB"), 100_000_000);
    }

    #[test]
    fn test_parse_size_bytes() {
        assert_eq!(parse_size_bytes("1.23kB"), 1230);
        assert_eq!(parse_size_bytes("4.56MB"), 4_560_000);
        assert_eq!(parse_size_bytes("0B"), 0);
        assert_eq!(parse_size_bytes(""), 0);
        assert_eq!(parse_size_bytes("1GB"), 1_000_000_000);
    }

    #[test]
    fn test_parse_net_rx_tx() {
        assert_eq!(parse_net_rx("1.23kB"), 1230);
        assert_eq!(parse_net_tx("4.56kB"), 4560);
    }

    #[test]
    fn test_parse_docker_diff() {
        let output = "A /tmp/newfile\nC /etc/hosts\nD /var/log/old.log\n";
        let accesses = parse_docker_diff(output);
        assert_eq!(accesses.len(), 3);
        assert_eq!(accesses[0].path, "/tmp/newfile");
        assert_eq!(accesses[0].op, FileOp::Create);
        assert_eq!(accesses[1].path, "/etc/hosts");
        assert_eq!(accesses[1].op, FileOp::Write);
        assert_eq!(accesses[2].path, "/var/log/old.log");
        assert_eq!(accesses[2].op, FileOp::Delete);
    }

    #[test]
    fn test_parse_docker_diff_empty() {
        let accesses = parse_docker_diff("");
        assert!(accesses.is_empty());
    }
}
