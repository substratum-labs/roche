# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

class RocheError(Exception):
    def __init__(self, message: str):
        super().__init__(message)


class SandboxNotFound(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Sandbox not found: {detail}")


class SandboxPaused(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Sandbox is paused: {detail}")


class ProviderUnavailable(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Provider unavailable: {detail}")


class TimeoutError(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Operation timed out: {detail}")


class UnsupportedOperation(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Unsupported operation: {detail}")
