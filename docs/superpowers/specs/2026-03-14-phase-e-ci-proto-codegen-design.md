# Phase E2: Proto Codegen CI — Design Spec

## Goal

Ensure CI can build and test both SDKs by installing protoc and running proto codegen scripts before tests.

## Current State

- CI has `rust` job (fmt, clippy, build, test) and `python` job (install, pytest)
- No TypeScript SDK CI job exists
- Proto generated files are gitignored in both SDKs
- Both SDKs have `scripts/proto-gen.sh` that require `protoc` on PATH

## Changes

### 1. Add protoc installation to Python SDK job

Insert before `pip install`:
```yaml
- name: Install protoc
  uses: arduino/setup-protoc@v3
  with:
    version: "28.x"
- run: cd sdk/python && bash scripts/proto-gen.sh
```

### 2. Add TypeScript SDK CI job

New job `typescript` parallel to `python`:
```yaml
typescript:
  name: TypeScript SDK
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
      with:
        node-version: "20"
    - uses: arduino/setup-protoc@v3
      with:
        version: "28.x"
    - run: cd sdk/typescript && npm ci
    - run: cd sdk/typescript && bash scripts/proto-gen.sh
    - run: cd sdk/typescript && npm test
```

### 3. Keep Rust job unchanged

Rust proto codegen is handled by `build.rs` (tonic-build), no changes needed.

## Module Decomposition

| File | Change |
|------|--------|
| `.github/workflows/ci.yml` | Add protoc setup to Python job, add TypeScript SDK job |
