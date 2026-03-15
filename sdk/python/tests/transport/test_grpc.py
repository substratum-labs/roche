from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from roche_sandbox.transport.grpc import GrpcTransport
from roche_sandbox.errors import (
    SandboxNotFound, SandboxPaused, ProviderUnavailable,
    TimeoutError, UnsupportedOperation, RocheError,
)


class FakeRpcError(Exception):
    def __init__(self, code, details="error"):
        self._code = code
        self._details = details
    def code(self):
        return self._code
    def details(self):
        return self._details


@pytest.mark.asyncio
class TestGrpcTransportErrorMapping:
    async def test_not_found_maps_to_sandbox_not_found(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("NOT_FOUND", "sandbox not found")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, SandboxNotFound)

    async def test_failed_precondition_maps_to_sandbox_paused(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("FAILED_PRECONDITION", "paused")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, SandboxPaused)

    async def test_unavailable_maps_to_provider_unavailable(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("UNAVAILABLE", "conn refused")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, ProviderUnavailable)

    async def test_deadline_exceeded_maps_to_timeout(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("DEADLINE_EXCEEDED", "timeout")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, TimeoutError)

    async def test_unimplemented_maps_to_unsupported(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("UNIMPLEMENTED", "not impl")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, UnsupportedOperation)

    async def test_other_maps_to_roche_error(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("INTERNAL", "boom")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, RocheError)
        assert not isinstance(mapped, SandboxNotFound)
