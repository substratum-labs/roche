// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use roche_core::sensor::{
    ExecutionTrace, ResourceUsage, TraceLevel,
};

#[test]
fn test_trace_level_default() {
    let level: TraceLevel = Default::default();
    assert_eq!(level, TraceLevel::Standard);
}

#[test]
fn test_execution_trace_serialization_roundtrip() {
    let trace = ExecutionTrace {
        duration_secs: 1.234,
        resource_usage: ResourceUsage {
            peak_memory_bytes: 1024,
            cpu_time_secs: 0.5,
            network_rx_bytes: 100,
            network_tx_bytes: 200,
        },
        file_accesses: vec![],
        network_attempts: vec![],
        blocked_ops: vec![],
        syscalls: vec![],
        resource_timeline: vec![],
    };

    let json = serde_json::to_string(&trace).unwrap();
    let deserialized: ExecutionTrace = serde_json::from_str(&json).unwrap();

    assert!((deserialized.duration_secs - 1.234).abs() < f64::EPSILON);
    assert_eq!(deserialized.resource_usage.peak_memory_bytes, 1024);
    assert!((deserialized.resource_usage.cpu_time_secs - 0.5).abs() < f64::EPSILON);
    assert_eq!(deserialized.resource_usage.network_rx_bytes, 100);
    assert_eq!(deserialized.resource_usage.network_tx_bytes, 200);
}

#[test]
fn test_execution_trace_empty_fields_deserialize() {
    // JSON with only required fields — Vec fields should default to empty
    let json = r#"{
        "duration_secs": 0.5,
        "resource_usage": {
            "peak_memory_bytes": 0,
            "cpu_time_secs": 0.0,
            "network_rx_bytes": 0,
            "network_tx_bytes": 0
        }
    }"#;

    let trace: ExecutionTrace = serde_json::from_str(json).unwrap();
    assert!((trace.duration_secs - 0.5).abs() < f64::EPSILON);
    assert!(trace.file_accesses.is_empty());
    assert!(trace.network_attempts.is_empty());
    assert!(trace.blocked_ops.is_empty());
    assert!(trace.syscalls.is_empty());
    assert!(trace.resource_timeline.is_empty());
}

#[test]
fn test_trace_level_serialization() {
    let level = TraceLevel::Full;
    let json = serde_json::to_string(&level).unwrap();
    assert_eq!(json, "\"full\"");

    let deserialized: TraceLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, TraceLevel::Full);

    // Test ordering
    assert!(TraceLevel::Off < TraceLevel::Summary);
    assert!(TraceLevel::Summary < TraceLevel::Standard);
    assert!(TraceLevel::Standard < TraceLevel::Full);
}
