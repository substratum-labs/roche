// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::provider::{ProviderError, SandboxFileOps, SandboxLifecycle, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo, SandboxStatus};
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;

const DEFAULT_API_BASE: &str = "https://api.e2b.app";
const DEFAULT_DOMAIN: &str = "e2b.app";
const ENVD_PORT: u16 = 49983;

/// Metadata stored after sandbox creation, needed for exec/file operations.
struct E2bSandboxMeta {
    envd_access_token: String,
    domain: String,
    #[allow(dead_code)]
    template_id: String,
}

/// E2B cloud sandbox provider.
///
/// Uses the E2B REST API for sandbox lifecycle and the envd Connect RPC
/// protocol for command execution inside sandboxes.
///
/// API key resolution: `E2B_API_KEY` env var → `~/.roche/e2b.toml` config file.
pub struct E2bProvider {
    api_key: String,
    api_base: String,
    domain: String,
    client: Client,
    /// Maps sandboxID → metadata needed for envd communication.
    sandboxes: RwLock<HashMap<String, E2bSandboxMeta>>,
}

// --- Control Plane API types (api.e2b.app) ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NewSandbox {
    template_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_internet_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env_vars: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SandboxResponse {
    sandbox_id: String,
    template_id: String,
    #[serde(default)]
    envd_access_token: String,
    #[serde(default)]
    domain: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedSandbox {
    sandbox_id: String,
    template_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    started_at: Option<String>,
}

// --- envd Connect RPC types (process execution) ---

#[derive(Serialize)]
struct StartRequest {
    process: ProcessConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdin: Option<bool>,
}

#[derive(Serialize)]
struct ProcessConfig {
    cmd: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    envs: HashMap<String, String>,
}

/// A single event from the process stream.
/// Connect RPC server-streaming returns these as binary envelopes.
#[derive(Deserialize, Debug)]
struct ProcessEvent {
    #[serde(default)]
    #[allow(dead_code)]
    start: Option<StartEvent>,
    #[serde(default)]
    data: Option<DataEvent>,
    #[serde(default)]
    end: Option<EndEvent>,
}

#[derive(Deserialize, Debug)]
struct StartEvent {
    #[allow(dead_code)]
    pid: u32,
}

#[derive(Deserialize, Debug)]
struct DataEvent {
    #[serde(default)]
    stdout: Option<String>,
    #[serde(default)]
    stderr: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct EndEvent {
    exit_code: i32,
    #[allow(dead_code)]
    exited: bool,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
}

impl E2bProvider {
    /// Create a new E2B provider.
    ///
    /// API key resolution order:
    /// 1. `E2B_API_KEY` environment variable
    /// 2. `~/.roche/e2b.toml` config file (`api_key` field)
    pub fn new() -> Result<Self, ProviderError> {
        let api_key = Self::resolve_api_key()?;
        let api_base =
            std::env::var("E2B_API_URL").unwrap_or_else(|_| DEFAULT_API_BASE.to_string());
        let domain = std::env::var("E2B_DOMAIN").unwrap_or_else(|_| DEFAULT_DOMAIN.to_string());

        Ok(Self {
            api_key,
            api_base,
            domain,
            client: Client::new(),
            sandboxes: RwLock::new(HashMap::new()),
        })
    }

    fn resolve_api_key() -> Result<String, ProviderError> {
        // 1. Environment variable
        if let Ok(key) = std::env::var("E2B_API_KEY") {
            if !key.is_empty() {
                return Ok(key);
            }
        }

        // 2. Config file fallback
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".roche").join("e2b.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path).map_err(|e| {
                    ProviderError::Unavailable(format!("failed to read e2b.toml: {e}"))
                })?;
                // Simple TOML parsing for api_key = "..."
                for line in content.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("api_key") {
                        let rest = rest.trim();
                        if let Some(rest) = rest.strip_prefix('=') {
                            let value = rest.trim().trim_matches('"').trim_matches('\'');
                            if !value.is_empty() {
                                return Ok(value.to_string());
                            }
                        }
                    }
                }
            }
        }

        Err(ProviderError::Unavailable(
            "E2B API key not found. Set E2B_API_KEY env var or add api_key to ~/.roche/e2b.toml"
                .into(),
        ))
    }


    /// Build the envd base URL for a given sandbox.
    fn envd_url(&self, sandbox_id: &str, domain: &str) -> String {
        format!("https://{ENVD_PORT}-{sandbox_id}.{domain}")
    }

    /// Build the Authorization header for envd (HTTP Basic with access token as user).
    fn envd_auth(access_token: &str) -> String {
        let credentials = format!("{access_token}:");
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
        format!("Basic {encoded}")
    }

    /// Look up sandbox metadata from local cache, or fetch from API if missing.
    async fn get_meta(&self, sandbox_id: &str) -> Result<(String, String), ProviderError> {
        // Try local cache first
        {
            let sandboxes = self.sandboxes.read().await;
            if let Some(meta) = sandboxes.get(sandbox_id) {
                return Ok((meta.envd_access_token.clone(), meta.domain.clone()));
            }
        }

        // Fetch from API
        let url = format!("{}/sandboxes/{}", self.api_base, sandbox_id);
        let resp = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("E2B API request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProviderError::NotFound(sandbox_id.to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ExecFailed(format!(
                "E2B API error ({status}): {body}"
            )));
        }

        let sandbox: SandboxResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("failed to parse response: {e}")))?;

        let token = sandbox.envd_access_token.clone();
        let domain = if sandbox.domain.is_empty() {
            self.domain.clone()
        } else {
            sandbox.domain.clone()
        };

        // Cache for future use
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(
            sandbox_id.to_string(),
            E2bSandboxMeta {
                envd_access_token: sandbox.envd_access_token,
                domain: domain.clone(),
                template_id: sandbox.template_id,
            },
        );

        Ok((token, domain))
    }

    /// Execute a command via envd Connect RPC (server-streaming).
    ///
    /// The Connect protocol server-streaming format:
    /// - Response body is a series of binary envelopes
    /// - Each envelope: 1 byte flags + 4 bytes big-endian length + JSON payload
    /// - flags=0x00: data message, flags=0x02: end-of-stream
    async fn envd_exec(
        &self,
        sandbox_id: &str,
        access_token: &str,
        domain: &str,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        let base_url = self.envd_url(sandbox_id, domain);
        let url = format!("{base_url}/process.Process/Start");

        let (cmd, args) = if request.command.is_empty() {
            return Err(ProviderError::ExecFailed("empty command".into()));
        } else if request.command.len() == 1 {
            // Single command: run via bash -c
            (
                "/bin/bash".to_string(),
                vec!["-c".to_string(), request.command[0].clone()],
            )
        } else {
            (request.command[0].clone(), request.command[1..].to_vec())
        };

        let body = StartRequest {
            process: ProcessConfig {
                cmd,
                args,
                envs: HashMap::new(),
            },
            stdin: Some(false),
        };

        let timeout_secs = request.timeout_secs.unwrap_or(300);

        let resp = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            self.client
                .post(&url)
                .header("Content-Type", "application/connect+json")
                .header("Connect-Protocol-Version", "1")
                .header("Authorization", Self::envd_auth(access_token))
                .json(&body)
                .send(),
        )
        .await
        .map_err(|_| ProviderError::Timeout(timeout_secs))?
        .map_err(|e| ProviderError::ExecFailed(format!("envd request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ExecFailed(format!(
                "envd error ({status}): {body}"
            )));
        }

        // Parse Connect server-streaming envelopes
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("failed to read response: {e}")))?;

        Self::parse_process_stream(&bytes)
    }

    /// Parse Connect RPC server-streaming binary envelopes into ExecOutput.
    fn parse_process_stream(data: &[u8]) -> Result<ExecOutput, ProviderError> {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code: Option<i32> = None;
        let mut offset = 0;

        let b64 = base64::engine::general_purpose::STANDARD;

        while offset + 5 <= data.len() {
            let flags = data[offset];
            let length = u32::from_be_bytes([
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
            ]) as usize;
            offset += 5;

            if offset + length > data.len() {
                break;
            }

            let payload = &data[offset..offset + length];
            offset += length;

            // flags=0x02 is end-of-stream trailer (may contain error)
            if flags & 0x02 != 0 {
                // End-of-stream envelope — check for errors
                if let Ok(trailer) = serde_json::from_slice::<serde_json::Value>(payload) {
                    if let Some(err) = trailer.get("error") {
                        if let Some(msg) = err.get("message").and_then(|m| m.as_str()) {
                            return Err(ProviderError::ExecFailed(format!(
                                "envd stream error: {msg}"
                            )));
                        }
                    }
                }
                continue;
            }

            // flags=0x00 is a data message
            let event: ProcessEvent = serde_json::from_slice(payload).map_err(|e| {
                ProviderError::ExecFailed(format!("failed to parse process event: {e}"))
            })?;

            if let Some(data_event) = &event.data {
                if let Some(ref out) = data_event.stdout {
                    if let Ok(decoded) = b64.decode(out) {
                        stdout.push_str(&String::from_utf8_lossy(&decoded));
                    }
                }
                if let Some(ref err) = data_event.stderr {
                    if let Ok(decoded) = b64.decode(err) {
                        stderr.push_str(&String::from_utf8_lossy(&decoded));
                    }
                }
            }

            if let Some(end_event) = &event.end {
                exit_code = Some(end_event.exit_code);
            }
        }

        Ok(ExecOutput {
            exit_code: exit_code.unwrap_or(-1),
            stdout,
            stderr,
        })
    }
}

