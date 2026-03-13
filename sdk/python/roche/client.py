"""Roche Python SDK client — wraps the roche CLI binary."""

from __future__ import annotations

import json
import subprocess
from typing import Any

from .errors import RocheError
from .types import ExecOutput, SandboxConfig


class Roche:
    """Client for the Roche sandbox orchestrator.

    Wraps the `roche` CLI binary via subprocess calls.
    """

    def __init__(self, binary: str = "roche"):
        self._binary = binary

    def _run(self, args: list[str], check: bool = True) -> subprocess.CompletedProcess[str]:
        try:
            return subprocess.run(
                [self._binary, *args],
                capture_output=True,
                text=True,
                check=check,
            )
        except FileNotFoundError:
            raise RocheError(
                f"Roche binary not found: {self._binary}. "
                "Install with: cargo install --path crates/roche-cli"
            )
        except subprocess.CalledProcessError as e:
            raise RocheError(e.stderr.strip(), stderr=e.stderr)

    def create(self, config: SandboxConfig | None = None) -> str:
        """Create a new sandbox. Returns the sandbox ID."""
        config = config or SandboxConfig()
        cmd = [
            "create",
            "--provider", config.provider,
            "--image", config.image,
            "--timeout", str(config.timeout),
        ]

        if config.memory:
            cmd.extend(["--memory", config.memory])
        if config.cpus is not None:
            cmd.extend(["--cpus", str(config.cpus)])
        if config.network:
            cmd.append("--network")
        if config.writable:
            cmd.append("--writable")

        result = self._run(cmd)
        return result.stdout.strip()

    def exec(
        self,
        sandbox_id: str,
        command: list[str],
        timeout: int | None = None,
    ) -> ExecOutput:
        """Execute a command inside a sandbox."""
        cmd = ["exec", "--sandbox", sandbox_id]
        if timeout is not None:
            cmd.extend(["--timeout", str(timeout)])
        cmd.extend(command)

        result = self._run(cmd, check=False)
        return ExecOutput(
            exit_code=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )

    def destroy(self, sandbox_id: str) -> None:
        """Destroy a sandbox."""
        self._run(["destroy", sandbox_id])

    def list(self) -> list[dict[str, Any]]:
        """List all active sandboxes."""
        result = self._run(["list", "--json"])
        return json.loads(result.stdout)


class Sandbox:
    """Context manager for a single sandbox. Auto-creates and destroys."""

    def __init__(self, client: Roche | None = None, config: SandboxConfig | None = None):
        self._client = client or Roche()
        self._config = config or SandboxConfig()
        self._id: str | None = None

    def __enter__(self) -> Sandbox:
        self._id = self._client.create(self._config)
        return self

    def __exit__(self, *exc: object) -> None:
        if self._id:
            self._client.destroy(self._id)
            self._id = None

    @property
    def id(self) -> str:
        if self._id is None:
            raise RocheError("Sandbox not created yet")
        return self._id

    def exec(self, command: list[str], timeout: int | None = None) -> ExecOutput:
        """Execute a command in this sandbox."""
        return self._client.exec(self.id, command, timeout=timeout)
