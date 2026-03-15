import subprocess
import pytest


def docker_available():
    """Check if Docker daemon is running."""
    try:
        result = subprocess.run(
            ["docker", "info"],
            capture_output=True,
            timeout=5,
        )
        return result.returncode == 0
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


def roche_cli_available():
    """Check if the roche CLI binary is on PATH."""
    try:
        result = subprocess.run(
            ["roche", "--help"],
            capture_output=True,
            timeout=5,
        )
        return result.returncode == 0
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


requires_docker = pytest.mark.skipif(
    not docker_available(), reason="Docker daemon not available"
)
requires_roche_cli = pytest.mark.skipif(
    not roche_cli_available(), reason="roche CLI not on PATH"
)
