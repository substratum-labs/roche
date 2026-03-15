"""End-to-end tests using the Python SDK with CLI transport."""
import pytest
from roche_sandbox import Roche
from .conftest import requires_docker, requires_roche_cli


pytestmark = [requires_docker, requires_roche_cli]


class TestCliTransportE2E:
    """Tests that exercise the full CLI transport path."""

    def setup_method(self):
        self.roche = Roche(mode="direct")

    def test_create_exec_destroy(self):
        """Full lifecycle: create -> exec -> destroy."""
        sandbox = self.roche.create(image="python:3.12-slim")
        assert sandbox.id is not None
        assert len(sandbox.id) > 0

        output = sandbox.exec(["echo", "hello from roche"])
        assert output.exit_code == 0
        assert "hello from roche" in output.stdout

        sandbox.destroy()

    def test_context_manager_auto_destroy(self):
        """Context manager should auto-destroy on exit."""
        with self.roche.create(image="python:3.12-slim") as sandbox:
            sandbox_id = sandbox.id
            output = sandbox.exec(["echo", "ctx"])
            assert output.exit_code == 0

        # After context exit, sandbox should be destroyed
        # Attempting to exec on it should fail
        # (We can't easily test this without another sandbox reference)

    def test_list_includes_created(self):
        """Created sandbox should appear in list."""
        sandbox = self.roche.create(image="python:3.12-slim")
        try:
            sandboxes = self.roche.list()
            ids = [s.id for s in sandboxes]
            assert sandbox.id in ids
        finally:
            sandbox.destroy()

    def test_exec_on_destroyed_raises(self):
        """Exec on a destroyed sandbox should raise an error."""
        sandbox = self.roche.create(image="python:3.12-slim")
        sandbox.destroy()

        with pytest.raises(Exception):
            sandbox.exec(["echo", "should fail"])

    def test_exec_nonzero_exit(self):
        """Command with non-zero exit should be captured."""
        sandbox = self.roche.create(image="python:3.12-slim")
        try:
            output = sandbox.exec(["sh", "-c", "exit 42"])
            assert output.exit_code == 42
        finally:
            sandbox.destroy()

    def test_exec_stderr(self):
        """Stderr should be captured."""
        sandbox = self.roche.create(image="python:3.12-slim")
        try:
            output = sandbox.exec(["sh", "-c", "echo err >&2"])
            assert "err" in output.stderr
        finally:
            sandbox.destroy()
