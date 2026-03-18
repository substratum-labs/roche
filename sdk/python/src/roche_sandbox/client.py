# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from roche_sandbox.daemon import detect_daemon
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.transport.cli import CliTransport
from roche_sandbox.transport.grpc import GrpcTransport
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo

if TYPE_CHECKING:
    from roche_sandbox.transport import Transport


class AsyncRoche:
    def __init__(
        self,
        *,
        mode: str = "auto",
        daemon_port: int | None = None,
        provider: str = "docker",
        binary: str = "roche",
        transport: Transport | None = None,
    ):
        self._provider = provider
        if transport is not None:
            self._transport = transport
        elif mode == "direct":
            self._transport = CliTransport(binary=binary)
        elif daemon_port is not None:
            self._transport = GrpcTransport(port=daemon_port)
        else:
            daemon = detect_daemon()
            if daemon is not None:
                self._transport = GrpcTransport(port=daemon["port"])
            else:
                self._transport = CliTransport(binary=binary)

    @property
    def transport(self) -> Transport:
        return self._transport

    async def create(
        self,
        *,
        provider: str | None = None,
        image: str = "python:3.12-slim",
        memory: str | None = None,
        cpus: float | None = None,
        timeout_secs: int = 300,
        network: bool = False,
        writable: bool = False,
        env: dict[str, str] | None = None,
        mounts: list | None = None,
        kernel: str | None = None,
        rootfs: str | None = None,
    ) -> AsyncSandbox:
        effective_provider = provider or self._provider
        config = SandboxConfig(
            provider=effective_provider,
            image=image,
            memory=memory,
            cpus=cpus,
            timeout_secs=timeout_secs,
            network=network,
            writable=writable,
            env=env or {},
            mounts=mounts or [],
            kernel=kernel,
            rootfs=rootfs,
        )
        sandbox_id = await self._transport.create(config, effective_provider)
        return AsyncSandbox(sandbox_id, effective_provider, self._transport)

    async def create_id(self, **kwargs) -> str:
        sb = await self.create(**kwargs)
        return sb.id

    async def exec(
        self, sandbox_id: str, command: list[str], timeout_secs: int | None = None, trace_level: str | None = None
    ) -> ExecOutput:
        return await self._transport.exec(sandbox_id, command, self._provider, timeout_secs, trace_level=trace_level)

    async def destroy(self, sandbox_id: str) -> None:
        await self._transport.destroy([sandbox_id], self._provider)

    async def list(self) -> list[SandboxInfo]:
        return await self._transport.list(self._provider)

    async def gc(self, dry_run: bool = False, all: bool = False) -> list[str]:
        return await self._transport.gc(self._provider, dry_run, all)


class Roche:
    def __init__(self, **kwargs):
        self._async = AsyncRoche(**kwargs)

    def create(self, **kwargs) -> Sandbox:
        sb = asyncio.run(self._async.create(**kwargs))
        return Sandbox(sb.id, sb.provider, self._async.transport)

    def create_id(self, **kwargs) -> str:
        return asyncio.run(self._async.create_id(**kwargs))

    def exec(
        self, sandbox_id: str, command: list[str], timeout_secs: int | None = None, trace_level: str | None = None
    ) -> ExecOutput:
        return asyncio.run(self._async.exec(sandbox_id, command, timeout_secs, trace_level=trace_level))

    def destroy(self, sandbox_id: str) -> None:
        asyncio.run(self._async.destroy(sandbox_id))

    def list(self) -> list[SandboxInfo]:
        return asyncio.run(self._async.list())

    def gc(self, dry_run: bool = False, all: bool = False) -> list[str]:
        return asyncio.run(self._async.gc(dry_run, all))
