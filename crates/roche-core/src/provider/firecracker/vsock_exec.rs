// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::provider::ProviderError;
use crate::types::{ExecOutput, ExecRequest};

/// Execute a command in the guest via vsock.
///
/// Connects to the roche-agent running inside the guest VM on vsock port 52.
/// Sends a length-prefixed JSON request and reads a length-prefixed JSON response.
#[cfg(target_os = "linux")]
pub async fn exec_via_vsock(
    cid: u32,
    request: &ExecRequest,
    timeout_secs: u64,
) -> Result<ExecOutput, ProviderError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_vsock::VsockStream;

    const AGENT_PORT: u32 = 52;

    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        VsockStream::connect(cid, AGENT_PORT),
    )
    .await
    .map_err(|_| ProviderError::Timeout(5))?
    .map_err(|e| ProviderError::ExecFailed(format!("vsock connect failed: {e}")))?;

    let (mut reader, mut writer) = tokio::io::split(stream);

    // Send the exec request as length-prefixed JSON
    let req_json = serde_json::json!({
        "command": request.command,
        "timeout_secs": request.timeout_secs.unwrap_or(timeout_secs),
    });
    let req_bytes = serde_json::to_vec(&req_json)
        .map_err(|e| ProviderError::ExecFailed(format!("serialize request: {e}")))?;

    let len = req_bytes.len() as u32;
    writer
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| ProviderError::ExecFailed(format!("vsock write len: {e}")))?;
    writer
        .write_all(&req_bytes)
        .await
        .map_err(|e| ProviderError::ExecFailed(format!("vsock write: {e}")))?;
    writer
        .flush()
        .await
        .map_err(|e| ProviderError::ExecFailed(format!("vsock flush: {e}")))?;

    // Read response: 4-byte big-endian length + JSON
    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("vsock read response len: {e}")))?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        if resp_len > 64 * 1024 * 1024 {
            return Err(ProviderError::ExecFailed(
                "response too large (>64MB)".into(),
            ));
        }
        let mut resp_buf = vec![0u8; resp_len];
        reader
            .read_exact(&mut resp_buf)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("vsock read response: {e}")))?;
        let output: ExecOutput = serde_json::from_slice(&resp_buf)
            .map_err(|e| ProviderError::ExecFailed(format!("parse response: {e}")))?;
        Ok(output)
    })
    .await
    .map_err(|_| ProviderError::Timeout(timeout_secs))?;

    result
}

/// Stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub async fn exec_via_vsock(
    _cid: u32,
    _request: &ExecRequest,
    _timeout_secs: u64,
) -> Result<ExecOutput, ProviderError> {
    Err(ProviderError::Unsupported(
        "vsock exec requires Linux".into(),
    ))
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_exec_via_vsock_non_linux_returns_unsupported() {
        use super::*;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let request = ExecRequest {
            command: vec!["echo".into(), "hello".into()],
            timeout_secs: None,
        };
        let result = rt.block_on(exec_via_vsock(3, &request, 30));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Linux"),
            "expected Linux mention, got: {err}"
        );
    }
}
