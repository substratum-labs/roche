# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

from roche_sandbox.errors import (
    ProviderUnavailable, RocheError, SandboxNotFound, SandboxPaused,
    TimeoutError, UnsupportedOperation,
)
from roche_sandbox.trace import (
    BlockedOperation, ExecutionTrace, FileAccess, NetworkAttempt,
    ResourceSnapshot, ResourceUsage, SyscallEvent,
)
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo, SandboxStatus

_PROTO_STATUS_MAP: dict[int, SandboxStatus] = {
    1: "running", 2: "paused", 3: "stopped", 4: "failed",
}

_TRACE_LEVEL_MAP: dict[str, int] = {
    "off": 0,       # TRACE_LEVEL_OFF
    "summary": 1,   # TRACE_LEVEL_SUMMARY
    "standard": 2,  # TRACE_LEVEL_STANDARD
    "full": 3,      # TRACE_LEVEL_FULL
}

_FILE_OP_MAP: dict[int, str] = {
    0: "read",    # FILE_OP_READ
    1: "write",   # FILE_OP_WRITE
    2: "create",  # FILE_OP_CREATE
    3: "delete",  # FILE_OP_DELETE
}

_GRPC_CODE_MAP = {
    "NOT_FOUND": SandboxNotFound,
    "FAILED_PRECONDITION": SandboxPaused,
    "UNAVAILABLE": ProviderUnavailable,
    "DEADLINE_EXCEEDED": TimeoutError,
    "UNIMPLEMENTED": UnsupportedOperation,
}


class GrpcTransport:
    def __init__(self, port: int):
        self._port = port
        self._channel = None
        self._stub = None

    def _get_stub(self):
        if self._stub is None:
            import grpc.aio
            from roche_sandbox.generated.roche.v1 import sandbox_pb2_grpc
            self._channel = grpc.aio.insecure_channel(f"127.0.0.1:{self._port}")
            self._stub = sandbox_pb2_grpc.SandboxServiceStub(self._channel)
        return self._stub

    async def create(self, config: SandboxConfig, provider: str) -> str:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        request = sandbox_pb2.CreateRequest(
            provider=provider, image=config.image, timeout_secs=config.timeout_secs,
            network=config.network, writable=config.writable, env=config.env,
            mounts=[sandbox_pb2.MountConfig(host_path=m.host_path, container_path=m.container_path, readonly=m.readonly) for m in config.mounts],
        )
        if config.memory:
            request.memory = config.memory
        if config.cpus is not None:
            request.cpus = config.cpus
        if config.kernel:
            request.kernel = config.kernel
        if config.rootfs:
            request.rootfs = config.rootfs
        try:
            response = await self._get_stub().Create(request)
        except Exception as e:
            raise self._map_grpc_error(e)
        return response.sandbox_id

    async def exec(self, sandbox_id: str, command: list[str], provider: str, timeout_secs: int | None = None, trace_level: str | None = None, idempotency_key: str | None = None) -> ExecOutput:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        request = sandbox_pb2.ExecRequest(sandbox_id=sandbox_id, command=command, provider=provider)
        if timeout_secs is not None:
            request.timeout_secs = timeout_secs
        if trace_level is not None:
            request.trace_level = _TRACE_LEVEL_MAP.get(trace_level, 0)
        if idempotency_key is not None:
            request.idempotency_key = idempotency_key
        try:
            response = await self._get_stub().Exec(request)
        except Exception as e:
            raise self._map_grpc_error(e)
        trace = None
        if response.HasField("trace"):
            rt = response.trace
            ru = rt.resource_usage
            trace = ExecutionTrace(
                duration_secs=rt.duration_secs,
                resource_usage=ResourceUsage(
                    peak_memory_bytes=ru.peak_memory_bytes,
                    cpu_time_secs=ru.cpu_time_secs,
                    network_rx_bytes=ru.network_rx_bytes,
                    network_tx_bytes=ru.network_tx_bytes,
                ),
                file_accesses=[
                    FileAccess(path=f.path, op=_FILE_OP_MAP.get(f.op, "read"), size_bytes=f.size_bytes or None)
                    for f in rt.file_accesses
                ],
                network_attempts=[
                    NetworkAttempt(address=n.address, protocol=n.protocol, allowed=n.allowed)
                    for n in rt.network_attempts
                ],
                blocked_ops=[
                    BlockedOperation(op_type=b.op_type, detail=b.detail)
                    for b in rt.blocked_ops
                ],
                syscalls=[
                    SyscallEvent(name=s.name, args=list(s.args), result=s.result, timestamp_ms=s.timestamp_ms)
                    for s in rt.syscalls
                ],
                resource_timeline=[
                    ResourceSnapshot(timestamp_ms=r.timestamp_ms, memory_bytes=r.memory_bytes, cpu_percent=r.cpu_percent)
                    for r in rt.resource_timeline
                ],
            )
        return ExecOutput(exit_code=response.exit_code, stdout=response.stdout, stderr=response.stderr, trace=trace)

    async def destroy(self, sandbox_ids: list[str], provider: str, all: bool = False) -> list[str]:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            response = await self._get_stub().Destroy(sandbox_pb2.DestroyRequest(sandbox_ids=sandbox_ids, all=all, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)
        return list(response.destroyed_ids)

    async def list(self, provider: str) -> list[SandboxInfo]:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            response = await self._get_stub().List(sandbox_pb2.ListRequest(provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)
        return [SandboxInfo(id=s.id, status=_PROTO_STATUS_MAP.get(s.status, "failed"), provider=s.provider, image=s.image, expires_at=s.expires_at if s.HasField("expires_at") else None) for s in response.sandboxes]

    async def pause(self, sandbox_id: str, provider: str) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            await self._get_stub().Pause(sandbox_pb2.PauseRequest(sandbox_id=sandbox_id, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)

    async def unpause(self, sandbox_id: str, provider: str) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            await self._get_stub().Unpause(sandbox_pb2.UnpauseRequest(sandbox_id=sandbox_id, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)

    async def gc(self, provider: str, dry_run: bool = False, all: bool = False) -> list[str]:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            response = await self._get_stub().Gc(sandbox_pb2.GcRequest(dry_run=dry_run, all=all, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)
        return list(response.destroyed_ids)

    async def copy_to(self, sandbox_id: str, host_path: str, sandbox_path: str, provider: str) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            await self._get_stub().CopyTo(sandbox_pb2.CopyToRequest(sandbox_id=sandbox_id, host_path=host_path, sandbox_path=sandbox_path, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)

    async def copy_from(self, sandbox_id: str, sandbox_path: str, host_path: str, provider: str) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            await self._get_stub().CopyFrom(sandbox_pb2.CopyFromRequest(sandbox_id=sandbox_id, sandbox_path=sandbox_path, host_path=host_path, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)

    def _map_grpc_error(self, err: Exception) -> RocheError:
        code_str = ""
        details = str(err)
        if hasattr(err, "code") and callable(err.code):
            code_val = err.code()
            code_str = code_val if isinstance(code_val, str) else code_val.name if hasattr(code_val, "name") else str(code_val)
        if hasattr(err, "details") and callable(err.details):
            details = err.details()
        cls = _GRPC_CODE_MAP.get(code_str, RocheError)
        return cls(details)