impl Default for E2bProvider {
    fn default() -> Self {
        Self::new().expect("E2B_API_KEY must be set")
    }
}

impl SandboxProvider for E2bProvider {
    fn capabilities(&self) -> crate::provider::capabilities::ProviderCapabilities {
        use crate::provider::capabilities::{FieldSupport, ProviderCapabilities};
        ProviderCapabilities {
            name: "e2b".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Unsupported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Unsupported,
            cpus: FieldSupport::Unsupported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
        }
    }

    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        crate::provider::capabilities::validate_config(config, &self.capabilities())?;

        let body = NewSandbox {
            template_id: config.image.clone(),
            timeout: if config.timeout_secs > 0 {
                Some(config.timeout_secs)
            } else {
                None
            },
            allow_internet_access: Some(config.network),
            env_vars: if config.env.is_empty() {
                None
            } else {
                Some(config.env.clone())
            },
            metadata: Some(HashMap::from([(
                "roche.managed".to_string(),
                "true".to_string(),
            )])),
        };

        let url = format!("{}/sandboxes", self.api_base);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::CreateFailed(format!("E2B API request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<ApiError>(&body)
                .map(|e| e.message)
                .unwrap_or(body);
            return Err(ProviderError::CreateFailed(format!(
                "E2B API error ({status}): {msg}"
            )));
        }

        let sandbox: SandboxResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::CreateFailed(format!("failed to parse response: {e}")))?;

