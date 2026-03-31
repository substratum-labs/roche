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
from roche_sandbox.intent import CodeIntent
from roche_sandbox.types import (
    Budget, BudgetUsage, DynamicPermissions, ExecEvent, ExecOutput,
    SandboxConfig, SandboxInfo, SandboxStatus, SessionInfo,
)

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
        if config.network_allowlist:
            request.network_allowlist.extend(config.network_allowlist)
        if config.fs_paths:
            request.fs_paths.extend(config.fs_paths)
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

    async def exec_stream(self, sandbox_id: str, command: list[str], provider: str, timeout_secs: int | None = None, trace_level: str | None = None):
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        request = sandbox_pb2.ExecStreamRequest(sandbox_id=sandbox_id, command=command, provider=provider)
        if timeout_secs is not None:
            request.timeout_secs = timeout_secs
        if trace_level is not None:
            request.trace_level = _TRACE_LEVEL_MAP.get(trace_level, 0)
        try:
            stream = self._get_stub().ExecStream(request)
            async for event in stream:
                which = event.WhichOneof("event")
                if which == "output":
                    yield ExecEvent(type="output", stream=event.output.stream, data=event.output.data)
                elif which == "heartbeat":
                    hb = event.heartbeat
                    res = hb.resources if hb.HasField("resources") else None
                    yield ExecEvent(
                        type="heartbeat",
                        elapsed_ms=hb.elapsed_ms,
                        memory_bytes=res.memory_bytes if res else None,
                        cpu_percent=res.cpu_percent if res else None,
                    )
                elif which == "result":
                    trace = None  # TODO: convert proto trace to ExecutionTrace
                    yield ExecEvent(type="result", exit_code=event.result.exit_code, trace=trace)
        except Exception as e:
            raise self._map_grpc_error(e)

    async def create_session(self, sandbox_id: str, provider: str, permissions: DynamicPermissions | None = None, budget: Budget | None = None) -> str:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        req_kwargs: dict = {"sandbox_id": sandbox_id, "provider": provider}
        if permissions is not None:
            req_kwargs["permissions"] = sandbox_pb2.DynamicPermissions(
                network=permissions.network, network_allowlist=permissions.network_allowlist,
                writable=permissions.writable, fs_paths=permissions.fs_paths,
            )
        if budget is not None:
            req_kwargs["budget"] = sandbox_pb2.Budget(
                max_execs=budget.max_execs, max_total_secs=budget.max_total_secs,
                max_output_bytes=budget.max_output_bytes,
            )
        try:
            response = await self._get_stub().CreateSession(sandbox_pb2.CreateSessionRequest(**req_kwargs))
        except Exception as e:
            raise self._map_grpc_error(e)
        return response.session_id

    async def destroy_session(self, session_id: str) -> SessionInfo:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            response = await self._get_stub().DestroySession(sandbox_pb2.DestroySessionRequest(session_id=session_id))
        except Exception as e:
            raise self._map_grpc_error(e)
        return self._proto_to_session_info(response.session)

    async def list_sessions(self) -> list[SessionInfo]:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            response = await self._get_stub().ListSessions(sandbox_pb2.ListSessionsRequest())
        except Exception as e:
            raise self._map_grpc_error(e)
        return [self._proto_to_session_info(s) for s in response.sessions]

    async def update_permissions(self, session_id: str, change: dict) -> DynamicPermissions:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        pc = sandbox_pb2.PermissionChange(**change)
        try:
            response = await self._get_stub().UpdatePermissions(
                sandbox_pb2.UpdatePermissionsRequest(session_id=session_id, change=pc)
            )
        except Exception as e:
            raise self._map_grpc_error(e)
        p = response.permissions
        return DynamicPermissions(
            network=p.network, network_allowlist=list(p.network_allowlist),
            writable=p.writable, fs_paths=list(p.fs_paths),
        )

    async def analyze_intent(self, code: str, language: str) -> CodeIntent:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2
        try:
            r = await self._get_stub().AnalyzeIntent(sandbox_pb2.AnalyzeIntentRequest(code=code, language=language))
        except Exception as e:
            raise self._map_grpc_error(e)
        return CodeIntent(
            provider=r.provider, needs_network=r.needs_network, network_hosts=list(r.network_hosts),
            needs_writable=r.needs_writable, writable_paths=list(r.writable_paths),
            needs_packages=r.needs_packages, package_manager=r.package_manager or None,
            memory_hint=r.memory_hint or None, language=r.language, confidence=r.confidence,
            reasoning=list(r.reasoning),
        )

    def _proto_to_session_info(self, s) -> SessionInfo:
        p = s.permissions
        b = s.budget
        u = s.usage
        return SessionInfo(
            session_id=s.session_id, sandbox_id=s.sandbox_id, provider=s.provider,
            permissions=DynamicPermissions(
                network=p.network, network_allowlist=list(p.network_allowlist),
                writable=p.writable, fs_paths=list(p.fs_paths),
            ),
            budget=Budget(max_execs=b.max_execs, max_total_secs=b.max_total_secs, max_output_bytes=b.max_output_bytes),
            usage=BudgetUsage(exec_count=u.exec_count, total_secs=u.total_secs, output_bytes=u.output_bytes),
            created_at_ms=s.created_at_ms,
        )

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
