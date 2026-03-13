# Phase B-A: Enhanced Docker Provider — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enhance the Docker provider with file transfer (cp + mount), automatic timeout cleanup (label + gc), pause/unpause, and batch operations, using a layered trait architecture.

**Architecture:** Existing `SandboxProvider` trait stays unchanged. Two new traits (`SandboxFileOps`, `SandboxLifecycle`) are added in `provider/mod.rs`. `DockerProvider` implements all three. New CLI commands: `cp`, `pause`, `unpause`, `gc`. Existing commands extended: `create` (--mount, --count), `destroy` (multiple IDs, --all), `list` (expires column).

**Tech Stack:** Rust 2021, clap, tokio, serde, thiserror, Python 3.12, pytest

**Spec:** `docs/superpowers/specs/2026-03-13-phase-b-enhanced-docker-design.md`

---

## Chunk 1: Core Types, Traits, and Pause/Unpause

### Task 1: Add MountConfig type and extend SandboxConfig

**Files:**
- Modify: `crates/roche-core/src/types.rs`

- [ ] **Step 1: Add MountConfig struct**

Add after the `SandboxConfig` struct:

```rust
/// Configuration for a volume mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub host_path: String,
    pub container_path: String,
    /// Default: true (readonly, AI-safe).
    pub readonly: bool,
}
```

- [ ] **Step 2: Add `mounts` field to SandboxConfig**

Add to the `SandboxConfig` struct after the `env` field:

```rust
    /// Volume mounts.
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
```

Also add `mounts: Vec::new(),` to the `Default` impl.

- [ ] **Step 3: Add `Paused` variant to SandboxStatus**

```rust
pub enum SandboxStatus {
    Running,
    Paused,
    Stopped,
    Failed,
}
```

- [ ] **Step 4: Add `expires_at` to SandboxInfo**

```rust
pub struct SandboxInfo {
    pub id: SandboxId,
    pub status: SandboxStatus,
    pub provider: String,
    pub image: String,
    pub expires_at: Option<u64>,
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compilation may fail in `docker.rs` because `SandboxInfo` construction is missing `expires_at`. That's expected — we'll fix it in Task 3.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-core/src/types.rs
git commit -m "feat: add MountConfig, Paused status, expires_at to core types"
```

---

### Task 2: Add new traits and error variants to provider/mod.rs

**Files:**
- Modify: `crates/roche-core/src/provider/mod.rs`
- Modify: `crates/roche-core/src/lib.rs`

- [ ] **Step 1: Add new error variants to ProviderError**

Add these variants to the `ProviderError` enum:

```rust
    #[error("operation not supported by this provider: {0}")]
    Unsupported(String),

    #[error("file operation failed: {0}")]
    FileFailed(String),

    #[error("sandbox is paused: {0}")]
    Paused(SandboxId),
```

- [ ] **Step 2: Add SandboxFileOps trait**

Add after `SandboxProvider`:

```rust
use crate::types::SandboxId;

/// File operations capability — not all providers support this.
#[allow(async_fn_in_trait)]
pub trait SandboxFileOps {
    /// Copy a file from host to sandbox.
    async fn copy_to(
        &self,
        id: &SandboxId,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), ProviderError>;

    /// Copy a file from sandbox to host.
    async fn copy_from(
        &self,
        id: &SandboxId,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), ProviderError>;
}
```

- [ ] **Step 3: Add SandboxLifecycle trait**

```rust
/// Lifecycle management capability — not all providers support this.
#[allow(async_fn_in_trait)]
pub trait SandboxLifecycle {
    /// Pause a sandbox (freeze all processes).
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError>;

    /// Unpause a sandbox.
    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError>;

    /// Garbage collect: destroy all expired sandboxes. Returns IDs of destroyed sandboxes.
    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError>;
}
```

- [ ] **Step 4: Update lib.rs re-exports**

In `crates/roche-core/src/lib.rs`, add:

