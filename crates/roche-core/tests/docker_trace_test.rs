use roche_core::sensor::docker::{parse_docker_diff, parse_memory_bytes, parse_net_rx, parse_net_tx};
use roche_core::sensor::types::FileOp;

#[test]
fn test_parse_docker_diff_creates() {
    let output = "A /workspace/output.txt\nA /tmp/cache\n";
    let accesses = parse_docker_diff(output);
    assert_eq!(accesses.len(), 2);
    assert_eq!(accesses[0].op, FileOp::Create);
    assert_eq!(accesses[0].path, "/workspace/output.txt");
}

#[test]
fn test_parse_docker_diff_changes() {
    let output = "C /var/log\nC /var/log/syslog\n";
    let accesses = parse_docker_diff(output);
    assert_eq!(accesses.len(), 2);
    assert_eq!(accesses[0].op, FileOp::Write);
}

#[test]
fn test_parse_docker_diff_deletes() {
    let output = "D /tmp/old_file\n";
    let accesses = parse_docker_diff(output);
    assert_eq!(accesses.len(), 1);
    assert_eq!(accesses[0].op, FileOp::Delete);
}

#[test]
fn test_parse_docker_diff_empty() {
    let accesses = parse_docker_diff("");
    assert!(accesses.is_empty());
}

#[test]
fn test_parse_memory_bytes_mib() {
    assert_eq!(parse_memory_bytes("340MiB"), 340 * 1024 * 1024);
}

#[test]
fn test_parse_memory_bytes_gib() {
    assert_eq!(
        parse_memory_bytes("1.5GiB"),
        (1.5 * 1024.0 * 1024.0 * 1024.0) as u64
    );
}

#[test]
fn test_parse_memory_bytes_empty() {
    assert_eq!(parse_memory_bytes(""), 0);
}

#[test]
fn test_parse_net_io() {
    assert_eq!(parse_net_rx("1.2kB"), 1200);
    assert_eq!(parse_net_tx("0B"), 0);
}
