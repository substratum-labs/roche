# Phase A: MVP Completion — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Roche MVP with `--env` CLI flag, README, CI/CD, and publishing preparation.

**Architecture:** Four independent work items executed sequentially. First commit the existing bugfix, then add `--env` flag (CLI + Python SDK), write full README, create GitHub Actions CI workflow, and finalize publishing metadata.

**Tech Stack:** Rust (clap, tokio, serde), Python (subprocess wrapper), GitHub Actions

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `crates/roche-cli/src/main.rs` | Add `--env` flag + `parse_env_vars()` |
| Modify | `sdk/python/roche/client.py` | Forward `config.env` as `--env` flags |
| Modify | `sdk/python/tests/test_client.py` | Test env var passthrough |
| Modify | `README.md` | Full project documentation |
| Create | `.github/workflows/ci.yml` | CI/CD workflow |
| Create | `CHANGELOG.md` | Release changelog |
| Modify | `Cargo.toml` (workspace root) | Add `keywords` |

---

## Chunk 1: Bugfix Commit + `--env` CLI Flag

### Task 1: Commit existing `trailing_var_arg` bugfix

**Files:**
- Modified (already): `crates/roche-cli/src/main.rs:55`

- [ ] **Step 1: Commit the uncommitted fix**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "fix: allow hyphenated arguments in exec command

Add trailing_var_arg and allow_hyphen_values to the Exec command's
command field so flags like -c are passed through to the sandbox."
```

---

### Task 2: Add `--env` flag to CLI with unit tests

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Add `parse_env_vars` function with unit tests**

In `crates/roche-cli/src/main.rs`, add a helper function before the `run` function:

```rust
fn parse_env_vars(pairs: &[String]) -> Result<std::collections::HashMap<String, String>, String> {
    pairs
        .iter()
        .map(|s| {
            let (k, v) = s
                .split_once('=')
                .ok_or_else(|| format!("invalid env format: {s} (expected KEY=VALUE)"))?;
            Ok((k.to_string(), v.to_string()))
        })
        .collect()
}
```

Add tests at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::parse_env_vars;

    #[test]
    fn test_parse_env_vars_happy_path() {
        let input = vec!["FOO=bar".to_string()];
        let result = parse_env_vars(&input).unwrap();
        assert_eq!(result.get("FOO").unwrap(), "bar");
    }

    #[test]
    fn test_parse_env_vars_value_with_equals() {
        let input = vec!["A=b=c".to_string()];
        let result = parse_env_vars(&input).unwrap();
        assert_eq!(result.get("A").unwrap(), "b=c");
    }

    #[test]
    fn test_parse_env_vars_malformed() {
        let input = vec!["NOEQUALS".to_string()];
        assert!(parse_env_vars(&input).is_err());
    }

    #[test]
    fn test_parse_env_vars_multiple() {
        let input = vec!["FOO=bar".to_string(), "BAZ=qux".to_string()];
        let result = parse_env_vars(&input).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("FOO").unwrap(), "bar");
        assert_eq!(result.get("BAZ").unwrap(), "qux");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (function exists but not wired yet)**

Run: `cargo test -p roche-cli`
Expected: All 4 new tests pass (function is defined, just not wired to CLI yet)

- [ ] **Step 3: Add `env` field to `Create` variant and wire it up**

Add the `env` field to the `Create` variant (after the `writable` field):

```rust
        /// Environment variables (KEY=VALUE, repeatable)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,
```

Update the `Commands::Create` match arm to include `env` in destructuring and use `parse_env_vars`:

```rust
        Commands::Create {
            provider: provider_name,
            image,
            memory,
            cpus,
            timeout,
            network,
            writable,
            env,
        } => {
            let env_map = parse_env_vars(&env).map_err(|e| {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }).unwrap();
            let config = SandboxConfig {
                provider: provider_name,
                image,
                memory,
                cpus,
                timeout_secs: timeout,
                network,
                writable,
                env: env_map,
                ..Default::default()
            };
            let id = provider.create(&config).await?;
            println!("{id}");
        }
```

- [ ] **Step 4: Verify it compiles and all tests pass**

Run: `cargo build && cargo test -p roche-cli`
Expected: Success, all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat: add --env KEY=VALUE flag to create command

Wire SandboxConfig.env to the CLI. Supports repeatable --env flags
with KEY=VALUE format, split at first = sign. Includes unit tests
for parse_env_vars (happy path, value with =, malformed, multiple)."
```

---

### Task 3: Python SDK — forward env vars

**Files:**
- Modify: `sdk/python/roche/client.py:38-57`
- Modify: `sdk/python/tests/test_client.py`

- [ ] **Step 1: Write failing test for env var passthrough**

Add to `sdk/python/tests/test_client.py` in `TestRocheClient`:

