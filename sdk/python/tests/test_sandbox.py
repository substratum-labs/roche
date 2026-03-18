from unittest.mock import AsyncMock

import pytest

from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import ExecOutput


def mock_transport():
    t = AsyncMock()
    t.create.return_value = "sb-1"
    t.exec.return_value = ExecOutput(exit_code=0, stdout="ok", stderr="")
    t.destroy.return_value = ["sb-1"]
    return t


@pytest.mark.asyncio
class TestAsyncSandbox:
    async def test_stores_id_and_provider(self):
        sb = AsyncSandbox("abc", "docker", mock_transport())
        assert sb.id == "abc"
        assert sb.provider == "docker"

    async def test_exec_delegates(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        output = await sb.exec(["echo", "hi"])
        t.exec.assert_called_once_with("abc", ["echo", "hi"], "docker", None, trace_level=None)
        assert output.exit_code == 0

    async def test_exec_with_timeout(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.exec(["sleep", "10"], timeout_secs=5)
        t.exec.assert_called_once_with("abc", ["sleep", "10"], "docker", 5, trace_level=None)

    async def test_pause(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.pause()
        t.pause.assert_called_once_with("abc", "docker")

    async def test_unpause(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.unpause()
        t.unpause.assert_called_once_with("abc", "docker")

    async def test_destroy(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.destroy()
        t.destroy.assert_called_once_with(["abc"], "docker")

    async def test_copy_to(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.copy_to("/local/f.py", "/sandbox/f.py")
        t.copy_to.assert_called_once_with("abc", "/local/f.py", "/sandbox/f.py", "docker")

    async def test_copy_from(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.copy_from("/sandbox/out.txt", "/local/out.txt")
        t.copy_from.assert_called_once_with("abc", "/sandbox/out.txt", "/local/out.txt", "docker")

    async def test_async_context_manager(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        async with sb:
            pass
        t.destroy.assert_called_once_with(["abc"], "docker")


class TestSyncSandbox:
    def test_exec(self):
        t = mock_transport()
        sb = Sandbox("abc", "docker", t)
        output = sb.exec(["echo", "hi"])
        assert output.exit_code == 0

    def test_context_manager(self):
        t = mock_transport()
        sb = Sandbox("abc", "docker", t)
        with sb:
            pass
        t.destroy.assert_called_once()
