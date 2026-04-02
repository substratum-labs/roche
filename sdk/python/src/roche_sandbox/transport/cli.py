# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

import asyncio
import json
import time

from roche_sandbox.daemon import _find_bundled_binary
from roche_sandbox.errors import (
    ProviderUnavailable, RocheError, SandboxNotFound, SandboxPaused,
    TimeoutError, UnsupportedOperation,
)
from roche_sandbox.trace import ExecutionTrace, ResourceUsage
from roche_sandbox.types import ExecEvent, ExecOutput, SandboxConfig, SandboxInfo


class CliTransport:
    def __init__(self, binary: str = "roche"):
        bundled = _find_bundled_binary("roche")
        self._binary = str(bundled) if bundled else binary

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
        for host in config.network_allowlist:
            args.extend(["--network-allow", host])
        for path in config.fs_paths:
            args.extend(["--fs-path", path])
        stdout, _ = await self._run(args)
        return stdout.strip()

    async def exec(self, sandbox_id: str, command: list[str], provider: str, timeout_secs: int | None = None, trace_level: str | None = None, idempotency_key: str | None = None) -> ExecOutput:
        args = ["exec", "--sandbox", sandbox_id]
        if timeout_secs is not None:
            args.extend(["--timeout", str(timeout_secs)])
        args.extend(["--", *command])
        t0 = time.monotonic()
        stdout, stderr, returncode = await self._run_unchecked(args)
        duration = time.monotonic() - t0
        if returncode != 0 and self._is_roche_error(stderr):
            raise self._map_cli_error(stderr)
        trace = None
        if trace_level is not None and trace_level != "off":
            trace = ExecutionTrace(
                duration_secs=round(duration, 3),
                resource_usage=ResourceUsage(
                    peak_memory_bytes=0, cpu_time_secs=0.0,
                    network_rx_bytes=0, network_tx_bytes=0,
                ),
            )
        return ExecOutput(exit_code=returncode, stdout=stdout, stderr=stderr, trace=trace)

    async def exec_stream(self, sandbox_id: str, command: list[str], provider: str, timeout_secs: int | None = None, trace_level: str | None = None):
        """CLI fallback: run exec and yield events from the result."""
        result = await self.exec(sandbox_id, command, provider, timeout_secs, trace_level=trace_level)
        if result.stdout:
            yield ExecEvent(type="output", stream="stdout", data=result.stdout.encode())
        if result.stderr:
            yield ExecEvent(type="output", stream="stderr", data=result.stderr.encode())
        yield ExecEvent(type="result", exit_code=result.exit_code, trace=result.trace)

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

    async def history(self, sandbox_id):
        raise UnsupportedOperation("Execution history requires the daemon (roched)")

    async def pool_status(self):
        raise UnsupportedOperation("Pool management requires the daemon (roched)")

    async def pool_warmup(self):
        raise UnsupportedOperation("Pool management requires the daemon (roched)")

    async def pool_drain(self):
        raise UnsupportedOperation("Pool management requires the daemon (roched)")

    async def create_session(self, sandbox_id, provider, permissions=None, budget=None):
        raise UnsupportedOperation("Session management requires the daemon (roched)")

    async def destroy_session(self, session_id):
        raise UnsupportedOperation("Session management requires the daemon (roched)")

    async def list_sessions(self):
        raise UnsupportedOperation("Session management requires the daemon (roched)")

    async def update_permissions(self, session_id, change):
        raise UnsupportedOperation("Session management requires the daemon (roched)")

    async def analyze_intent(self, code, language):
        raise UnsupportedOperation("Intent analysis requires the daemon (roched)")

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
            raise ProviderUnavailable(
                f"Roche CLI not found: '{self._binary}'\n\n"
                "Install the Roche CLI using one of these methods:\n"
                "  pip install roche-sandbox[cli]   # auto-download prebuilt binary\n"
                "  cargo install roche-cli          # build from source\n"
                "  # or download from https://github.com/substratum-labs/roche/releases"
            )
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