```python
    def test_create_with_env_vars(self):
        mock_result = MagicMock()
        mock_result.stdout = "env123\n"
        mock_result.returncode = 0

        config = SandboxConfig(env={"FOO": "bar", "DB": "localhost"})

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            sandbox_id = client.create(config)

        assert sandbox_id == "env123"
        args = mock_run.call_args[0][0]
        assert "--env" in args
        assert "FOO=bar" in args
        assert "DB=localhost" in args
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/python && python -m pytest tests/test_client.py::TestRocheClient::test_create_with_env_vars -v`
Expected: FAIL — `--env` not in args

- [ ] **Step 3: Implement env passthrough in client.py**

In `sdk/python/roche/client.py`, add after line 55 (`if config.writable:` block), before `result = self._run(cmd)`:

```python
        for key, value in config.env.items():
            cmd.extend(["--env", f"{key}={value}"])
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/python && python -m pytest tests/test_client.py::TestRocheClient::test_create_with_env_vars -v`
Expected: PASS

- [ ] **Step 5: Run all Python tests**

Run: `cd sdk/python && python -m pytest tests/ -v`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add sdk/python/roche/client.py sdk/python/tests/test_client.py
git commit -m "feat: forward env vars from Python SDK to CLI

Add --env KEY=VALUE passthrough in Roche.create() for each entry
in SandboxConfig.env."
```

---

## Chunk 2: README

### Task 4: Write complete README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace README.md with full documentation**

Replace the entire contents of `README.md` with:

```markdown
# Roche

> Universal sandbox orchestrator for AI agents.

