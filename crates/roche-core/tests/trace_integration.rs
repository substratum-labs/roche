use roche_core::provider::docker::DockerProvider;
use roche_core::provider::SandboxProvider;
use roche_core::sensor::{DockerSensor, SandboxSensor, TraceLevel};
use roche_core::types::{ExecRequest, SandboxConfig};

#[tokio::test]
#[ignore]
async fn test_trace_summary_returns_duration() {
    let provider = DockerProvider::new();
    let sensor = DockerSensor;
    let config = SandboxConfig::default();
    let id = provider.create(&config).await.unwrap();

    let collector = sensor.start_trace(&id, TraceLevel::Summary).await.unwrap();
    let output = provider
        .exec(
            &id,
            &ExecRequest {
                command: vec!["echo".into(), "hello".into()],
                timeout_secs: Some(10),
            },
        )
        .await
        .unwrap();
    let trace = collector.finish().await.unwrap();

    assert!(trace.duration_secs > 0.0);
    assert_eq!(output.stdout.trim(), "hello");
    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_trace_standard_detects_file_writes() {
    let provider = DockerProvider::new();
    let sensor = DockerSensor;
    let mut config = SandboxConfig::default();
    config.writable = true;
    let id = provider.create(&config).await.unwrap();

    let collector = sensor.start_trace(&id, TraceLevel::Standard).await.unwrap();
    let _output = provider
        .exec(
            &id,
            &ExecRequest {
                command: vec![
                    "sh".into(),
                    "-c".into(),
                    "echo test > /tmp/output.txt".into(),
                ],
                timeout_secs: Some(10),
            },
        )
        .await
        .unwrap();
    let trace = collector.finish().await.unwrap();

    assert!(trace.duration_secs > 0.0);
    assert!(
        !trace.file_accesses.is_empty(),
        "should detect file creation"
    );
    let created_files: Vec<_> = trace
        .file_accesses
        .iter()
        .filter(|f| f.path.contains("output.txt"))
        .collect();
    assert!(
        !created_files.is_empty(),
        "should detect output.txt creation"
    );
    provider.destroy(&id).await.unwrap();
}
