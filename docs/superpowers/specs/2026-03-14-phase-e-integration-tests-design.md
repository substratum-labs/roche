# Phase E3: Integration Tests — Design Spec

## Goal

Add end-to-end tests that exercise real sandbox lifecycle (create → exec → destroy) through the Docker provider, both directly and via SDKs.

## Architecture

```
crates/roche-core/tests/
└── docker_integration.rs     # Rust: DockerProvider e2e

sdk/python/tests/
└── integration/
    ├── conftest.py           # fixtures, skip-if-no-docker
    └── test_cli_e2e.py       # Python SDK via CLI transport

sdk/typescript/test/
└── integration/
    ├── setup.ts              # skip-if-no-docker helper
    └── cli-e2e.test.ts       # TypeScript SDK via CLI transport
```

## Test Gating

All integration tests are skipped when Docker is not available:

- **Rust**: `#[ignore]` attribute — run with `cargo test -- --ignored`
- **Python**: `@pytest.mark.integration` + `conftest.py` that auto-skips if `docker info` fails
- **TypeScript**: Custom jest config or `describe.skip` based on Docker availability check

## Test Cases

### Rust (DockerProvider direct)
1. `create` → verify sandbox ID returned
2. `exec` echo command → verify stdout
3. `list` → verify sandbox appears
4. `pause` → `exec` fails with `Paused` → `unpause` → `exec` succeeds
5. `destroy` → `list` → verify sandbox gone
6. `gc` with expired sandbox → verify cleanup

### Python SDK (CLI transport)
1. `Roche(mode="direct")` → `create()` → `exec()` → `destroy()`
2. Context manager auto-destroy
3. `list()` returns created sandbox
4. Error handling: exec on destroyed sandbox raises `SandboxNotFound`

### TypeScript SDK (CLI transport)
1. `new Roche({ mode: "direct" })` → `createSandbox()` → `exec()` → `destroy()`
2. `using` auto-dispose
3. `list()` returns created sandbox
4. Error handling: exec on destroyed sandbox throws `SandboxNotFound`

## CI Integration

Add to `.github/workflows/ci.yml`:

```yaml
integration:
  name: Integration Tests
  runs-on: ubuntu-latest
  needs: [rust, python, typescript]
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo test -- --ignored
    - uses: actions/setup-python@v5
      with:
        python-version: "3.12"
    - run: pip install -e "sdk/python[dev]"
    - run: pytest sdk/python/tests/integration/ -v
    - uses: actions/setup-node@v4
      with:
        node-version: "20"
    - run: cd sdk/typescript && npm ci && npm run test:integration
```

## Non-Goals

- gRPC transport integration tests (requires daemon running)
- Firecracker integration tests (requires Linux + KVM)
- WASM integration tests (no external deps, covered by unit tests)
