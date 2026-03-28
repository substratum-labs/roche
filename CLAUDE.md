# Roche — Claude Code Guidelines

## Project Overview

Roche is a universal sandbox orchestrator for AI agents. It provides a single abstraction over multiple sandbox providers (Docker, Firecracker, WASM) with AI-optimized security defaults.

**Named after Édouard Roche** — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

## Tech Stack

- Rust (2021 edition)
- Workspace: `roche-core` (lib) + `roche-cli` (bin)
- `clap` for CLI argument parsing
- `tokio` for async runtime
- `serde` / `serde_json` for serialization
- `thiserror` for error types

## Commands

```bash
cargo build              # Build all crates
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt --check        # Check formatting
cargo run -- --help      # Run CLI
```

## Architecture

```
crates/
├── roche-core/          # Library: traits, types, providers
│   └── src/
│       ├── lib.rs
│       ├── types.rs     # SandboxConfig, ExecOutput, etc.
│       └── provider/
│           ├── mod.rs   # SandboxProvider trait + ProviderError
│           └── docker.rs
└── roche-cli/           # Binary: clap CLI
    └── src/
        └── main.rs      # create/exec/destroy/list subcommands
```

## Key Design Decisions

- **AI-safe defaults**: network=off, filesystem=readonly, timeout=300s
- **Provider trait**: `SandboxProvider` with four async methods: `create`, `exec`, `destroy`, `list`
- **Docker first**: MVP uses Docker CLI via `tokio::process::Command`
- **No framework coupling**: Roche is agent-framework-agnostic

## Code Conventions

- Use `thiserror` for all error types
- All public types derive `Debug`, `Clone`, `Serialize`, `Deserialize` where applicable
- Provider implementations go in `provider/` submodules
- Prefer `&str` over `String` in function parameters where possible
- Use `#[tokio::main]` in CLI, `async fn` in library code

## Sister Projects

- **Castor** (`../castor/`) — Security microkernel for LLM agents. Roche does not depend on Castor.
- **Tiphys** (`../tiphys/`) — Digital Life Form agent. Bridges Castor + Roche for safe agent execution.
- **castor-internal** (`../castor-internal/`) — Private design docs, cross-project status.
  - Read `status/PROGRESS.md` for current cross-project state.