        let sandbox_id = sandbox.sandbox_id.clone();
        let domain = if sandbox.domain.is_empty() {
            self.domain.clone()
        } else {
            sandbox.domain
        };

        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(
            sandbox_id.clone(),
            E2bSandboxMeta {
                envd_access_token: sandbox.envd_access_token,
                domain,
                template_id: sandbox.template_id,
            },
        );

        Ok(sandbox_id)
    }

    async fn exec(
        &self,
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        let (access_token, domain) = self.get_meta(id).await?;
        self.envd_exec(id, &access_token, &domain, request).await
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError> {
        let url = format!("{}/sandboxes/{}", self.api_base, id);
        let resp = self
            .client
            .delete(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("E2B API request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProviderError::NotFound(id.clone()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ExecFailed(format!(
                "E2B API error ({status}): {body}"
            )));
        }

        // Remove from local cache
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.remove(id);

        Ok(())
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        let url = format!("{}/sandboxes", self.api_base);
        let resp = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Unavailable(format!("E2B API request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Unavailable(format!(
                "E2B API error ({status}): {body}"
            )));
        }

        let sandboxes: Vec<ListedSandbox> = resp
            .json()
            .await
            .map_err(|e| ProviderError::Unavailable(format!("failed to parse response: {e}")))?;

        Ok(sandboxes
            .into_iter()
            .map(|s| SandboxInfo {
                id: s.sandbox_id,
                status: SandboxStatus::Running,
                provider: "e2b".to_string(),
                image: s.template_id,
                expires_at: None,
            })
            .collect())
    }
}

