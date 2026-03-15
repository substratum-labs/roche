# Contributing to Roche

Thanks for your interest in contributing to Roche. This guide covers everything you need to get started.

## Development Setup

### Rust

1. Install Rust via [rustup](https://rustup.rs/) (stable toolchain).
2. Install Docker for running integration tests.
3. Clone the repo and build:

```bash
git clone https://github.com/substratum-labs/roche.git
cd roche
cargo build
```

### Python SDK

```bash
pip install -e "sdk/python[dev]"
```

### TypeScript SDK

```bash
cd sdk/typescript
npm ci
```

### Proto Codegen

Both SDKs include a `scripts/proto-gen.sh` script for regenerating gRPC bindings. You need `protoc` (the Protocol Buffers compiler) installed on your system.

```bash
# Python
cd sdk/python && bash scripts/proto-gen.sh

# TypeScript
cd sdk/typescript && bash scripts/proto-gen.sh
```

## Build and Test

### Rust

```bash
cargo build              # Build all crates
cargo test               # Run all tests
cargo clippy -- -D warnings   # Lint (must pass with zero warnings)
cargo fmt --check        # Check formatting
```

### Python SDK

```bash
pytest sdk/python/tests/ -v
```

### TypeScript SDK

```bash
cd sdk/typescript && npm test
```

## Making Changes

### Workflow

1. Fork the repository.
2. Create a feature branch from `main`.
3. Make your changes.
4. Ensure all tests pass and lints are clean.
5. Open a pull request against `main`.

### Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation only
- `test:` adding or updating tests
- `ci:` CI/CD changes
- `refactor:` code restructuring with no behavior change

Examples:

```
feat(core): add WASM provider support
fix(docker): handle container timeout on slow hosts
docs: update CLI usage examples
```

### Code Style

- **Rust**: Format with `rustfmt`. Run `cargo fmt` before committing.
- **Python**: Standard formatting (black/ruff recommended).
- **TypeScript**: Standard formatting (prettier recommended).

### Testing Expectations

- New features should include tests.
- Bug fixes should include a regression test where practical.
- Integration tests that require Docker should be gated behind `#[cfg(feature = "integration")]` or equivalent.

## Project Structure

```
crates/
  roche-core/       # Library: traits, types, providers
  roche-cli/        # Binary: CLI interface
  roche-daemon/     # gRPC daemon
sdk/
  python/           # Python SDK
  typescript/       # TypeScript SDK
```

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
