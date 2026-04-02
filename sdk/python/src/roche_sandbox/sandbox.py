# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
from typing import TYPE_CHECKING

from roche_sandbox.types import ExecEvent, ExecOutput, ExecRecord

if TYPE_CHECKING:
    from roche_sandbox.transport import Transport


class AsyncSandbox:
    def __init__(self, id: str, provider: str, transport: Transport):
        self._id = id
        self._provider = provider
        self._transport = transport

    @property
    def id(self) -> str:
        return self._id

    @property
    def provider(self) -> str:
        return self._provider

    async def exec(self, command: list[str], timeout_secs: int | None = None, trace_level: str | None = None, idempotency_key: str | None = None) -> ExecOutput:
        return await self._transport.exec(self._id, command, self._provider, timeout_secs, trace_level=trace_level, idempotency_key=idempotency_key)

    async def exec_stream(self, command: list[str], timeout_secs: int | None = None, trace_level: str | None = None) -> AsyncIterator[ExecEvent]:
        """Stream exec events (output chunks, heartbeats, final result) as an async iterator."""
        async for event in self._transport.exec_stream(self._id, command, self._provider, timeout_secs, trace_level=trace_level):
            yield event

    async def pause(self) -> None:
        await self._transport.pause(self._id, self._provider)

    async def unpause(self) -> None:
        await self._transport.unpause(self._id, self._provider)

    async def destroy(self) -> None:
        await self._transport.destroy([self._id], self._provider)

    async def copy_to(self, host_path: str, sandbox_path: str) -> None:
        await self._transport.copy_to(self._id, host_path, sandbox_path, self._provider)

    async def copy_from(self, sandbox_path: str, host_path: str) -> None:
        await self._transport.copy_from(self._id, sandbox_path, host_path, self._provider)

    async def history(self) -> list[ExecRecord]:
        """Return execution history for this sandbox."""
        return await self._transport.history(self._id)

    async def __aenter__(self) -> AsyncSandbox:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.destroy()


class Sandbox:
    def __init__(self, id: str, provider: str, transport: Transport):
        self._inner = AsyncSandbox(id, provider, transport)

    @property
    def id(self) -> str:
        return self._inner.id

    @property
    def provider(self) -> str:
        return self._inner.provider

    def exec(self, command: list[str], timeout_secs: int | None = None, trace_level: str | None = None, idempotency_key: str | None = None) -> ExecOutput:
        return asyncio.run(self._inner.exec(command, timeout_secs, trace_level=trace_level, idempotency_key=idempotency_key))

    def pause(self) -> None:
        asyncio.run(self._inner.pause())

    def unpause(self) -> None:
        asyncio.run(self._inner.unpause())

    def destroy(self) -> None:
        asyncio.run(self._inner.destroy())

    def copy_to(self, host_path: str, sandbox_path: str) -> None:
        asyncio.run(self._inner.copy_to(host_path, sandbox_path))

    def copy_from(self, sandbox_path: str, host_path: str) -> None:
        asyncio.run(self._inner.copy_from(sandbox_path, host_path))

    def __enter__(self) -> Sandbox:
        return self

    def __exit__(self, *exc: object) -> None:
        self.destroy()
