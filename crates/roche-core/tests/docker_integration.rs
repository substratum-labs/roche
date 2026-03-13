//! Integration tests for DockerProvider.
//! Requires Docker daemon running.
//!
//! Run with: cargo test -p roche-core --test docker_integration -- --ignored --test-threads=1

use roche_core::provider::docker::DockerProvider;
use roche_core::provider::SandboxProvider;
use roche_core::types::{ExecRequest, SandboxConfig, SandboxStatus};

/// Helper: create a sandbox with defaults, return its ID.
/// Caller is responsible for cleanup.
async fn create_default_sandbox(provider: &DockerProvider) -> String {
    let config = SandboxConfig::default();
    provider
        .create(&config)
        .await
        .expect("failed to create sandbox")
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_create_and_destroy() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;
    assert!(!id.is_empty());
    assert!(id.len() == 12, "ID should be 12 hex chars, got: {id}");

    provider.destroy(&id).await.expect("failed to destroy");
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_exec_simple_command() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let request = ExecRequest {
        command: vec!["echo".into(), "hello roche".into()],
        timeout_secs: Some(30),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.trim(), "hello roche");
    assert!(output.stderr.is_empty());

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_exec_python() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let request = ExecRequest {
        command: vec!["python3".into(), "-c".into(), "print(2 + 2)".into()],
        timeout_secs: Some(30),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.trim(), "4");

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_exec_nonzero_exit() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let request = ExecRequest {
        command: vec!["sh".into(), "-c".into(), "exit 42".into()],
        timeout_secs: Some(30),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");

    assert_eq!(output.exit_code, 42);

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_list_includes_created_sandbox() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    let sandboxes = provider.list().await.expect("list failed");
    let found = sandboxes.iter().any(|s| s.id == id);
    assert!(found, "Created sandbox {id} should appear in list");

    let sb = sandboxes.iter().find(|s| s.id == id).unwrap();
    assert_eq!(sb.status, SandboxStatus::Running);
    assert_eq!(sb.provider, "docker");

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_destroy_nonexistent_returns_error() {
    let provider = DockerProvider::new();
    let result = provider.destroy(&"nonexistent12".to_string()).await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_network_disabled_by_default() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    // Attempting to reach the network should fail
    let request = ExecRequest {
        command: vec![
            "python3".into(),
            "-c".into(),
            "import urllib.request; urllib.request.urlopen('http://1.1.1.1', timeout=3)".into(),
        ],
        timeout_secs: Some(10),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");
    assert_ne!(output.exit_code, 0, "Network should be disabled by default");

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires Docker daemon"]
async fn test_readonly_fs_by_default() {
    let provider = DockerProvider::new();
    let id = create_default_sandbox(&provider).await;

    // Test that root filesystem is read-only.
    // Note: /tmp might be tmpfs, so we write to / directly.
    let request = ExecRequest {
        command: vec!["sh".into(), "-c".into(), "touch /test_readonly 2>&1".into()],
        timeout_secs: Some(10),
    };
    let output = provider.exec(&id, &request).await.expect("exec failed");
    assert_ne!(output.exit_code, 0, "Root FS should be read-only");

    provider.destroy(&id).await.unwrap();
}
