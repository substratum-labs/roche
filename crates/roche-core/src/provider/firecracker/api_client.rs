// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::provider::ProviderError;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Request;
use std::path::PathBuf;

/// HTTP client for Firecracker's REST API over a Unix socket.
pub struct FirecrackerApiClient {
    socket_path: PathBuf,
}

impl FirecrackerApiClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Send an HTTP request to the Firecracker API.
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> Result<(), ProviderError> {
        let body_str = serde_json::to_string(&body)
            .map_err(|e| ProviderError::ExecFailed(format!("json serialize: {e}")))?;

        let stream = tokio::net::UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| {
                ProviderError::Unavailable(format!(
                    "cannot connect to Firecracker socket {}: {e}",
                    self.socket_path.display()
                ))
            })?;

        let io = hyper_util::rt::TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("HTTP handshake failed: {e}")))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Firecracker API connection error: {e}");
            }
        });

        let req = Request::builder()
            .method(method)
            .uri(format!("http://localhost{path}"))
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from(body_str)))
            .map_err(|e| ProviderError::ExecFailed(format!("request build: {e}")))?;

        let response = sender
            .send_request(req)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("API request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_bytes = http_body_util::BodyExt::collect(response.into_body())
                .await
                .map_err(|e| ProviderError::ExecFailed(format!("read response: {e}")))?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body_bytes);
            return Err(ProviderError::ExecFailed(format!(
                "Firecracker API error ({status}): {body_str}"
            )));
        }

        Ok(())
    }

    /// Configure the boot source (kernel + boot args).
    pub async fn put_boot_source(
        &self,
        kernel_image_path: &str,
        boot_args: &str,
    ) -> Result<(), ProviderError> {
        self.send(
            "PUT",
            "/boot-source",
            serde_json::json!({
                "kernel_image_path": kernel_image_path,
                "boot_args": boot_args
            }),
        )
        .await
    }

    /// Configure a drive.
    pub async fn put_drive(
        &self,
        drive_id: &str,
        path_on_host: &str,
        is_root_device: bool,
        is_read_only: bool,
    ) -> Result<(), ProviderError> {
        self.send(
            "PUT",
            &format!("/drives/{drive_id}"),
            serde_json::json!({
                "drive_id": drive_id,
                "path_on_host": path_on_host,
                "is_root_device": is_root_device,
                "is_read_only": is_read_only
            }),
        )
        .await
    }

    /// Configure machine resources (vCPUs, memory).
    pub async fn put_machine_config(
        &self,
        vcpu_count: u8,
        mem_size_mib: u64,
    ) -> Result<(), ProviderError> {
        self.send(
            "PUT",
            "/machine-config",
            serde_json::json!({
                "vcpu_count": vcpu_count,
                "mem_size_mib": mem_size_mib
            }),
        )
        .await
    }

    /// Configure the vsock device.
    pub async fn put_vsock(&self, guest_cid: u32) -> Result<(), ProviderError> {
        self.send(
            "PUT",
            "/vsock",
            serde_json::json!({
                "guest_cid": guest_cid,
                "uds_path": "vsock.sock"
            }),
        )
        .await
    }

    /// Start the microVM.
    pub async fn start(&self) -> Result<(), ProviderError> {
        self.send(
            "PUT",
            "/actions",
            serde_json::json!({
                "action_type": "InstanceStart"
            }),
        )
        .await
    }

    /// Pause the microVM.
    pub async fn pause(&self) -> Result<(), ProviderError> {
        self.send(
            "PATCH",
            "/vm",
            serde_json::json!({
                "state": "Paused"
            }),
        )
        .await
    }

    /// Resume the microVM.
    pub async fn resume(&self) -> Result<(), ProviderError> {
        self.send(
            "PATCH",
            "/vm",
            serde_json::json!({
                "state": "Resumed"
            }),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_new() {
        let client = FirecrackerApiClient::new(PathBuf::from("/tmp/test.sock"));
        assert_eq!(client.socket_path, PathBuf::from("/tmp/test.sock"));
    }
}
