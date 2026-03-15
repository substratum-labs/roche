from roche_sandbox.types import SandboxConfig, ExecOutput, Mount, SandboxInfo

def test_sandbox_config_defaults():
    config = SandboxConfig()
    assert config.provider == "docker"
    assert config.image == "python:3.12-slim"
    assert config.timeout_secs == 300
    assert config.network is False
    assert config.writable is False
    assert config.env == {}
    assert config.mounts == []

def test_exec_output():
    output = ExecOutput(exit_code=0, stdout="hi", stderr="")
    assert output.exit_code == 0

def test_mount_defaults():
    mount = Mount(host_path="/a", container_path="/b")
    assert mount.readonly is True

def test_sandbox_info():
    info = SandboxInfo(id="abc", status="running", provider="docker", image="python:3.12-slim")
    assert info.status == "running"
    assert info.expires_at is None
