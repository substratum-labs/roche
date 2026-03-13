"""Exception types for the Roche Python SDK."""


class RocheError(Exception):
    """Base exception for Roche operations."""

    def __init__(self, message: str, stderr: str = ""):
        super().__init__(message)
        self.stderr = stderr