impl SandboxLifecycle for E2bProvider {
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        let url = format!("{}/sandboxes/{}/pause", self.api_base, id);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("E2B API request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProviderError::NotFound(id.clone()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ExecFailed(format!(
                "E2B pause error ({status}): {body}"
            )));
        }
        Ok(())
    }

    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        let url = format!("{}/sandboxes/{}/connect", self.api_base, id);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .header("Content-Type", "application/json")
            .body(r#"{"timeout":300}"#)
            .send()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("E2B API request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProviderError::NotFound(id.clone()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ExecFailed(format!(
                "E2B resume error ({status}): {body}"
            )));
        }

        // Update cached metadata if sandbox was resumed
        if let Ok(sandbox) = resp.json::<SandboxResponse>().await {
            let domain = if sandbox.domain.is_empty() {
                self.domain.clone()
            } else {
                sandbox.domain
            };
            let mut sandboxes = self.sandboxes.write().await;
            sandboxes.insert(
                id.clone(),
                E2bSandboxMeta {
                    envd_access_token: sandbox.envd_access_token,
                    domain,
                    template_id: sandbox.template_id,
                },
            );
        }
        Ok(())
    }

    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError> {
        // E2B manages sandbox lifecycle via timeouts on their side.
        // No local GC needed — return empty list.
        Ok(Vec::new())
    }
}

impl SandboxFileOps for E2bProvider {
    async fn copy_to(
        &self,
        id: &SandboxId,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), ProviderError> {
        let (access_token, domain) = self.get_meta(id).await?;
        let base_url = self.envd_url(id, &domain);
        let url = format!("{base_url}/files");

        let file_content = tokio::fs::read(src)
            .await
            .map_err(|e| ProviderError::FileFailed(format!("failed to read local file: {e}")))?;

        let file_name = std::path::Path::new(dest)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");

        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(file_content).file_name(file_name.to_string()),
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", Self::envd_auth(&access_token))
            .query(&[("path", dest)])
            .multipart(form)
            .send()
            .await
            .map_err(|e| ProviderError::FileFailed(format!("envd file upload failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::FileFailed(format!(
                "envd file upload error ({status}): {body}"
            )));
        }