```rust
pub use provider::{SandboxFileOps, SandboxLifecycle};
pub use types::MountConfig;
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build 2>&1`
Expected: May still fail in `docker.rs` due to missing `expires_at` — that's OK.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-core/src/provider/mod.rs crates/roche-core/src/lib.rs
git commit -m "feat: add SandboxFileOps and SandboxLifecycle traits"
```

---

### Task 3: Implement pause/unpause and fix docker.rs compilation

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Write test for parse_status("paused")**

Add to the existing `tests` module in `docker.rs`:

```rust
    #[test]
    fn test_parse_status_paused() {
        assert_eq!(parse_status("paused"), SandboxStatus::Paused);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p roche-core test_parse_status_paused 2>&1`
Expected: FAIL — `parse_status` doesn't handle "paused".

- [ ] **Step 3: Update parse_status to handle "paused"**

In `docker.rs`, update the `parse_status` function:

```rust
fn parse_status(state: &str) -> SandboxStatus {
    match state {
        "running" => SandboxStatus::Running,
        "paused" => SandboxStatus::Paused,
        "exited" | "created" => SandboxStatus::Stopped,
        _ => SandboxStatus::Failed,
    }
}
```

- [ ] **Step 4: Fix SandboxInfo construction in `list`**

In the `list` method, update the `SandboxInfo` construction to include `expires_at`:

```rust
                SandboxInfo {
                    id: parts.first().unwrap_or(&"").to_string(),
                    status: parse_status(parts.get(1).unwrap_or(&"unknown")),
                    provider: "docker".to_string(),
                    image: parts.get(2).unwrap_or(&"").to_string(),
                    expires_at: None, // Will be populated in Task 6
                }
```

- [ ] **Step 5: Implement SandboxLifecycle for DockerProvider (pause/unpause only, gc stub)**

Add after the `SandboxProvider` impl block:

```rust
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
        // Stub — will be implemented in Task 6
        Ok(vec![])
    }
}
```

- [ ] **Step 6: Add use import for SandboxLifecycle**

At the top of `docker.rs`, add to the existing import:

```rust
use crate::provider::{ProviderError, SandboxProvider, SandboxLifecycle};
```

- [ ] **Step 7: Run all tests**

Run: `cargo test -p roche-core 2>&1`
Expected: All tests pass (including the new `test_parse_status_paused`).

- [ ] **Step 8: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1`
Expected: No warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "feat: implement pause/unpause for DockerProvider"
```

---

### Task 4: Add pause/unpause CLI commands

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Add Pause and Unpause to Commands enum**

```rust
    /// Pause a sandbox (freeze all processes)
    Pause {
        /// Sandbox ID
        id: String,
    },

    /// Unpause a sandbox
    Unpause {
        /// Sandbox ID
        id: String,
    },
```

- [ ] **Step 2: Handle Pause/Unpause in the run() function**

Add to the `match cli.command` block, after `Commands::Destroy`:

```rust
        Commands::Pause { id } => {
            provider.pause(&id).await?;
        }
        Commands::Unpause { id } => {
            provider.unpause(&id).await?;
        }
```

- [ ] **Step 3: Add use import for SandboxLifecycle**

In the `run()` function's use block, add:

```rust
    use roche_core::provider::SandboxLifecycle;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build 2>&1`
Expected: Compiles successfully.

- [ ] **Step 5: Verify CLI help shows new commands**

Run: `cargo run -- --help 2>&1`
Expected: Shows `pause` and `unpause` commands in the list.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat: add pause and unpause CLI commands"
```

---

## Chunk 2: File Transfer (cp + mount)

### Task 5: Implement SandboxFileOps for DockerProvider

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Write test for build_create_args with mounts**

Add to the existing `tests` module:

```rust
    #[test]
    fn test_build_create_args_with_mounts() {
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

        // Check readonly mount
        let v_positions: Vec<usize> = args.iter()
            .enumerate()
            .filter(|(_, a)| a == &"-v")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(v_positions.len(), 2);
        assert_eq!(args[v_positions[0] + 1], "/host/data:/sandbox/data:ro");
        assert_eq!(args[v_positions[1] + 1], "/host/out:/sandbox/out:rw");
    }
```

