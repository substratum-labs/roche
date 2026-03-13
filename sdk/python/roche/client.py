"""Roche Python SDK client — wraps the roche CLI binary."""

from __future__ import annotations

import json
import subprocess
from typing import Any

from .errors import RocheError
from .types import ExecOutput, Mount, SandboxConfig


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

        for key, value in config.env.items():
            cmd.extend(["--env", f"{key}={value}"])

        for mount in config.mounts:
            mode = "ro" if mount.readonly else "rw"
            cmd.extend(["--mount", f"{mount.host_path}:{mount.container_path}:{mode}"])

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

    def copy_to(self, sandbox_id: str, local_path: str, sandbox_path: str) -> None:
        """Copy a file from host to sandbox."""
        self._run(["cp", local_path, f"{sandbox_id}:{sandbox_path}"])

    def copy_from(self, sandbox_id: str, sandbox_path: str, local_path: str) -> None:
        """Copy a file from sandbox to host."""
        self._run(["cp", f"{sandbox_id}:{sandbox_path}", local_path])

    def pause(self, sandbox_id: str) -> None:
        """Pause a sandbox."""
        self._run(["pause", sandbox_id])

    def unpause(self, sandbox_id: str) -> None:
        """Unpause a sandbox."""
        self._run(["unpause", sandbox_id])

    def gc(self, dry_run: bool = False) -> None:
        """Garbage collect expired sandboxes."""
        cmd = ["gc"]
        if dry_run:
            cmd.append("--dry-run")
        self._run(cmd)

    def create_many(self, config: SandboxConfig | None = None, count: int = 1) -> list[str]:
        """Create multiple sandboxes. Returns list of sandbox IDs."""
        config = config or SandboxConfig()
        cmd = [
            "create",
            "--provider", config.provider,
            "--image", config.image,
            "--timeout", str(config.timeout),
            "--count", str(count),
        ]

        if config.memory:
            cmd.extend(["--memory", config.memory])
        if config.cpus is not None:
            cmd.extend(["--cpus", str(config.cpus)])
        if config.network:
            cmd.append("--network")
        if config.writable:
            cmd.append("--writable")

        for key, value in config.env.items():
            cmd.extend(["--env", f"{key}={value}"])

        for mount in config.mounts:
            mode = "ro" if mount.readonly else "rw"
            cmd.extend(["--mount", f"{mount.host_path}:{mount.container_path}:{mode}"])

        result = self._run(cmd)
        return [line for line in result.stdout.strip().split("\n") if line]

    def destroy_many(self, sandbox_ids: list[str]) -> None:
        """Destroy multiple sandboxes."""
        self._run(["destroy", *sandbox_ids])

    def destroy_all(self) -> None:
        """Destroy all roche-managed sandboxes."""
        self._run(["destroy", "--all"])


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

    def copy_to(self, local_path: str, sandbox_path: str) -> None:
        """Copy a file from host to this sandbox."""
        self._client.copy_to(self.id, local_path, sandbox_path)

    def copy_from(self, sandbox_path: str, local_path: str) -> None:
        """Copy a file from this sandbox to host."""
        self._client.copy_from(self.id, sandbox_path, local_path)

    def pause(self) -> None:
        """Pause this sandbox."""
        self._client.pause(self.id)

    def unpause(self) -> None:
        """Unpause this sandbox."""
        self._client.unpause(self.id)
