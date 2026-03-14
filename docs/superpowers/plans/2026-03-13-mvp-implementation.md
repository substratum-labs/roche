# Roche MVP Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Docker provider, wire CLI subcommands, build the Python SDK, and add tests — taking Roche from compiling stubs to a fully working MVP.

**Architecture:** DockerProvider shells out to `docker` CLI via `tokio::process::Command`. CLI matches on `--provider` to instantiate the provider, calls trait methods, and formats output. Python SDK wraps the `roche` binary via `subprocess`. Each layer is tested independently: unit tests for arg building and output parsing, integration tests for end-to-end Docker operations.

**Tech Stack:** Rust 2021, tokio, clap 4, serde/serde_json, thiserror, Docker CLI, Python 3.10+, pytest

---

## File Structure

```
crates/roche-core/
├── Cargo.toml                          # Add: tokio "time" feature
└── src/
    ├── lib.rs                          # No changes
    ├── types.rs                        # No changes
    └── provider/
        ├── mod.rs                      # No changes
        └── docker.rs                   # REWRITE: full DockerProvider implementation

crates/roche-cli/
├── Cargo.toml                          # No changes
└── src/
    └── main.rs                         # REWRITE: wire CLI to DockerProvider

crates/roche-core/
└── tests/
    └── docker_integration.rs          # CREATE: end-to-end Docker provider tests

sdk/python/
├── pyproject.toml                      # Modify: add pytest dev dependency
├── roche/
│   ├── __init__.py                     # Modify: re-export public API
│   ├── client.py                       # CREATE: Roche client class
│   ├── types.py                        # CREATE: SandboxConfig, ExecOutput dataclasses
│   └── errors.py                       # CREATE: RocheError exception
└── tests/
    └── test_client.py                  # CREATE: SDK unit tests
```

---

## Chunk 1: DockerProvider Core Implementation

### Task 1: Add tokio "time" feature to roche-core

**Files:**
- Modify: `crates/roche-core/Cargo.toml:13`

- [ ] **Step 1: Add "time" feature to tokio dependency**

In `crates/roche-core/Cargo.toml`, change line 13 from:
```toml
tokio = { version = "1", features = ["process", "rt-multi-thread", "macros"] }
```
to:
```toml
tokio = { version = "1", features = ["process", "rt-multi-thread", "macros", "time"] }
```

- [ ] **Step 2: Verify it compiles**

Run: `cd ~/roche && cargo check --workspace`
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/Cargo.toml
git commit -m "build: add tokio time feature for timeout support"
```

---

### Task 2: Implement DockerProvider::create

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Replace the entire contents of `docker.rs`**

Replace the entire contents of `docker.rs` with the implementation below. This includes `build_create_args`, `create()`, stubs for the remaining methods, and unit tests:

```rust
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
```

- [ ] **Step 2: Run unit tests to verify they pass**

Run: `cd ~/roche && cargo test -p roche-core -- tests::test_build`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "feat: implement DockerProvider::create with arg builder and unit tests"
```

---

### Task 3: Implement DockerProvider::exec with timeout

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Write unit test for exec arg building**

Add to the `tests` module in `docker.rs`:

```rust
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
```

- [ ] **Step 2: Implement `build_exec_args` and the `exec` method**

Add the `build_exec_args` function after `build_create_args`:

```rust
/// Build the argument list for `docker exec`.
fn build_exec_args(id: &SandboxId, request: &ExecRequest) -> Vec<String> {
    let mut args = vec!["exec".to_string(), id.clone()];
    args.extend(request.command.clone());
    args
}
```

Replace the `exec` stub in the `SandboxProvider` impl:

```rust
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
                Ok(ExecOutput {
                    exit_code,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            }
            Ok(Err(e)) => Err(ProviderError::ExecFailed(e.to_string())),
            Err(_) => Err(ProviderError::Timeout(timeout_secs)),
        }
    }
```

- [ ] **Step 3: Run tests**

Run: `cd ~/roche && cargo test -p roche-core`
Expected: 4 tests pass (3 create + 1 exec).

