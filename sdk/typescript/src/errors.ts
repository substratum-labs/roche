// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

export class RocheError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RocheError";
  }
}

export class SandboxNotFound extends RocheError {
  constructor(sandboxId: string) {
    super(`Sandbox not found: ${sandboxId}`);
    this.name = "SandboxNotFound";
  }
}

export class SandboxPaused extends RocheError {
  constructor(sandboxId: string) {
    super(`Sandbox is paused: ${sandboxId}`);
    this.name = "SandboxPaused";
  }
}

export class ProviderUnavailable extends RocheError {
  constructor(detail: string) {
    super(`Provider unavailable: ${detail}`);
    this.name = "ProviderUnavailable";
  }
}

export class TimeoutError extends RocheError {
  constructor(detail: string) {
    super(`Operation timed out: ${detail}`);
    this.name = "TimeoutError";
  }
}

export class UnsupportedOperation extends RocheError {
  constructor(operation: string) {
    super(`Unsupported operation: ${operation}`);
    this.name = "UnsupportedOperation";
  }
}
