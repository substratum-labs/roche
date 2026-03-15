from unittest.mock import AsyncMock

import pytest

from roche_sandbox.client import AsyncRoche, Roche
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo


def mock_transport():
    t = AsyncMock()
    t.create.return_value = "sb-1"
    t.exec.return_value = ExecOutput(exit_code=0, stdout="ok", stderr="")
    t.destroy.return_value = ["sb-1"]
    t.list.return_value = [
        SandboxInfo(id="sb-1", status="running", provider="docker", image="python:3.12-slim")
    ]
    t.gc.return_value = ["sb-old"]
    return t


@pytest.mark.asyncio
class TestAsyncRoche:
    async def test_create_returns_async_sandbox(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sb = await roche.create(image="node:20")
        assert isinstance(sb, AsyncSandbox)
        assert sb.id == "sb-1"
        assert sb.provider == "docker"

    async def test_create_captures_provider(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sb = await roche.create(provider="firecracker")
        assert sb.provider == "firecracker"
        config_arg, provider_arg = t.create.call_args[0]
        assert provider_arg == "firecracker"

    async def test_create_id_returns_string(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sandbox_id = await roche.create_id(image="python:3.12-slim")
        assert sandbox_id == "sb-1"

    async def test_exec(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        output = await roche.exec("sb-1", ["echo", "hi"])
        assert output.exit_code == 0
        t.exec.assert_called_once_with("sb-1", ["echo", "hi"], "docker", None)

    async def test_destroy(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        await roche.destroy("sb-1")
        t.destroy.assert_called_once_with(["sb-1"], "docker")

    async def test_list(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sandboxes = await roche.list()
        assert len(sandboxes) == 1

    async def test_gc(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        destroyed = await roche.gc()
        assert destroyed == ["sb-old"]

    async def test_custom_provider(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t, provider="firecracker")
        await roche.list()
        t.list.assert_called_once_with("firecracker")


class TestSyncRoche:
    def test_create_returns_sync_sandbox(self):
        t = mock_transport()
        roche = Roche(transport=t)
        sb = roche.create(image="node:20")
        assert isinstance(sb, Sandbox)
        assert sb.id == "sb-1"

    def test_exec(self):
        t = mock_transport()
        roche = Roche(transport=t)
        output = roche.exec("sb-1", ["echo", "hi"])
        assert output.exit_code == 0
