# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

import unittest
from unittest.mock import AsyncMock, patch

from roche_sandbox.run import (
    Snapshot,
    async_delete_snapshot,
    async_restore,
    async_snapshot,
)


class TestSnapshotDataclass(unittest.TestCase):
    def test_fields(self):
        snap = Snapshot(
            snapshot_id="roche-snap-abc123-1234",
            sandbox_id="abc123def456",
            provider="docker",
            image="roche-snap-abc123-1234",
        )
        assert snap.snapshot_id == "roche-snap-abc123-1234"
        assert snap.sandbox_id == "abc123def456"
        assert snap.provider == "docker"
        assert snap.image == "roche-snap-abc123-1234"


class TestAsyncSnapshot(unittest.IsolatedAsyncioTestCase):
    @patch("roche_sandbox.run.asyncio.create_subprocess_exec")
    async def test_snapshot_calls_docker_commit(self, mock_exec):
        mock_proc = AsyncMock()
        mock_proc.returncode = 0
        mock_proc.communicate.return_value = (b"sha256:abc123\n", b"")
        mock_exec.return_value = mock_proc

        snap = await async_snapshot("my-sandbox-id-full")

        # Verify docker commit was called
        mock_exec.assert_called_once()
        call_args = mock_exec.call_args[0]
        assert call_args[0] == "docker"
        assert call_args[1] == "commit"
        assert call_args[2] == "my-sandbox-id-full"
        # The snapshot image name should be the 4th arg
        assert call_args[3].startswith("roche-snap-my-sandbox-i")

        assert snap.sandbox_id == "my-sandbox-id-full"
        assert snap.provider == "docker"
        assert snap.snapshot_id == snap.image

    @patch("roche_sandbox.run.asyncio.create_subprocess_exec")
    async def test_snapshot_raises_on_failure(self, mock_exec):
        mock_proc = AsyncMock()
        mock_proc.returncode = 1
        mock_proc.communicate.return_value = (b"", b"Error: no such container")
        mock_exec.return_value = mock_proc

        with self.assertRaises(RuntimeError) as ctx:
            await async_snapshot("bad-container")
        assert "Snapshot failed" in str(ctx.exception)


class TestAsyncRestore(unittest.IsolatedAsyncioTestCase):
    async def test_restore_requires_command(self):
        snap = Snapshot(
            snapshot_id="roche-snap-abc-123",
            sandbox_id="abc",
            provider="docker",
            image="roche-snap-abc-123",
        )
        with self.assertRaises(ValueError) as ctx:
            await async_restore(snap, command=None)
        assert "command is required" in str(ctx.exception)

    @patch("roche_sandbox.run.AsyncRoche")
    async def test_restore_creates_sandbox_from_snapshot_image(self, MockClient):
        from roche_sandbox.types import ExecOutput

        mock_sandbox = AsyncMock()
        mock_sandbox.exec.return_value = ExecOutput(exit_code=0, stdout="1.24.0\n", stderr="")
        mock_sandbox.destroy = AsyncMock()

        mock_instance = MockClient.return_value
        mock_instance.create = AsyncMock(return_value=mock_sandbox)

        snap = Snapshot(
            snapshot_id="roche-snap-abc-123",
            sandbox_id="abc",
            provider="docker",
            image="roche-snap-abc-123",
        )

        result = await async_restore(snap, command=["python3", "-c", "import numpy; print(numpy.__version__)"])

        assert result.exit_code == 0
        assert result.stdout == "1.24.0\n"

        # Verify sandbox was created with the snapshot image
        create_kwargs = mock_instance.create.call_args[1]
        assert create_kwargs["image"] == "roche-snap-abc-123"
        assert create_kwargs["writable"] is True

        mock_sandbox.destroy.assert_called_once()


class TestAsyncDeleteSnapshot(unittest.IsolatedAsyncioTestCase):
    @patch("roche_sandbox.run.asyncio.create_subprocess_exec")
    async def test_delete_calls_docker_rmi(self, mock_exec):
        mock_proc = AsyncMock()
        mock_proc.returncode = 0
        mock_proc.communicate.return_value = (b"", b"")
        mock_exec.return_value = mock_proc

        snap = Snapshot(
            snapshot_id="roche-snap-abc-123",
            sandbox_id="abc",
            provider="docker",
            image="roche-snap-abc-123",
        )

        await async_delete_snapshot(snap)

        mock_exec.assert_called_once()
        call_args = mock_exec.call_args[0]
        assert call_args[0] == "docker"
        assert call_args[1] == "rmi"
        assert call_args[2] == "roche-snap-abc-123"


if __name__ == "__main__":
    unittest.main()
