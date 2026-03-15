# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## 0.1.0 (2025-03-15)

### Added

- Docker sandbox provider (create, exec, destroy, list) via Docker CLI
- CLI (`roche`) with AI-safe defaults (no network, readonly FS, 300s timeout)
- Python SDK (`roche-python`) with subprocess-based client and `Sandbox` context manager
- Resource limits: memory, CPU, timeout, PID limit
- Security hardening: `--security-opt no-new-privileges`, `--pids-limit 256`
- Environment variable support (`--env KEY=VALUE`)