[![CI](https://github.com/substratum-labs/roche/actions/workflows/ci.yml/badge.svg)](https://github.com/substratum-labs/roche/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Roche provides a single abstraction over multiple sandbox providers (Docker, Firecracker, WASM) with **AI-optimized security defaults** — network disabled, filesystem readonly, timeout enforced.

Named after [Édouard Roche](https://en.wikipedia.org/wiki/%C3%89douard_Roche) — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

## Why Roche?

Every AI agent framework independently integrates sandbox providers, creating an N×M complexity problem:

```
LangChain ──┐         ┌── Docker
CrewAI   ───┤  N × M  ├── E2B
AutoGen  ───┘         └── Modal
```

Roche reduces this to N+M:

```
LangChain ──┐              ┌── Docker
CrewAI   ───┤── Roche() ───├── Firecracker
AutoGen  ───┘              └── WASM
```

## Features

- **AI-safe defaults** — network off, readonly filesystem, 300s timeout
- **Multi-provider** — Docker (MVP), Firecracker, WASM (planned)
- **CLI + SDK** — `roche` binary + Python SDK
- **Resource limits** — memory, CPU, PID limits, timeout enforcement
- **Zero config** — sensible defaults, opt-in for permissions

## Quick Start

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) installed and running
- [Rust](https://rustup.rs/) toolchain (for building from source)

### Install

```bash
cargo install --path crates/roche-cli
```

### Usage

```bash
# Create a sandbox (network off, readonly FS by default)
SANDBOX_ID=$(roche create --provider docker --memory 512m)

# Execute code in the sandbox
roche exec --sandbox $SANDBOX_ID python3 -c "print('Hello from Roche!')"

# List active sandboxes
roche list

# Clean up
roche destroy $SANDBOX_ID
```

## CLI Reference

### `roche create`

Create a new sandbox and print its ID.

| Flag | Default | Description |
|------|---------|-------------|
| `--provider` | `docker` | Sandbox provider |
| `--image` | `python:3.12-slim` | Container image |
| `--memory` | (none) | Memory limit (e.g. `512m`, `1g`) |
| `--cpus` | (none) | CPU limit (e.g. `0.5`, `2.0`) |
| `--timeout` | `300` | Sandbox timeout in seconds |
| `--network` | off | Enable network access |
| `--writable` | off | Enable writable filesystem |
| `--env` | (none) | Environment variable `KEY=VALUE` (repeatable) |

### `roche exec`

Execute a command inside an existing sandbox.

| Flag | Default | Description |
|------|---------|-------------|
| `--sandbox` | (required) | Sandbox ID |
| `--timeout` | (none) | Timeout override in seconds |

Remaining arguments are the command to execute.

### `roche destroy`

Destroy a sandbox and release its resources.

```bash
roche destroy <SANDBOX_ID>
```

### `roche list`

List all active Roche-managed sandboxes.

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | off | Output as JSON |

## Python SDK

### Install

```bash
pip install -e sdk/python
```

### Usage

```python
from roche import Roche, Sandbox, SandboxConfig

# Direct client usage
client = Roche()
sandbox_id = client.create(SandboxConfig(memory="512m"))
output = client.exec(sandbox_id, ["python3", "-c", "print(2 + 2)"])
print(output.stdout)  # "4\n"
client.destroy(sandbox_id)

# Context manager (auto-cleanup)
with Sandbox(config=SandboxConfig(memory="512m")) as sb:
    result = sb.exec(["python3", "-c", "print('Hello!')"])
    print(result.stdout)
# sandbox is automatically destroyed
```

### Configuration

```python
config = SandboxConfig(
    provider="docker",          # sandbox provider
    image="python:3.12-slim",   # container image
    memory="1g",                # memory limit
    cpus=2.0,                   # CPU limit
    timeout=600,                # timeout in seconds
    network=True,               # enable network (default: False)
    writable=True,              # enable writable FS (default: False)
    env={"API_KEY": "secret"},  # environment variables
)
```

## Security Defaults

Roche is designed for AI agent workloads where untrusted code execution is the norm:

| Setting | Default | Rationale |
|---------|---------|-----------|
| Network | **disabled** | Prevent data exfiltration and C2 communication |
| Filesystem | **readonly** | Prevent persistent compromise and file tampering |
| Timeout | **300s** | Prevent resource exhaustion and infinite loops |
| PID limit | **256** | Prevent fork bombs |
| Privileges | **no-new-privileges** | Prevent privilege escalation |

Override explicitly when needed: `--network`, `--writable`.

## Development

```bash
cargo build              # Build all crates
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt --check        # Check formatting

# Python SDK
pip install -e "sdk/python[dev]"
pytest sdk/python/tests/ -v
```

## License

Apache-2.0
```

- [ ] **Step 2: Verify README renders correctly (visual check)**

Run: `head -5 README.md`
Expected: Shows `# Roche` header and badge lines

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: write complete README with CLI reference and SDK examples

Add installation, quickstart, CLI reference table, Python SDK usage,
security defaults, and development instructions."
```

---

## Chunk 3: CI/CD + Publishing Preparation

### Task 5: Create GitHub Actions CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create directory and workflow file**

```bash
mkdir -p .github/workflows
```

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  rust:
    name: Rust (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy -- -D warnings
      - run: cargo build
      - run: cargo test

  python:
    name: Python SDK
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - run: pip install -e "sdk/python[dev]"
      - run: pytest sdk/python/tests/ -v
```

- [ ] **Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" 2>/dev/null || python3 -c "import json, sys; print('YAML check skipped — no PyYAML')"`
Expected: No errors (or skipped message)

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add GitHub Actions workflow for Rust and Python

Rust job: fmt, clippy, build, test on ubuntu + macos with rust-cache.
Python job: install SDK and run pytest on ubuntu."
```

---

### Task 6: Publishing preparation — CHANGELOG + metadata

**Files:**
- Create: `CHANGELOG.md`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create CHANGELOG.md**

Create `CHANGELOG.md` at project root:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## 0.1.0 (Unreleased)

### Added

- Docker sandbox provider (create, exec, destroy, list) via Docker CLI
- CLI (`roche`) with AI-safe defaults (no network, readonly FS, 300s timeout)
- Python SDK (`roche-python`) with subprocess-based client and `Sandbox` context manager
- Resource limits: memory, CPU, timeout, PID limit
- Security hardening: `--security-opt no-new-privileges`, `--pids-limit 256`
- Environment variable support (`--env KEY=VALUE`)
```

- [ ] **Step 2: Add keywords to workspace Cargo.toml**

In `Cargo.toml` (workspace root), add under `[workspace.package]`:

```toml
keywords = ["sandbox", "docker", "ai", "agent"]
```

- [ ] **Step 3: Verify version alignment**

Run: `grep -r 'version.*0\.1\.0' Cargo.toml crates/*/Cargo.toml sdk/python/pyproject.toml`
Expected: All show `0.1.0`

- [ ] **Step 4: Verify both crates have description fields**

Run: `grep 'description' crates/roche-core/Cargo.toml crates/roche-cli/Cargo.toml`
Expected: Both have `description = "..."`

- [ ] **Step 5: Verify LICENSE exists**

Run: `test -f LICENSE && echo "OK" || echo "MISSING"`
Expected: `OK`

- [ ] **Step 6: Commit**

```bash
git add CHANGELOG.md Cargo.toml
git commit -m "chore: add CHANGELOG and publishing metadata

Add CHANGELOG.md with 0.1.0 features. Add keywords to workspace
Cargo.toml. All versions aligned at 0.1.0."
```

---

### Task 7: Final verification

- [ ] **Step 1: Run full Rust build and test suite**

Run: `cargo build && cargo test`
Expected: All pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Check formatting**

Run: `cargo fmt --check`
Expected: No formatting issues

- [ ] **Step 4: Run Python tests**

Run: `cd sdk/python && python -m pytest tests/ -v`
Expected: All tests pass

- [ ] **Step 5: Verify CLI help shows --env flag**

Run: `cargo run -- create --help`
Expected: Output includes `--env <KEY=VALUE>` line
