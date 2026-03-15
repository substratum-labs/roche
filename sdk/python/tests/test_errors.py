from roche_sandbox.errors import (
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    ProviderUnavailable,
    TimeoutError,
    UnsupportedOperation,
)

def test_sandbox_not_found_is_roche_error():
    err = SandboxNotFound("abc123")
    assert isinstance(err, RocheError)
    assert "abc123" in str(err)

def test_sandbox_paused_is_roche_error():
    assert isinstance(SandboxPaused("abc"), RocheError)

def test_provider_unavailable_is_roche_error():
    assert isinstance(ProviderUnavailable("down"), RocheError)

def test_timeout_error_is_roche_error():
    assert isinstance(TimeoutError("30s"), RocheError)

def test_unsupported_operation_is_roche_error():
    assert isinstance(UnsupportedOperation("pause"), RocheError)