        Ok(())
    }

    async fn copy_from(
        &self,
        id: &SandboxId,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), ProviderError> {
        let (access_token, domain) = self.get_meta(id).await?;
        let base_url = self.envd_url(id, &domain);
        let url = format!("{base_url}/files");

        let resp = self
            .client
            .get(&url)
            .header("Authorization", Self::envd_auth(&access_token))
            .query(&[("path", src)])
            .send()
            .await
            .map_err(|e| ProviderError::FileFailed(format!("envd file download failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProviderError::FileFailed(format!(
                "file not found in sandbox: {src}"
            )));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::FileFailed(format!(
                "envd file download error ({status}): {body}"
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::FileFailed(format!("failed to read response: {e}")))?;

        tokio::fs::write(dest, &bytes)
            .await
            .map_err(|e| ProviderError::FileFailed(format!("failed to write local file: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_config_defaults_rejected() {
        // Default config has writable=false, which E2B doesn't support
        use crate::provider::capabilities::{self, FieldSupport, ProviderCapabilities};
        let caps = ProviderCapabilities {
            name: "e2b".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Unsupported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Unsupported,
            cpus: FieldSupport::Unsupported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
        };
        let config = SandboxConfig::default();
        let result = capabilities::validate_config(&config, &caps);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProviderError::Unsupported(_))));
    }

    #[test]
    fn test_validate_config_writable_ok() {
        use crate::provider::capabilities::{self, FieldSupport, ProviderCapabilities};
        let caps = ProviderCapabilities {
            name: "e2b".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Unsupported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Unsupported,
            cpus: FieldSupport::Unsupported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
        };
        let config = SandboxConfig {
            writable: true,
            ..Default::default()
        };
        assert!(capabilities::validate_config(&config, &caps).is_ok());
    }

    #[test]
    fn test_validate_config_memory_rejected() {
        use crate::provider::capabilities::{self, FieldSupport, ProviderCapabilities};
        let caps = ProviderCapabilities {
            name: "e2b".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Unsupported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Unsupported,
            cpus: FieldSupport::Unsupported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
        };
        let config = SandboxConfig {
            writable: true,
            memory: Some("512m".into()),
            ..Default::default()
        };
        let result = capabilities::validate_config(&config, &caps);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProviderError::Unsupported(_))));
    }

    #[test]
    fn test_validate_config_cpus_rejected() {
        use crate::provider::capabilities::{self, FieldSupport, ProviderCapabilities};
        let caps = ProviderCapabilities {
            name: "e2b".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Unsupported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Unsupported,
            cpus: FieldSupport::Unsupported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
        };
        let config = SandboxConfig {
            writable: true,
            cpus: Some(2.0),
            ..Default::default()
        };
        let result = capabilities::validate_config(&config, &caps);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_config_mounts_rejected() {
        use crate::provider::capabilities::{self, FieldSupport, ProviderCapabilities};
        use crate::types::MountConfig;
        let caps = ProviderCapabilities {
            name: "e2b".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Unsupported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Unsupported,
            cpus: FieldSupport::Unsupported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: true,
            unpause: true,
            copy_to: true,
            copy_from: true,
        };
        let config = SandboxConfig {
            writable: true,
            mounts: vec![MountConfig {
                host_path: "/tmp".into(),
                container_path: "/data".into(),
                readonly: true,
            }],
            ..Default::default()
        };
        let result = capabilities::validate_config(&config, &caps);
        assert!(result.is_err());
    }

    #[test]
    fn test_envd_auth_header() {
        let auth = E2bProvider::envd_auth("test-token");
        let expected_b64 = base64::engine::general_purpose::STANDARD.encode("test-token:");
        assert_eq!(auth, format!("Basic {expected_b64}"));
    }

    #[test]
    fn test_parse_process_stream_empty() {
        let result = E2bProvider::parse_process_stream(&[]);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.exit_code, -1);
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn test_parse_process_stream_with_events() {
        let b64 = base64::engine::general_purpose::STANDARD;

        // Build a stream with: start event, stdout data, end event
        let mut stream = Vec::new();

        // Start event
        let start = br#"{"start":{"pid":42}}"#;
        stream.push(0x00u8); // flags
        stream.extend_from_slice(&(start.len() as u32).to_be_bytes());
        stream.extend_from_slice(start);

        // Stdout data event
        let hello_b64 = b64.encode(b"hello world\n");
        let data = format!(r#"{{"data":{{"stdout":"{hello_b64}"}}}}"#);
        let data_bytes = data.as_bytes();
        stream.push(0x00u8);
        stream.extend_from_slice(&(data_bytes.len() as u32).to_be_bytes());
        stream.extend_from_slice(data_bytes);

        // End event
        let end = br#"{"end":{"exitCode":0,"exited":true,"status":"exited"}}"#;
        stream.push(0x00u8);
        stream.extend_from_slice(&(end.len() as u32).to_be_bytes());
        stream.extend_from_slice(end);

        // End-of-stream trailer
        let trailer = b"{}";
        stream.push(0x02u8);
        stream.extend_from_slice(&(trailer.len() as u32).to_be_bytes());
        stream.extend_from_slice(trailer);

        let result = E2bProvider::parse_process_stream(&stream);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, "hello world\n");
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn test_parse_process_stream_with_stderr() {
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut stream = Vec::new();

        let err_b64 = b64.encode(b"error occurred");
        let data = format!(r#"{{"data":{{"stderr":"{err_b64}"}}}}"#);
        let data_bytes = data.as_bytes();
        stream.push(0x00u8);
        stream.extend_from_slice(&(data_bytes.len() as u32).to_be_bytes());
        stream.extend_from_slice(data_bytes);

        let end = br#"{"end":{"exitCode":1,"exited":true,"status":"exited"}}"#;
        stream.push(0x00u8);
        stream.extend_from_slice(&(end.len() as u32).to_be_bytes());
        stream.extend_from_slice(end);

        let result = E2bProvider::parse_process_stream(&stream);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.exit_code, 1);
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, "error occurred");
    }

    #[test]
    fn test_parse_process_stream_error_trailer() {
        let mut stream = Vec::new();

        let trailer = br#"{"error":{"message":"process killed"}}"#;
        stream.push(0x02u8);
        stream.extend_from_slice(&(trailer.len() as u32).to_be_bytes());
        stream.extend_from_slice(trailer);

        let result = E2bProvider::parse_process_stream(&stream);
        assert!(result.is_err());
        assert!(matches!(result, Err(ProviderError::ExecFailed(_))));
    }

    #[test]
    fn test_new_sandbox_serialization() {
        let body = NewSandbox {
            template_id: "Python3".to_string(),
            timeout: Some(300),
            allow_internet_access: Some(false),
            env_vars: None,
            metadata: Some(HashMap::from([("roche.managed".into(), "true".into())])),
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["templateId"], "Python3");
        assert_eq!(json["timeout"], 300);
        assert_eq!(json["allowInternetAccess"], false);
        assert!(json.get("envVars").is_none()); // skipped when None
    }
}
