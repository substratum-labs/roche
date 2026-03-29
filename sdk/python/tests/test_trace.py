from roche_sandbox.trace import ExecutionTrace, ResourceUsage, FileAccess, TraceLevel


def test_trace_summary_basic():
    trace = ExecutionTrace(
        duration_secs=2.3,
        resource_usage=ResourceUsage(peak_memory_bytes=356_000_000, cpu_time_secs=1.2, network_rx_bytes=0, network_tx_bytes=0),
        file_accesses=[
            FileAccess(path="/data/input.csv", op="read", size_bytes=2_300_000),
            FileAccess(path="/workspace/out.json", op="create", size_bytes=4_100),
        ],
    )
    summary = trace.summary()
    assert "2.3s" in summary
    assert "356MB" in summary
    assert "read 1 files" in summary
    assert "wrote 1 files" in summary


def test_trace_summary_empty():
    trace = ExecutionTrace(
        duration_secs=0.01,
        resource_usage=ResourceUsage(peak_memory_bytes=1_000_000, cpu_time_secs=0.0, network_rx_bytes=0, network_tx_bytes=0),
    )
    summary = trace.summary()
    assert "0.0s" in summary
    assert "blocked" not in summary


def test_trace_level_values():
    assert TraceLevel.OFF == "off"
    assert TraceLevel.STANDARD == "standard"
    assert TraceLevel.FULL == "full"
