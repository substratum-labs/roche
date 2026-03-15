# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

import asyncio
import json

from roche_sandbox.errors import (
    ProviderUnavailable, RocheError, SandboxNotFound, SandboxPaused,
    TimeoutError, UnsupportedOperation,
)
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo


class CliTransport:
    def __init__(self, binary: str = "roche"):
        self._binary = binary

    async def create(self, config: SandboxConfig, provider: str) -> str:
        args = [
            "create", "--provider", provider,
            "--image", config.image,
            "--timeout", str(config.timeout_secs),
        ]
        if config.memory:
            args.extend(["--memory", config.memory])
        if config.cpus is not None:
            args.extend(["--cpus", str(config.cpus)])
        if config.network:
            args.append("--network")
        if config.writable:
            args.append("--writable")
        for k, v in config.env.items():
            args.extend(["--env", f"{k}={v}"])
        for m in config.mounts:
            mode = "ro" if m.readonly else "rw"
            args.extend(["--mount", f"{m.host_path}:{m.container_path}:{mode}"])
        if config.kernel:
            args.extend(["--kernel", config.kernel])
        if config.rootfs:
            args.extend(["--rootfs", config.rootfs])
        stdout, _ = await self._run(args)
        return stdout.strip()

    async def exec(self, sandbox_id: str, command: list[str], provider: str, timeout_secs: int | None = None) -> ExecOutput:
        args = ["exec", "--sandbox", sandbox_id]
        if timeout_secs is not None:
            args.extend(["--timeout", str(timeout_secs)])
        args.extend(["--", *command])
        stdout, stderr, returncode = await self._run_unchecked(args)
        if returncode != 0 and self._is_roche_error(stderr):
            raise self._map_cli_error(stderr)
        return ExecOutput(exit_code=returncode, stdout=stdout, stderr=stderr)

    async def destroy(self, sandbox_ids: list[str], provider: str, all: bool = False) -> list[str]:
        args = ["destroy"]
        if all:
            args.append("--all")
        else:
            args.extend(sandbox_ids)
        stdout, _ = await self._run(args)
        return [line for line in stdout.strip().split("\n") if line]

    async def list(self, provider: str) -> list[SandboxInfo]:
        stdout, _ = await self._run(["list", "--json"])
        raw = json.loads(stdout)
        return [
            SandboxInfo(id=s["id"], status=s["status"], provider=s["provider"],
                       image=s["image"], expires_at=s.get("expires_at"))
            for s in raw
        ]

    async def pause(self, sandbox_id: str, provider: str) -> None:
        await self._run(["pause", sandbox_id])

    async def unpause(self, sandbox_id: str, provider: str) -> None:
        await self._run(["unpause", sandbox_id])

    async def gc(self, provider: str, dry_run: bool = False, all: bool = False) -> list[str]:
        args = ["gc"]
        if dry_run:
            args.append("--dry-run")
        if all:
            args.append("--all")
        stdout, _ = await self._run(args)
        return [line for line in stdout.strip().split("\n") if line]

    async def copy_to(self, sandbox_id: str, host_path: str, sandbox_path: str, provider: str) -> None:
        await self._run(["cp", host_path, f"{sandbox_id}:{sandbox_path}"])

    async def copy_from(self, sandbox_id: str, sandbox_path: str, host_path: str, provider: str) -> None:
        await self._run(["cp", f"{sandbox_id}:{sandbox_path}", host_path])

    async def _run(self, args: list[str]) -> tuple[str, str]:
        stdout, stderr, returncode = await self._run_unchecked(args)
        if returncode != 0:
            raise self._map_cli_error(stderr)
        return stdout, stderr

    async def _run_unchecked(self, args: list[str]) -> tuple[str, str, int]:
        try:
            proc = await asyncio.create_subprocess_exec(
                self._binary, *args,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
        except FileNotFoundError:
            raise ProviderUnavailable(f"Roche binary not found: {self._binary}")
        stdout_bytes, stderr_bytes = await proc.communicate()
        return stdout_bytes.decode(), stderr_bytes.decode(), proc.returncode or 0

    def _is_roche_error(self, stderr: str) -> bool:
        return stderr.lstrip().startswith("Error: ")

    def _map_cli_error(self, stderr: str) -> RocheError:
        lower = stderr.lower()
        if "not found" in lower:
            return SandboxNotFound(stderr.strip())
        if "paused" in lower:
            return SandboxPaused(stderr.strip())
        if "timeout" in lower:
            return TimeoutError(stderr.strip())
        if "unsupported" in lower:
            return UnsupportedOperation(stderr.strip())
        if "unavailable" in lower or "connection refused" in lower:
            return ProviderUnavailable(stderr.strip())
        return RocheError(stderr.strip())