- [ ] **Step 4: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "feat: implement DockerProvider::exec with timeout support"
```

---

### Task 4: Implement DockerProvider::destroy

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Implement the `destroy` method**

Replace the `destroy` stub:

```rust
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
```

- [ ] **Step 2: Verify compilation**

Run: `cd ~/roche && cargo check -p roche-core`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "feat: implement DockerProvider::destroy"
```

---

### Task 5: Implement DockerProvider::list

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Write unit test for status parsing**

Add to the `tests` module:

```rust
#[test]
fn test_parse_status() {
    assert_eq!(parse_status("running"), SandboxStatus::Running);
    assert_eq!(parse_status("exited"), SandboxStatus::Stopped);
    assert_eq!(parse_status("created"), SandboxStatus::Stopped);
    assert_eq!(parse_status("dead"), SandboxStatus::Failed);
    assert_eq!(parse_status("anything_else"), SandboxStatus::Failed);
}
```

- [ ] **Step 2: Implement `parse_status` helper and `list` method**

Add the `parse_status` helper function (note: `SandboxStatus` is already imported in the top-level `use` statement added in Task 2):

```rust
/// Map Docker container state string to SandboxStatus.
fn parse_status(state: &str) -> SandboxStatus {
    match state {
        "running" => SandboxStatus::Running,
        "exited" | "created" => SandboxStatus::Stopped,
        _ => SandboxStatus::Failed,
    }
}
```

Replace the `list` stub:

```rust
    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter", "label=roche.managed=true",
                "--format", "{{.ID}}\t{{.State}}\t{{.Image}}",
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
                }
            })
            .collect();

        Ok(sandboxes)
    }
```

- [ ] **Step 3: Run all unit tests**

Run: `cd ~/roche && cargo test -p roche-core`
Expected: 5 tests pass (3 create args + 1 exec args + 1 parse_status).

- [ ] **Step 4: Run clippy and fmt**

