# Phase A: Complete MVP — Design Spec

**版本:** 0.1.0
**日期:** 2026-03-13
**状态:** Approved

---

## 1. Overview

Phase A completes the Roche MVP by adding the missing `--env` CLI flag, writing project documentation (README), setting up CI/CD, and preparing for package publishing. These are the final steps before Roche 0.1.0 can be considered release-ready.

**Scope:** 4 work items, all additive — no architectural changes.

---

## 2. `--env` CLI Flag

### Problem

`SandboxConfig` already supports `env: HashMap<String, String>`, and `DockerProvider` already passes env vars to `docker create` via `-e` flags. However, the CLI does not expose this — users cannot set environment variables when creating a sandbox.

### Design

Add a repeatable `--env` flag to the `Create` subcommand:

```
roche create --env FOO=bar --env DB_HOST=localhost
```

**CLI definition:**

```rust
/// Environment variables (KEY=VALUE, repeatable)
#[arg(long = "env", value_name = "KEY=VALUE")]
env: Vec<String>,
```

**Parsing:** A helper function `parse_env_vars` splits each `KEY=VALUE` string at the first `=`:

```rust
fn parse_env_vars(pairs: &[String]) -> Result<HashMap<String, String>, String> {
    pairs.iter().map(|s| {
        let (k, v) = s.split_once('=')
            .ok_or_else(|| format!("invalid env format: {s} (expected KEY=VALUE)"))?;
        Ok((k.to_string(), v.to_string()))
    }).collect()
}
```

Error on malformed input (no `=` sign). Values may contain `=` (split at first only).

### Python SDK Passthrough

The Python SDK's `Roche.create()` must also forward env vars. `SandboxConfig` already has an `env: dict[str, str]` field (default `{}`). Add to `client.py`'s `create()`:

```python
for key, value in config.env.items():
    cmd.extend(["--env", f"{key}={value}"])
```

### Testing

- Unit test for `parse_env_vars`: happy path (`FOO=bar`), value with `=` (`A=b=c`), malformed input (no `=`)
- CLI integration: verify `--env FOO=bar` is parsed and forwarded to `SandboxConfig.env`

**Files touched:**
- `crates/roche-cli/src/main.rs` — add `env` field to `Create`, parse in `run()`
- `sdk/python/roche/client.py` — forward `config.env` as `--env` flags

---

## 3. README

### Structure

Single `README.md` at project root. Sections:

1. **Header** — project name, one-line description, badges (CI status, license)
2. **What is Roche** — 2-3 sentences: the N×M problem, Roche as unified abstraction
3. **Features** — bullet list: AI-safe defaults, multi-provider, CLI + SDK
4. **Quick Start** — install from source (`cargo install --path crates/roche-cli`), create/exec/destroy example
5. **CLI Reference** — table of all commands and their flags
6. **Python SDK** — `pip install -e sdk/python`, code example with `Roche` client and `Sandbox` context manager
7. **Security Defaults** — table: network=off, fs=readonly, timeout=300s
8. **License** — Apache-2.0

**Files touched:**
- `README.md` — replace existing placeholder

---

## 4. CI/CD (GitHub Actions)

### Workflow

Single file `.github/workflows/ci.yml`, triggered on:
- Push to `main`
- Pull requests targeting `main`

### Jobs

**Job 1: `rust`**
- Matrix: `[ubuntu-latest, macos-latest]`
- Steps:
  1. Checkout
  2. Install Rust stable
  3. Cache Rust build artifacts (`Swatinem/rust-cache@v2`)
  4. `cargo fmt --check`
  5. `cargo clippy -- -D warnings`
  6. `cargo build`
  7. `cargo test`

**Job 2: `python`**
- Runs on: `ubuntu-latest`
- Python: 3.12
- Steps:
  1. Checkout
  2. Set up Python 3.12
  3. `pip install -e "sdk/python[dev]"`
  4. `pytest sdk/python/tests/ -v`

**Not included:** Docker integration tests (require running daemon + built binary — defer to Phase 2).

**Files touched:**
- `.github/workflows/ci.yml` — new file

---

## 5. Publishing Preparation

Lightweight preparation — no actual publishing.

### 5.1 CHANGELOG

Create `CHANGELOG.md` at project root:

```markdown
# Changelog

## 0.1.0 (Unreleased)

### Added
- Docker sandbox provider (create, exec, destroy, list)
- CLI with AI-safe defaults (no network, readonly FS, 300s timeout)
- Python SDK (`roche-python`) with subprocess-based client
- Resource limits: memory, CPU, timeout, PID
- Environment variable support (`--env KEY=VALUE`)
```

### 5.2 Version Alignment

Ensure all version strings read `0.1.0`:
- `crates/roche-core/Cargo.toml`
- `crates/roche-cli/Cargo.toml`
- `sdk/python/pyproject.toml`

### 5.3 Cargo.toml Metadata

The workspace `Cargo.toml` already defines `version`, `edition`, `license`, and `repository`. Add `keywords` to the workspace metadata:

```toml
# In [workspace.package]
keywords = ["sandbox", "docker", "ai", "agent"]
```

Verify each crate's `Cargo.toml` has a `description` field (crate-specific, not inherited).

### 5.4 License File

Verify `LICENSE` file exists at project root (already confirmed present).

**Files touched:**
- `CHANGELOG.md` — new file
- `Cargo.toml` (workspace root) — add `keywords`
- `crates/roche-core/Cargo.toml` — verify `description`
- `crates/roche-cli/Cargo.toml` — verify `description`

---

## 6. Out of Scope

- Actual `cargo publish` or PyPI upload
- Docker integration tests in CI
- New providers (Firecracker, WASM)
- Daemon mode / gRPC
- TypeScript SDK