Also add `use crate::types::MountConfig;` to the test module imports.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p roche-core test_build_create_args_with_mounts 2>&1`
Expected: FAIL — `build_create_args` doesn't handle mounts yet.

- [ ] **Step 3: Add mount handling to build_create_args**

In `build_create_args`, add after the environment variables loop and before the image push:

```rust
    // Volume mounts
    for mount in &config.mounts {
        let mode = if mount.readonly { "ro" } else { "rw" };
        args.extend([
            "-v".into(),
            format!("{}:{}:{}", mount.host_path, mount.container_path, mode),
        ]);
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p roche-core test_build_create_args_with_mounts 2>&1`
Expected: PASS

- [ ] **Step 5: Implement SandboxFileOps for DockerProvider**

Add after the `SandboxLifecycle` impl block:

```rust
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
```

- [ ] **Step 6: Add use import for SandboxFileOps**

Update the import at the top of `docker.rs`:

```rust
use crate::provider::{ProviderError, SandboxProvider, SandboxLifecycle, SandboxFileOps};
```

- [ ] **Step 7: Run all tests and clippy**

Run: `cargo test -p roche-core 2>&1 && cargo clippy -- -D warnings 2>&1`
Expected: All tests pass, no clippy warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "feat: implement SandboxFileOps and mount support for DockerProvider"
```

---

### Task 6: Add cp and --mount CLI commands

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Write test for parse_mount helper**

Add a `parse_mount` helper function and test:

```rust
fn parse_mount(s: &str) -> Result<roche_core::types::MountConfig, String> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    match parts.len() {
        2 => Ok(roche_core::types::MountConfig {
            host_path: parts[0].to_string(),
            container_path: parts[1].to_string(),
            readonly: true, // AI-safe default
        }),
        3 => {
            let readonly = match parts[2] {
                "ro" => true,
                "rw" => false,
                other => return Err(format!("invalid mount mode: {other} (expected ro or rw)")),
            };
            Ok(roche_core::types::MountConfig {
                host_path: parts[0].to_string(),
                container_path: parts[1].to_string(),
                readonly,
            })
        }
        _ => Err(format!("invalid mount format: {s} (expected host:container[:ro|rw])")),
    }
}
```

Add tests:

```rust
    #[test]
    fn test_parse_mount_with_mode() {
        let m = parse_mount("/host:/container:rw").unwrap();
        assert_eq!(m.host_path, "/host");
        assert_eq!(m.container_path, "/container");
        assert!(!m.readonly);
    }

    #[test]
    fn test_parse_mount_default_readonly() {
        let m = parse_mount("/host:/container").unwrap();
        assert!(m.readonly);
    }

    #[test]
    fn test_parse_mount_invalid() {
        assert!(parse_mount("nocolon").is_err());
        assert!(parse_mount("/host:/container:xx").is_err());
    }
```

- [ ] **Step 2: Write test for parse_cp_path helper**

```rust
fn parse_cp_path(s: &str) -> Option<(&str, &str)> {
    s.split_once(':')
}
```

Add tests:

```rust
    #[test]
    fn test_parse_cp_path() {
        assert_eq!(parse_cp_path("abc123:/app/file"), Some(("abc123", "/app/file")));
        assert_eq!(parse_cp_path("./local.txt"), None);
    }
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p roche-cli 2>&1`
Expected: All tests pass.

- [ ] **Step 4: Add Cp command and --mount flag to CLI**

Add to the `Commands` enum:

```rust
    /// Copy files between host and sandbox
    Cp {
        /// Source path (local path or sandbox_id:/path)
        src: String,
        /// Destination path (local path or sandbox_id:/path)
        dest: String,
    },
```

Add `--mount` to `Create`:

```rust
        /// Volume mounts (host:container[:ro|rw], repeatable)
        #[arg(long = "mount", value_name = "HOST:CONTAINER[:ro|rw]")]
        mounts: Vec<String>,
```

- [ ] **Step 5: Handle Cp and mounts in run()**

Add the `mounts` field to the `Create` match arm destructuring. Parse mounts in the Create handler:

```rust
            let mount_configs: Vec<_> = mounts.iter()
                .map(|s| parse_mount(s).unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }))
                .collect();
```

Add `mounts: mount_configs,` to the `SandboxConfig` construction.

Add the Cp handler:

```rust
        Commands::Cp { src, dest } => {
            use roche_core::provider::SandboxFileOps;

            match (parse_cp_path(&src), parse_cp_path(&dest)) {
                (Some((sandbox_id, sandbox_path)), None) => {
                    // sandbox → host
                    provider.copy_from(sandbox_id, sandbox_path, std::path::Path::new(&dest)).await?;
                }
                (None, Some((sandbox_id, sandbox_path))) => {
                    // host → sandbox
                    provider.copy_to(sandbox_id, std::path::Path::new(&src), sandbox_path).await?;
                }
                (Some(_), Some(_)) => {
                    eprintln!("Error: both source and destination cannot be sandbox paths");
                    std::process::exit(1);
                }
                (None, None) => {
                    eprintln!("Error: one of source or destination must be a sandbox path (sandbox_id:/path)");
                    std::process::exit(1);
                }
            }
        }
```

- [ ] **Step 6: Verify it compiles and CLI help is correct**

Run: `cargo build 2>&1 && cargo run -- cp --help 2>&1`
Expected: Shows `cp` command with `src` and `dest` args.

Run: `cargo run -- create --help 2>&1`
Expected: Shows `--mount` flag.

- [ ] **Step 7: Run all tests and clippy**

Run: `cargo test 2>&1 && cargo clippy -- -D warnings 2>&1`
Expected: All pass.

- [ ] **Step 8: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat: add cp command and --mount flag to CLI"
```

---

## Chunk 3: Timeout Cleanup (GC)

### Task 7: Add expiry label to build_create_args and update list

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Write test for expiry label in build_create_args**

Add to tests:

```rust
    #[test]
    fn test_build_create_args_has_expires_label() {
        let config = SandboxConfig::default(); // timeout_secs = 300
        let args = build_create_args(&config);

        // Find the roche.expires label
        let label_positions: Vec<usize> = args.iter()
            .enumerate()
            .filter(|(_, a)| a == &"--label")
            .map(|(i, _)| i)
            .collect();

        let expires_label = label_positions.iter()
            .find(|&&i| args[i + 1].starts_with("roche.expires="))
            .expect("should have roche.expires label");

        let value: u64 = args[*expires_label + 1]
            .strip_prefix("roche.expires=")
            .unwrap()
            .parse()
            .expect("expires should be a unix timestamp");

        // Should be roughly now + 300 (within 5 seconds tolerance)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(value >= now + 295 && value <= now + 305);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p roche-core test_build_create_args_has_expires_label 2>&1`
Expected: FAIL — no expires label yet.

- [ ] **Step 3: Add expiry label to build_create_args**

In `build_create_args`, add after the `roche.managed=true` label:

```rust
    // Expiry timestamp
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() + config.timeout_secs;
    args.extend(["--label".into(), format!("roche.expires={expires}")]);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p roche-core test_build_create_args_has_expires_label 2>&1`
Expected: PASS

- [ ] **Step 5: Update list to parse expires_at**

Update the Docker format string in the `list` method:

```rust
                "--format",
                "{{.ID}}\t{{.State}}\t{{.Image}}\t{{index .Labels \"roche.expires\"}}",
```

Update the `SandboxInfo` construction:

```rust
                SandboxInfo {
                    id: parts.first().unwrap_or(&"").to_string(),
                    status: parse_status(parts.get(1).unwrap_or(&"unknown")),
                    provider: "docker".to_string(),
                    image: parts.get(2).unwrap_or(&"").to_string(),
                    expires_at: parts.get(3).and_then(|s| s.parse::<u64>().ok()),
                }
```

- [ ] **Step 6: Run all tests**

Run: `cargo test -p roche-core 2>&1`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs
git commit -m "feat: add expiry label to sandbox creation and parse in list"
```

---

### Task 8: Implement gc and add GC CLI command

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Implement gc in DockerProvider**

Replace the `gc` stub in `SandboxLifecycle` impl:

```rust
    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError> {
        let output = Command::new("docker")
            .args([
                "ps", "-a",
                "--filter", "label=roche.managed=true",
                "--format", "{{.ID}}\t{{index .Labels \"roche.expires\"}}",
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
```

- [ ] **Step 2: Add Gc command to CLI**

Add to `Commands` enum:

```rust
    /// Garbage collect expired sandboxes
    Gc {
        /// Only list expired sandboxes, don't destroy
        #[arg(long)]
        dry_run: bool,

        /// Destroy ALL roche-managed sandboxes (ignore expiry)
        #[arg(long)]
        all: bool,
    },
```

- [ ] **Step 3: Handle Gc in run()**

```rust
        Commands::Gc { dry_run, all } => {
            if all {
                let sandboxes = provider.list().await?;
                for sb in &sandboxes {
                    if dry_run {
                        println!("{}", sb.id);
                    } else {
                        provider.destroy(&sb.id).await?;
                        println!("destroyed: {}", sb.id);
                    }
                }
            } else if dry_run {
                // List expired without destroying
                let sandboxes = provider.list().await?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                for sb in &sandboxes {
                    if let Some(exp) = sb.expires_at {
                        if exp <= now {
                            println!("{}", sb.id);
                        }
                    }
                }
            } else {
                let destroyed = provider.gc().await?;
                for id in &destroyed {
                    println!("destroyed: {id}");
                }
                if destroyed.is_empty() {
                    println!("No expired sandboxes found.");
                }
            }
        }
```

- [ ] **Step 4: Update list display to show expires column**

In the `List` handler, update the table header and row formatting:

```rust
            } else if sandboxes.is_empty() {
                println!("No active sandboxes.");
            } else {
                println!("{:<16} {:<10} {:<10} {:<10} IMAGE", "ID", "STATUS", "PROVIDER", "EXPIRES");
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                for sb in &sandboxes {
                    let expires_str = match sb.expires_at {
                        Some(exp) if exp > now => {
                            let remaining = exp - now;
                            let mins = remaining / 60;
                            let secs = remaining % 60;
                            format!("{mins}m{secs:02}s")
                        }
                        Some(_) => "expired".to_string(),
                        None => "-".to_string(),
                    };
                    println!(
                        "{:<16} {:<10} {:<10} {:<10} {}",
                        sb.id,
                        format!("{:?}", sb.status).to_lowercase(),
                        sb.provider,
                        expires_str,
                        sb.image,
                    );
                }
            }
```

- [ ] **Step 5: Verify it compiles and CLI help is correct**

Run: `cargo build 2>&1 && cargo run -- gc --help 2>&1`
Expected: Shows `gc` command with `--dry-run` and `--all` flags.

- [ ] **Step 6: Run all tests and clippy**

Run: `cargo test 2>&1 && cargo clippy -- -D warnings 2>&1`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs crates/roche-cli/src/main.rs
git commit -m "feat: add gc command and expires display in list"
```

---

## Chunk 4: Batch Operations

### Task 9: Add batch create (--count) and batch destroy (multiple IDs, --all)

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Add --count to Create command**

```rust
        /// Number of sandboxes to create
        #[arg(long, default_value = "1")]
        count: u32,
```

- [ ] **Step 2: Update Create handler for --count**

```rust
        // In the Create handler, after building config:
        Commands::Create {
            // ... existing fields ...
            count,
            mounts,
        } => {
            // ... existing parsing ...
            for _ in 0..count {
                match provider.create(&config).await {
                    Ok(id) => println!("{id}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
        }
```

Note: The existing code does `let id = provider.create(&config).await?;` and `println!("{id}");`. Replace this with the loop above. The loop prints each ID on success and errors on stderr without stopping.

- [ ] **Step 3: Modify Destroy to accept multiple IDs and --all**

Change the `Destroy` variant:

```rust
    /// Destroy sandboxes
    Destroy {
        /// Sandbox IDs (one or more)
        #[arg(required_unless_present = "all")]
        ids: Vec<String>,

        /// Destroy ALL roche-managed sandboxes
        #[arg(long)]
        all: bool,
    },
```

- [ ] **Step 4: Update Destroy handler**

```rust
        Commands::Destroy { ids, all } => {
            let targets = if all {
                provider.list().await?.into_iter().map(|sb| sb.id).collect()
            } else {
                ids
            };
            for id in &targets {
                match provider.destroy(id).await {
                    Ok(()) => {}
                    Err(e) => eprintln!("Error destroying {id}: {e}"),
                }
            }
        }
```

- [ ] **Step 5: Verify it compiles and CLI help is correct**

Run: `cargo build 2>&1 && cargo run -- create --help 2>&1`
Expected: Shows `--count` flag.

Run: `cargo run -- destroy --help 2>&1`
Expected: Shows `ids` positional (multiple) and `--all` flag.

- [ ] **Step 6: Run all tests and clippy**

Run: `cargo test 2>&1 && cargo clippy -- -D warnings 2>&1`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat: add --count for batch create and multi-ID/--all for destroy"
```

---

## Chunk 5: Python SDK

### Task 10: Add Mount type and update SandboxConfig in Python SDK

**Files:**
- Modify: `sdk/python/roche/types.py`
- Modify: `sdk/python/roche/__init__.py`

- [ ] **Step 1: Add Mount dataclass**

In `types.py`, add after `ExecOutput`:

```python
@dataclass
class Mount:
    """Volume mount configuration."""

    host_path: str
    container_path: str
    readonly: bool = True  # AI-safe default
```

- [ ] **Step 2: Add mounts field to SandboxConfig**

```python
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
    mounts: list[Mount] = field(default_factory=list)
```

- [ ] **Step 3: Update __init__.py to export Mount**

```python
from .types import ExecOutput, Mount, SandboxConfig

__all__ = ["Roche", "Sandbox", "RocheError", "SandboxConfig", "ExecOutput", "Mount"]
```

- [ ] **Step 4: Commit**

```bash
git add sdk/python/roche/types.py sdk/python/roche/__init__.py
git commit -m "feat: add Mount type to Python SDK"
```

---

### Task 11: Add new methods to Python SDK client

**Files:**
- Modify: `sdk/python/roche/client.py`
- Modify: `sdk/python/tests/test_client.py`

- [ ] **Step 1: Write tests for new methods**

Add to `test_client.py`:

```python
class TestNewFeatures:
    def test_create_with_mounts(self):
        from roche import Mount

        mock_result = MagicMock()
        mock_result.stdout = "mount123\n"
        mock_result.returncode = 0

        config = SandboxConfig(mounts=[
            Mount("/host/data", "/sandbox/data"),
            Mount("/host/out", "/sandbox/out", readonly=False),
        ])

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            sandbox_id = client.create(config)

        assert sandbox_id == "mount123"
        args = mock_run.call_args[0][0]
        assert "--mount" in args
        assert "/host/data:/sandbox/data:ro" in args
        assert "/host/out:/sandbox/out:rw" in args

    def test_copy_to(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.copy_to("abc123", "./local.py", "/app/local.py")

        args = mock_run.call_args[0][0]
        assert "cp" in args
        assert "./local.py" in args
        assert "abc123:/app/local.py" in args

    def test_copy_from(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.copy_from("abc123", "/app/result.json", "./result.json")

        args = mock_run.call_args[0][0]
        assert "cp" in args
        assert "abc123:/app/result.json" in args
        assert "./result.json" in args

    def test_pause(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.pause("abc123")

        args = mock_run.call_args[0][0]
        assert "pause" in args
        assert "abc123" in args

    def test_unpause(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.unpause("abc123")

        args = mock_run.call_args[0][0]
        assert "unpause" in args
        assert "abc123" in args

    def test_gc(self):
        mock_result = MagicMock()
        mock_result.stdout = "destroyed: abc123\ndestroyed: def456\n"
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.gc()

        args = mock_run.call_args[0][0]
        assert "gc" in args

    def test_create_many(self):
        mock_result = MagicMock()
        mock_result.stdout = "id1\nid2\nid3\n"
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            ids = client.create_many(count=3)

        assert ids == ["id1", "id2", "id3"]
        args = mock_run.call_args[0][0]
        assert "--count" in args
        assert "3" in args

    def test_destroy_many(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.destroy_many(["id1", "id2"])

        args = mock_run.call_args[0][0]
        assert "destroy" in args
        assert "id1" in args
        assert "id2" in args

    def test_destroy_all(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.destroy_all()

        args = mock_run.call_args[0][0]
        assert "destroy" in args
        assert "--all" in args

    def test_sandbox_pause_unpause(self):
        mock_create = MagicMock()
        mock_create.stdout = "sandbox123\n"
        mock_create.returncode = 0

        mock_op = MagicMock()
        mock_op.returncode = 0

        mock_destroy = MagicMock()
        mock_destroy.returncode = 0

        with patch("subprocess.run", side_effect=[mock_create, mock_op, mock_op, mock_destroy]):
            client = Roche()
            with Sandbox(client) as sb:
                sb.pause()
                sb.unpause()

    def test_sandbox_copy_to_from(self):
        mock_create = MagicMock()
        mock_create.stdout = "sandbox123\n"
        mock_create.returncode = 0

        mock_cp = MagicMock()
        mock_cp.returncode = 0

        mock_destroy = MagicMock()
        mock_destroy.returncode = 0

        with patch("subprocess.run", side_effect=[mock_create, mock_cp, mock_cp, mock_destroy]):
            client = Roche()
            with Sandbox(client) as sb:
                sb.copy_to("./local.py", "/app/local.py")
                sb.copy_from("/app/result.json", "./result.json")
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `python3 -m pytest sdk/python/tests/test_client.py -v 2>&1`
Expected: New tests fail (methods don't exist yet).

- [ ] **Step 3: Add new methods to Roche client class**

In `client.py`, add to the `Roche` class after `list()`:

```python
    def copy_to(self, sandbox_id: str, local_path: str, sandbox_path: str) -> None:
        """Copy a file from host to sandbox."""
        self._run(["cp", local_path, f"{sandbox_id}:{sandbox_path}"])

    def copy_from(self, sandbox_id: str, sandbox_path: str, local_path: str) -> None:
        """Copy a file from sandbox to host."""
        self._run(["cp", f"{sandbox_id}:{sandbox_path}", local_path])

    def pause(self, sandbox_id: str) -> None:
        """Pause a sandbox."""
        self._run(["pause", sandbox_id])

    def unpause(self, sandbox_id: str) -> None:
        """Unpause a sandbox."""
        self._run(["unpause", sandbox_id])

    def gc(self) -> None:
        """Garbage collect expired sandboxes."""
        self._run(["gc"])

    def create_many(self, config: SandboxConfig | None = None, count: int = 1) -> list[str]:
        """Create multiple sandboxes. Returns list of sandbox IDs."""
        config = config or SandboxConfig()
        cmd = [
            "create",
            "--provider", config.provider,
            "--image", config.image,
            "--timeout", str(config.timeout),
            "--count", str(count),
        ]

        if config.memory:
            cmd.extend(["--memory", config.memory])
        if config.cpus is not None:
            cmd.extend(["--cpus", str(config.cpus)])
        if config.network:
            cmd.append("--network")
        if config.writable:
            cmd.append("--writable")

        for key, value in config.env.items():
            cmd.extend(["--env", f"{key}={value}"])

        for mount in config.mounts:
            mode = "ro" if mount.readonly else "rw"
            cmd.extend(["--mount", f"{mount.host_path}:{mount.container_path}:{mode}"])

        result = self._run(cmd)
        return [line for line in result.stdout.strip().split("\n") if line]

    def destroy_many(self, sandbox_ids: list[str]) -> None:
        """Destroy multiple sandboxes."""
        self._run(["destroy", *sandbox_ids])

    def destroy_all(self) -> None:
        """Destroy all roche-managed sandboxes."""
        self._run(["destroy", "--all"])
```

- [ ] **Step 4: Update create() to forward mounts**

In the existing `create()` method, add after the env loop:

```python
        for mount in config.mounts:
            mode = "ro" if mount.readonly else "rw"
            cmd.extend(["--mount", f"{mount.host_path}:{mount.container_path}:{mode}"])
```

- [ ] **Step 5: Add methods to Sandbox context manager**

In the `Sandbox` class, add after `exec()`:

```python
    def copy_to(self, local_path: str, sandbox_path: str) -> None:
        """Copy a file from host to this sandbox."""
        self._client.copy_to(self.id, local_path, sandbox_path)

    def copy_from(self, sandbox_path: str, local_path: str) -> None:
        """Copy a file from this sandbox to host."""
        self._client.copy_from(self.id, sandbox_path, local_path)

    def pause(self) -> None:
        """Pause this sandbox."""
        self._client.pause(self.id)

    def unpause(self) -> None:
        """Unpause this sandbox."""
        self._client.unpause(self.id)
```

- [ ] **Step 6: Run tests**

Run: `python3 -m pytest sdk/python/tests/test_client.py -v 2>&1`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add sdk/python/roche/client.py sdk/python/tests/test_client.py
git commit -m "feat: add cp, pause, unpause, gc, batch ops to Python SDK"
```

---

### Task 12: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Run full Rust test suite**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 2: Run clippy and fmt**

Run: `cargo clippy -- -D warnings 2>&1 && cargo fmt --check 2>&1`
Expected: Clean.

- [ ] **Step 3: Run Python tests**

Run: `python3 -m pytest sdk/python/tests/ -v 2>&1`
Expected: All tests pass.

- [ ] **Step 4: Verify all new CLI commands appear**

Run: `cargo run -- --help 2>&1`
Expected: Shows commands: create, exec, destroy, list, cp, pause, unpause, gc.
