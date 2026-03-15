import asyncio
import json
from unittest.mock import AsyncMock, patch

import pytest

from roche_sandbox.transport.cli import CliTransport
from roche_sandbox.types import SandboxConfig, Mount
from roche_sandbox.errors import ProviderUnavailable, SandboxNotFound


@pytest.fixture
def transport():
    return CliTransport(binary="roche")


def make_process_mock(stdout="", stderr="", returncode=0):
    proc = AsyncMock()
    proc.communicate = AsyncMock(return_value=(stdout.encode(), stderr.encode()))
    proc.returncode = returncode
    return proc


@pytest.mark.asyncio
class TestCliTransportCreate:
    async def test_create_default_config(self, transport):
        proc = make_process_mock(stdout="abc123\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            sandbox_id = await transport.create(SandboxConfig(), "docker")
        assert sandbox_id == "abc123"
        args = mock_exec.call_args[0]
        assert "create" in args
        assert "--provider" in args
        assert "docker" in args
        assert "--image" in args
        assert "python:3.12-slim" in args

    async def test_create_with_network_and_writable(self, transport):
        proc = make_process_mock(stdout="id1\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            config = SandboxConfig(network=True, writable=True, memory="1g", cpus=2.0)
            await transport.create(config, "docker")
        args = mock_exec.call_args[0]
        assert "--network" in args
        assert "--writable" in args
        assert "--memory" in args
        assert "1g" in args
        assert "--cpus" in args
        assert "2.0" in args

    async def test_create_with_mounts(self, transport):
        proc = make_process_mock(stdout="id1\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            config = SandboxConfig(mounts=[
                Mount("/host/a", "/container/a"),
                Mount("/host/b", "/container/b", readonly=False),
            ])
            await transport.create(config, "docker")
        args = mock_exec.call_args[0]
        assert "/host/a:/container/a:ro" in args
        assert "/host/b:/container/b:rw" in args

    async def test_create_with_env(self, transport):
        proc = make_process_mock(stdout="id1\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            config = SandboxConfig(env={"FOO": "bar"})
            await transport.create(config, "docker")
        args = mock_exec.call_args[0]
        assert "--env" in args
        assert "FOO=bar" in args


@pytest.mark.asyncio
class TestCliTransportExec:
    async def test_exec_success(self, transport):
        proc = make_process_mock(stdout="hello\n", returncode=0)
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            output = await transport.exec("abc", ["echo", "hello"], "docker")
        assert output.exit_code == 0
        assert output.stdout == "hello\n"

    async def test_exec_nonzero_exit(self, transport):
        proc = make_process_mock(stderr="command failed", returncode=1)
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            output = await transport.exec("abc", ["false"], "docker")
        assert output.exit_code == 1

    async def test_exec_roche_error_raises(self, transport):
        proc = make_process_mock(stderr="Error: sandbox not found", returncode=1)
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            with pytest.raises(SandboxNotFound):
                await transport.exec("abc", ["echo"], "docker")


@pytest.mark.asyncio
class TestCliTransportOther:
    async def test_list_parses_json(self, transport):
        data = [{"id": "abc", "status": "running", "provider": "docker", "image": "python:3.12-slim"}]
        proc = make_process_mock(stdout=json.dumps(data))
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            sandboxes = await transport.list("docker")
        assert len(sandboxes) == 1
        assert sandboxes[0].id == "abc"

    async def test_pause(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.pause("abc", "docker")
        args = mock_exec.call_args[0]
        assert "pause" in args
        assert "abc" in args

    async def test_copy_to(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.copy_to("abc", "/local/f.py", "/sandbox/f.py", "docker")
        args = mock_exec.call_args[0]
        assert "cp" in args
        assert "/local/f.py" in args
        assert "abc:/sandbox/f.py" in args

    async def test_copy_from(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.copy_from("abc", "/sandbox/out.txt", "/local/out.txt", "docker")
        args = mock_exec.call_args[0]
        assert "cp" in args
        assert "abc:/sandbox/out.txt" in args
        assert "/local/out.txt" in args

    async def test_unpause(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.unpause("abc", "docker")
        args = mock_exec.call_args[0]
        assert "unpause" in args
        assert "abc" in args

    async def test_destroy_with_ids(self, transport):
        proc = make_process_mock(stdout="abc\ndef\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            destroyed = await transport.destroy(["abc", "def"], "docker")
        assert destroyed == ["abc", "def"]
        args = mock_exec.call_args[0]
        assert "destroy" in args
        assert "abc" in args
        assert "def" in args

    async def test_destroy_all(self, transport):
        proc = make_process_mock(stdout="abc\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.destroy([], "docker", all=True)
        args = mock_exec.call_args[0]
        assert "destroy" in args
        assert "--all" in args

    async def test_gc_with_flags(self, transport):
        proc = make_process_mock(stdout="old1\nold2\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            destroyed = await transport.gc("docker", dry_run=True, all=True)
        assert destroyed == ["old1", "old2"]
        args = mock_exec.call_args[0]
        assert "gc" in args
        assert "--dry-run" in args
        assert "--all" in args

    async def test_binary_not_found(self, transport):
        with patch("asyncio.create_subprocess_exec", side_effect=FileNotFoundError):
            with pytest.raises(ProviderUnavailable):
                await transport.create(SandboxConfig(), "docker")