Run: `cd ~/roche && cargo clippy --workspace -- -D warnings && cargo fmt --all --check`
Expected: no warnings, no format issues.

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "feat: implement DockerProvider::list with status parsing"
```

---

## Chunk 2: Wire CLI to DockerProvider

### Task 6: Wire all CLI subcommands to DockerProvider

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Add `--timeout` flag to Exec, `--json` flag to List**

Add a `timeout` field to the `Exec` variant and `json` flag to `List`:

```rust
    /// Execute a command in a sandbox
    Exec {
        /// Sandbox ID
        #[arg(long)]
        sandbox: String,

        /// Timeout override in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Command to execute
        command: Vec<String>,
    },

    /// Destroy a sandbox
    Destroy {
        /// Sandbox ID
        id: String,
    },

    /// List active sandboxes
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
```

- [ ] **Step 2: Rewrite the main function to wire everything up**

Replace the entire `main` function:

```rust
#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = run(cli).await;
    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::docker::DockerProvider;
    use roche_core::provider::SandboxProvider;
    use roche_core::types::{ExecRequest, SandboxConfig};

    let provider = DockerProvider::new();

    match cli.command {
        Commands::Create {
            provider: _provider_name,
            image,
            memory,
            cpus,
            timeout,
            network,
            writable,
        } => {
            let config = SandboxConfig {
                provider: _provider_name,
                image,
                memory,
                cpus,
                timeout_secs: timeout,
                network,
                writable,
                ..Default::default()
            };
            let id = provider.create(&config).await?;
            println!("{id}");
        }
        Commands::Exec {
            sandbox,
            timeout,
            command,
        } => {
            let request = ExecRequest {
                command,
                timeout_secs: timeout,
            };
            let output = provider.exec(&sandbox, &request).await?;
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            if output.exit_code != 0 {
                std::process::exit(output.exit_code);
            }
        }
        Commands::Destroy { id } => {
            provider.destroy(&id).await?;
        }
        Commands::List { json } => {
            let sandboxes = provider.list().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&sandboxes).unwrap());
            } else {
                if sandboxes.is_empty() {
                    println!("No active sandboxes.");
                } else {
                    println!("{:<16} {:<10} {:<10} {}", "ID", "STATUS", "PROVIDER", "IMAGE");
                    for sb in &sandboxes {
                        println!(
                            "{:<16} {:<10} {:<10} {}",
                            sb.id,
                            format!("{:?}", sb.status).to_lowercase(),
                            sb.provider,
                            sb.image,
                        );
                    }
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Verify compilation**

Run: `cd ~/roche && cargo build --workspace`
Expected: compiles successfully.

- [ ] **Step 4: Verify CLI help output**

Run: `cd ~/roche && cargo run -- --help`
Expected: shows all subcommands (create, exec, destroy, list).

Run: `cd ~/roche && cargo run -- create --help`
Expected: shows all create flags (--provider, --image, --memory, --cpus, --timeout, --network, --writable).

- [ ] **Step 5: Run clippy and fmt**

Run: `cd ~/roche && cargo clippy --workspace -- -D warnings && cargo fmt --all --check`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat: wire CLI subcommands to DockerProvider"
```

---

## Chunk 3: Integration Tests

### Task 7: Docker integration tests

These tests require a running Docker daemon. They create real containers and clean up after themselves.

**Files:**
- Create: `crates/roche-core/tests/docker_integration.rs`

- [ ] **Step 1: Write integration tests**

Create `crates/roche-core/tests/docker_integration.rs`:

```rust
//! Integration tests for DockerProvider.
//! Requires Docker daemon running.

use roche_core::provider::docker::DockerProvider;
use roche_core::provider::SandboxProvider;
use roche_core::types::{ExecRequest, SandboxConfig, SandboxStatus};

/// Helper: create a sandbox with defaults, return its ID.
/// Caller is responsible for cleanup.
async fn create_default_sandbox(provider: &DockerProvider) -> String {
    let config = SandboxConfig::default();
    provider
        .create(&config)
        .await
        .expect("failed to create sandbox")
}

#[tokio::test]
async fn test_create_and_destroy() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;
    assert!(!id.is_empty());
    assert!(id.len() == 12, "ID should be 12 hex chars, got: {id}");

    provider.destroy(&id).await.expect("failed to destroy");
}

#[tokio::test]
async fn test_exec_simple_command() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let request = ExecRequest {
        command: vec!["echo".into(), "hello roche".into()],
        timeout_secs: Some(30),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.trim(), "hello roche");
    assert!(output.stderr.is_empty());

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_exec_python() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let request = ExecRequest {
        command: vec![
            "python3".into(),
            "-c".into(),
            "print(2 + 2)".into(),
        ],
        timeout_secs: Some(30),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.trim(), "4");

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_exec_nonzero_exit() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let request = ExecRequest {
        command: vec!["sh".into(), "-c".into(), "exit 42".into()],
        timeout_secs: Some(30),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");

    assert_eq!(output.exit_code, 42);

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_list_includes_created_sandbox() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let sandboxes = provider.list().await.expect("list failed");
    let found = sandboxes.iter().any(|s| s.id == id);
    assert!(found, "Created sandbox {id} should appear in list");

    let sb = sandboxes.iter().find(|s| s.id == id).unwrap();
    assert_eq!(sb.status, SandboxStatus::Running);
    assert_eq!(sb.provider, "docker");

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_destroy_nonexistent_returns_not_found() {
    let provider = DockerProvider::new();
    let result = provider.destroy(&"nonexistent12".to_string()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_network_disabled_by_default() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    // Attempting to reach the network should fail
    let request = ExecRequest {
        command: vec![
            "python3".into(),
            "-c".into(),
            "import urllib.request; urllib.request.urlopen('http://1.1.1.1', timeout=3)".into(),
        ],
        timeout_secs: Some(10),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");
    assert_ne!(output.exit_code, 0, "Network should be disabled by default");

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
async fn test_readonly_fs_by_default() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    // Test that root filesystem is read-only.
    // Note: /tmp might be tmpfs, so we write to / directly.
    let request2 = ExecRequest {
        command: vec![
            "sh".into(),
            "-c".into(),
            "touch /test_readonly 2>&1".into(),
        ],
        timeout_secs: Some(10),
    };
    let output2 = provider.exec(&id, &request2).await.expect("exec failed");
    assert_ne!(output2.exit_code, 0, "Root FS should be read-only");

    provider.destroy(&id).await.unwrap();
}
```

- [ ] **Step 2: Run integration tests**

Run: `cd ~/roche && cargo test -p roche-core --test docker_integration -- --test-threads=1`
Expected: all tests pass (may take 30-60s due to Docker operations).

Note: `--test-threads=1` to avoid Docker resource contention.

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/tests/docker_integration.rs
git commit -m "test: add Docker provider integration tests"
```

---

## Chunk 4: Python SDK

### Task 8: Create Python SDK types and errors

**Files:**
- Create: `sdk/python/roche/types.py`
- Create: `sdk/python/roche/errors.py`

- [ ] **Step 1: Create types module**

Create `sdk/python/roche/types.py`:

```python
"""Core data types for the Roche Python SDK."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class SandboxConfig:
    """Configuration for creating a new sandbox."""

    provider: str = "docker"
    image: str = "python:3.12-slim"
    memory: str | None = None
    cpus: float | None = None
    timeout: int = 300
    network: bool = False
    writable: bool = False
    env: dict[str, str] = field(default_factory=dict)


@dataclass
class ExecOutput:
    """Output from executing a command in a sandbox."""

    exit_code: int
    stdout: str
    stderr: str
```

- [ ] **Step 2: Create errors module**

Create `sdk/python/roche/errors.py`:

```python
"""Exception types for the Roche Python SDK."""


class RocheError(Exception):
    """Base exception for Roche operations."""

    def __init__(self, message: str, stderr: str = ""):
        super().__init__(message)
        self.stderr = stderr
```

- [ ] **Step 3: Commit**

```bash
git add sdk/python/roche/types.py sdk/python/roche/errors.py
git commit -m "feat(sdk): add Python SDK types and errors"
```

---

### Task 9: Implement Python SDK client

**Files:**
- Create: `sdk/python/roche/client.py`
- Modify: `sdk/python/roche/__init__.py`

- [ ] **Step 1: Create client module**

Create `sdk/python/roche/client.py`:

```python
"""Roche Python SDK client — wraps the roche CLI binary."""

from __future__ import annotations

import json
import subprocess
from typing import Any

from .errors import RocheError
from .types import ExecOutput, SandboxConfig


class Roche:
    """Client for the Roche sandbox orchestrator.

    Wraps the `roche` CLI binary via subprocess calls.
    """

    def __init__(self, binary: str = "roche"):
        self._binary = binary

    def _run(self, args: list[str], check: bool = True) -> subprocess.CompletedProcess[str]:
        try:
            return subprocess.run(
                [self._binary, *args],
                capture_output=True,
                text=True,
                check=check,
            )
        except FileNotFoundError:
            raise RocheError(
                f"Roche binary not found: {self._binary}. "
                "Install with: cargo install --path crates/roche-cli"
            )
        except subprocess.CalledProcessError as e:
            raise RocheError(e.stderr.strip(), stderr=e.stderr)

    def create(self, config: SandboxConfig | None = None) -> str:
        """Create a new sandbox. Returns the sandbox ID."""
        config = config or SandboxConfig()
        cmd = [
            "create",
            "--provider", config.provider,
            "--image", config.image,
            "--timeout", str(config.timeout),
        ]

        if config.memory:
            cmd.extend(["--memory", config.memory])
        if config.cpus is not None:
            cmd.extend(["--cpus", str(config.cpus)])
        if config.network:
            cmd.append("--network")
        if config.writable:
            cmd.append("--writable")

        result = self._run(cmd)
        return result.stdout.strip()

    def exec(
        self,
        sandbox_id: str,
        command: list[str],
        timeout: int | None = None,
    ) -> ExecOutput:
        """Execute a command inside a sandbox."""
        cmd = ["exec", "--sandbox", sandbox_id]
        if timeout is not None:
            cmd.extend(["--timeout", str(timeout)])
        cmd.extend(command)

        result = self._run(cmd, check=False)
        return ExecOutput(
            exit_code=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )

    def destroy(self, sandbox_id: str) -> None:
        """Destroy a sandbox."""
        self._run(["destroy", sandbox_id])

    def list(self) -> list[dict[str, Any]]:
        """List all active sandboxes."""
        result = self._run(["list", "--json"])
        return json.loads(result.stdout)


class Sandbox:
    """Context manager for a single sandbox. Auto-creates and destroys."""

    def __init__(self, client: Roche | None = None, config: SandboxConfig | None = None):
        self._client = client or Roche()
        self._config = config or SandboxConfig()
        self._id: str | None = None

    def __enter__(self) -> Sandbox:
        self._id = self._client.create(self._config)
        return self

    def __exit__(self, *exc: object) -> None:
        if self._id:
            self._client.destroy(self._id)
            self._id = None

    @property
    def id(self) -> str:
        if self._id is None:
            raise RocheError("Sandbox not created yet")
        return self._id

    def exec(self, command: list[str], timeout: int | None = None) -> ExecOutput:
        """Execute a command in this sandbox."""
        return self._client.exec(self.id, command, timeout=timeout)
```

- [ ] **Step 2: Update `__init__.py` to re-export public API**

Replace `sdk/python/roche/__init__.py`:

```python
"""Roche — Universal sandbox orchestrator for AI agents (Python SDK)."""

__version__ = "0.1.0"

from .client import Roche, Sandbox
from .errors import RocheError
from .types import ExecOutput, SandboxConfig

__all__ = ["Roche", "Sandbox", "RocheError", "SandboxConfig", "ExecOutput"]
```

- [ ] **Step 3: Commit**

```bash
git add sdk/python/roche/client.py sdk/python/roche/__init__.py
git commit -m "feat(sdk): implement Python SDK client with Sandbox context manager"
```

---

### Task 10: Add Python SDK tests

**Files:**
- Modify: `sdk/python/pyproject.toml`
- Create: `sdk/python/tests/test_client.py`

- [ ] **Step 1: Add pytest to dev dependencies**

Add to `sdk/python/pyproject.toml`:

```toml
[project.optional-dependencies]
dev = ["pytest>=7.0"]
```

- [ ] **Step 2: Write SDK unit tests (mock subprocess)**

Create `sdk/python/tests/test_client.py`:

```python
"""Unit tests for the Roche Python SDK (no Docker required)."""

from unittest.mock import MagicMock, patch
import subprocess

import pytest

from roche import Roche, Sandbox, SandboxConfig, ExecOutput, RocheError


class TestRocheClient:
    def test_create_default_config(self):
        mock_result = MagicMock()
        mock_result.stdout = "abc123def456\n"
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche(binary="/usr/bin/roche")
            sandbox_id = client.create()

        assert sandbox_id == "abc123def456"
        args = mock_run.call_args[0][0]
        assert args[0] == "/usr/bin/roche"
        assert "create" in args
        assert "--provider" in args
        assert "docker" in args
        assert "--image" in args
        assert "python:3.12-slim" in args
        # Network and writable flags should NOT be present (defaults off)
        assert "--network" not in args
        assert "--writable" not in args

    def test_create_custom_config(self):
        mock_result = MagicMock()
        mock_result.stdout = "xyz789\n"
        mock_result.returncode = 0

        config = SandboxConfig(
            memory="1g",
            cpus=2.0,
            network=True,
            writable=True,
        )

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            sandbox_id = client.create(config)

        assert sandbox_id == "xyz789"
        args = mock_run.call_args[0][0]
        assert "--memory" in args
        assert "1g" in args
        assert "--cpus" in args
        assert "2.0" in args
        assert "--network" in args
        assert "--writable" in args

    def test_exec_returns_output(self):
        mock_result = MagicMock()
        mock_result.stdout = "4\n"
        mock_result.stderr = ""
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result):
            client = Roche()
            output = client.exec("abc123", ["python3", "-c", "print(2+2)"])

        assert isinstance(output, ExecOutput)
        assert output.exit_code == 0
        assert output.stdout == "4\n"

    def test_exec_nonzero_exit(self):
        mock_result = MagicMock()
        mock_result.stdout = ""
        mock_result.stderr = "error\n"
        mock_result.returncode = 1

        with patch("subprocess.run", return_value=mock_result):
            client = Roche()
            output = client.exec("abc123", ["false"])

        assert output.exit_code == 1
        assert output.stderr == "error\n"

    def test_destroy_calls_cli(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.destroy("abc123")

        args = mock_run.call_args[0][0]
        assert "destroy" in args
        assert "abc123" in args

    def test_list_parses_json(self):
        mock_result = MagicMock()
        mock_result.stdout = '[{"id":"abc","status":"running","provider":"docker","image":"python:3.12-slim"}]'
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result):
            client = Roche()
            sandboxes = client.list()

        assert len(sandboxes) == 1
        assert sandboxes[0]["id"] == "abc"

    def test_binary_not_found_raises_error(self):
        with patch("subprocess.run", side_effect=FileNotFoundError):
            client = Roche(binary="nonexistent")
            with pytest.raises(RocheError, match="not found"):
                client.create()

    def test_cli_error_raises_roche_error(self):
        with patch(
            "subprocess.run",
            side_effect=subprocess.CalledProcessError(1, "roche", stderr="provider unavailable"),
        ):
            client = Roche()
            with pytest.raises(RocheError, match="provider unavailable"):
                client.create()


class TestSandboxContextManager:
    def test_sandbox_creates_and_destroys(self):
        mock_create = MagicMock()
        mock_create.stdout = "sandbox123\n"
        mock_create.returncode = 0

        mock_destroy = MagicMock()
        mock_destroy.returncode = 0

        with patch("subprocess.run", side_effect=[mock_create, mock_destroy]):
            client = Roche()
            with Sandbox(client) as sb:
                assert sb.id == "sandbox123"

    def test_sandbox_id_before_enter_raises(self):
        client = Roche()
        sb = Sandbox(client)
        with pytest.raises(RocheError, match="not created"):
            _ = sb.id
```

- [ ] **Step 3: Run Python tests**

Run: `cd ~/roche/sdk/python && pip install -e ".[dev]" && pytest tests/ -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add sdk/python/pyproject.toml sdk/python/tests/test_client.py
git commit -m "test(sdk): add Python SDK unit tests with mocked subprocess"
```

---

## Chunk 5: Final Verification

### Task 11: Full CI verification pass

**Files:** None (verification only)

- [ ] **Step 1: Run the complete Rust CI pipeline**

Run:
```bash
cd ~/roche && cargo build --workspace && cargo test --workspace -- --test-threads=1 && cargo clippy --workspace -- -D warnings && cargo fmt --all --check
```
Expected: all pass.

- [ ] **Step 2: Run Python SDK tests**

Run: `cd ~/roche/sdk/python && pytest tests/ -v`
Expected: all pass.

- [ ] **Step 3: Manual smoke test**

Run:
```bash
cd ~/roche
cargo run -- create --memory 256m
# Capture the output ID, e.g. abc123def456
cargo run -- exec --sandbox <id> echo "Hello from Roche"
cargo run -- list
cargo run -- list --json
cargo run -- destroy <id>
cargo run -- list
```
Expected:
- `create` prints a 12-char hex ID
- `exec` prints "Hello from Roche"
- `list` shows the sandbox in a table
- `list --json` shows JSON array
- `destroy` exits silently
- Final `list` shows "No active sandboxes."

- [ ] **Step 4: Commit any final fixes**

If any fixes were needed, commit them individually with descriptive messages.
