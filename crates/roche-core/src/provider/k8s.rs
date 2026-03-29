// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

// Kubernetes sandbox provider for Roche.

use crate::provider::{ProviderError, SandboxFileOps, SandboxLifecycle, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo, SandboxStatus};
use k8s_openapi::api::core::v1::{
    Container, EmptyDirVolumeSource, EnvVar, Namespace, Pod, PodSpec, ResourceRequirements,
    SecurityContext, Volume, VolumeMount,
};
use k8s_openapi::api::networking::v1::{NetworkPolicy, NetworkPolicySpec};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{AttachParams, DeleteParams, ListParams, PostParams};
use kube::core::ObjectMeta;
use std::collections::BTreeMap;
use std::io::Read as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DEFAULT_NAMESPACE: &str = "roche-sandboxes";
const EXIT_SENTINEL: &str = "ROCHE_EXIT:";

/// Resolve the Kubernetes namespace for Roche sandboxes.
///
/// Resolution order:
/// 1. `ROCHE_K8S_NAMESPACE` environment variable
/// 2. `~/.roche/k8s.toml` config file (`namespace` field)
/// 3. Default: `"roche-sandboxes"`
fn resolve_namespace() -> String {
    // 1. Environment variable
    if let Ok(ns) = std::env::var("ROCHE_K8S_NAMESPACE") {
        if !ns.is_empty() {
            return ns;
        }
    }

    // 2. Config file fallback
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(".roche").join("k8s.toml");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(table) = content.parse::<toml::Table>() {
                    if let Some(ns) = table.get("namespace").and_then(|v| v.as_str()) {
                        if !ns.is_empty() {
                            return ns.to_string();
                        }
                    }
                }
            }
        }
    }

    // 3. Default
    DEFAULT_NAMESPACE.to_string()
}

/// Kubernetes sandbox provider.
///
/// Runs each sandbox as an isolated Pod with network policies,
/// security contexts, and resource limits.
pub struct K8sProvider {
    client: kube::Client,
    namespace: String,
}

impl K8sProvider {
    /// Create a new K8sProvider.
    ///
    /// Infers kube config from the environment (in-cluster or kubeconfig),
    /// ensures the target namespace exists, and warns about CNI requirements.
    pub async fn new() -> Result<Self, ProviderError> {
        let client = kube::Client::try_default().await.map_err(|e| {
            ProviderError::Unavailable(format!("failed to create Kubernetes client: {e}"))
        })?;

        let namespace = resolve_namespace();

        // Ensure namespace exists (create if missing)
        let ns_api: kube::Api<Namespace> = kube::Api::all(client.clone());
        let ns_obj = Namespace {
            metadata: ObjectMeta {
                name: Some(namespace.clone()),
                labels: Some(BTreeMap::from([(
                    "roche.managed".to_string(),
                    "true".to_string(),
                )])),
                ..Default::default()
            },
            ..Default::default()
        };
        match ns_api
            .create(&kube::api::PostParams::default(), &ns_obj)
            .await
        {
            Ok(_) => {
                tracing::info!(namespace = %namespace, "created Roche namespace");
            }
            Err(kube::Error::Api(ae)) if ae.code == 409 => {
                // Already exists — fine
                tracing::debug!(namespace = %namespace, "namespace already exists");
            }
            Err(e) => {
                return Err(ProviderError::Unavailable(format!(
                    "failed to ensure namespace '{namespace}': {e}"
                )));
            }
        }

        // Warn about CNI requirement for NetworkPolicy enforcement
        tracing::warn!(
            "K8s provider requires a CNI plugin (e.g. Calico, Cilium) for NetworkPolicy enforcement. \
             Without one, network isolation will NOT be enforced."
        );

        Ok(Self { client, namespace })
    }

    fn pods_api(&self) -> kube::Api<Pod> {
        kube::Api::namespaced(self.client.clone(), &self.namespace)
    }

    fn netpol_api(&self) -> kube::Api<NetworkPolicy> {
        kube::Api::namespaced(self.client.clone(), &self.namespace)
    }
}

impl SandboxProvider for K8sProvider {
    fn capabilities(&self) -> crate::provider::capabilities::ProviderCapabilities {
        use crate::provider::capabilities::{FieldSupport, ProviderCapabilities};
        ProviderCapabilities {
            name: "k8s".into(),
            writable_true: FieldSupport::Supported,
            writable_false: FieldSupport::Supported,
            network: FieldSupport::Supported,
            mounts: FieldSupport::Unsupported,
            memory: FieldSupport::Supported,
            cpus: FieldSupport::Supported,
            kernel: FieldSupport::NotApplicable,
            rootfs: FieldSupport::NotApplicable,
            pause: false,
            unpause: false,
            copy_to: true,
            copy_from: true,
        }
    }

    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        crate::provider::capabilities::validate_config(config, &self.capabilities())?;

        let pod_name = format!("roche-{}", uuid::Uuid::new_v4());
        let pod = build_pod(&pod_name, &self.namespace, config);

        // Create the pod
        self.pods_api()
            .create(&PostParams::default(), &pod)
            .await
            .map_err(|e| ProviderError::CreateFailed(format!("failed to create pod: {e}")))?;

        // Create NetworkPolicy if network is disabled
        if !config.network {
            let netpol = build_deny_all_network_policy(&pod_name, &self.namespace);
            self.netpol_api()
                .create(&PostParams::default(), &netpol)
                .await
                .map_err(|e| {
                    ProviderError::CreateFailed(format!("failed to create network policy: {e}"))
                })?;
        }

        // Poll for pod to become Running (up to 60s, 500ms interval)
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        let interval = std::time::Duration::from_millis(500);

        loop {
            if tokio::time::Instant::now() >= deadline {
                // Timeout — cleanup and return error
                let _ = self.destroy(&pod_name).await;
                return Err(ProviderError::Timeout(60));
            }

            match self.pods_api().get(&pod_name).await {
                Ok(pod) => {
                    let phase = pod.status.as_ref().and_then(|s| s.phase.as_deref());

                    match phase {
                        Some("Running") => return Ok(pod_name),
                        Some("Failed") | Some("Succeeded") => {
                            let _ = self.destroy(&pod_name).await;
                            return Err(ProviderError::CreateFailed(format!(
                                "pod entered {phase} phase",
                                phase = phase.unwrap()
                            )));
                        }
                        _ => {
                            // Still pending — wait and retry
                            tokio::time::sleep(interval).await;
                        }
                    }
                }
                Err(e) => {
                    let _ = self.destroy(&pod_name).await;
                    return Err(ProviderError::CreateFailed(format!(
                        "failed to get pod status: {e}"
                    )));
                }
            }
        }
    }

    async fn exec(
        &self,
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        // Build command: sh -c '<escaped_cmd>; echo "ROCHE_EXIT:$?"'
        let escaped_parts: Vec<String> = request.command.iter().map(|s| shell_escape(s)).collect();
        let joined_cmd = escaped_parts.join(" ");
        let wrapped_cmd = format!("{joined_cmd}; echo \"{EXIT_SENTINEL}$?\"");
        let command = vec!["sh".to_string(), "-c".to_string(), wrapped_cmd];

        let attach_params = AttachParams::default()
            .container("sandbox")
            .stdout(true)
            .stderr(true);

        let exec_future = async {
            let mut attached = self
                .pods_api()
                .exec(id, command, &attach_params)
                .await
                .map_err(|e| match &e {
                    kube::Error::Api(ae) if ae.code == 404 => ProviderError::NotFound(id.clone()),
                    _ => ProviderError::ExecFailed(format!("failed to exec in pod: {e}")),
                })?;

            let mut stdout_reader = attached
                .stdout()
                .ok_or_else(|| ProviderError::ExecFailed("no stdout stream".to_string()))?;
            let mut stderr_reader = attached
                .stderr()
                .ok_or_else(|| ProviderError::ExecFailed("no stderr stream".to_string()))?;

            // Read stdout and stderr concurrently to avoid deadlocks
            let (stdout_result, stderr_result) = tokio::try_join!(
                async {
                    let mut buf = Vec::new();
                    stdout_reader.read_to_end(&mut buf).await.map_err(|e| {
                        ProviderError::ExecFailed(format!("stdout read error: {e}"))
                    })?;
                    Ok::<_, ProviderError>(String::from_utf8_lossy(&buf).to_string())
                },
                async {
                    let mut buf = Vec::new();
                    stderr_reader.read_to_end(&mut buf).await.map_err(|e| {
                        ProviderError::ExecFailed(format!("stderr read error: {e}"))
                    })?;
                    Ok::<_, ProviderError>(String::from_utf8_lossy(&buf).to_string())
                }
            )?;

            let (exit_code, clean_stdout) = parse_exit_sentinel(&stdout_result);

            Ok(ExecOutput {
                exit_code,
                stdout: clean_stdout,
                stderr: stderr_result,
            })
        };

        // Apply timeout if requested
        if let Some(timeout_secs) = request.timeout_secs {
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), exec_future)
                .await
                .map_err(|_| ProviderError::Timeout(timeout_secs))?
        } else {
            exec_future.await
        }
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError> {
        // Delete the pod
        match self.pods_api().delete(id, &DeleteParams::default()).await {
            Ok(_) => {}
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                return Err(ProviderError::NotFound(id.clone()));
            }
            Err(e) => {
                return Err(ProviderError::ExecFailed(format!(
                    "failed to delete pod: {e}"
                )));
            }
        }

        // Delete NetworkPolicy (ignore NotFound — may not exist if network=true)
        let netpol_name = format!("roche-deny-{id}");
        match self
            .netpol_api()
            .delete(&netpol_name, &DeleteParams::default())
            .await
        {
            Ok(_) => {}
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                // Network policy doesn't exist — that's fine
            }
            Err(e) => {
                return Err(ProviderError::ExecFailed(format!(
                    "failed to delete network policy: {e}"
                )));
            }
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        let lp = ListParams::default().labels("roche.managed=true");
        let pods = self
            .pods_api()
            .list(&lp)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("failed to list pods: {e}")))?;

        let mut infos = Vec::new();
        for pod in pods.items {
            let name = pod.metadata.name.unwrap_or_default();
            let labels = pod.metadata.labels.unwrap_or_default();
            let annotations = pod.metadata.annotations.unwrap_or_default();

            let image = labels.get("roche.image").cloned().unwrap_or_default();

            let expires_at = annotations
                .get("roche.expires")
                .and_then(|v| v.parse::<u64>().ok());

            let phase = pod
                .status
                .as_ref()
                .and_then(|s| s.phase.as_deref())
                .map(String::from);

            let status = pod_phase_to_status(phase.as_deref());

            infos.push(SandboxInfo {
                id: name,
                status,
                provider: "k8s".to_string(),
                image,
                expires_at,
            });
        }

        Ok(infos)
    }
}

impl SandboxLifecycle for K8sProvider {
    async fn pause(&self, _id: &SandboxId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported(
            "K8s does not support Pod pause".to_string(),
        ))
    }

    async fn unpause(&self, _id: &SandboxId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported(
            "K8s does not support Pod unpause".to_string(),
        ))
    }

    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError> {
        let lp = ListParams::default().labels("roche.managed=true");
        let pods = self
            .pods_api()
            .list(&lp)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("Failed to list Pods: {e}")))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut destroyed = Vec::new();
        for pod in pods.items {
            let name = match pod.metadata.name {
                Some(n) => n,
                None => continue,
            };
            let annotations = pod.metadata.annotations.unwrap_or_default();
            if let Some(expires_str) = annotations.get("roche.expires") {
                if let Ok(expires_at) = expires_str.parse::<u64>() {
                    if expires_at <= now && self.destroy(&name).await.is_ok() {
                        destroyed.push(name);
                    }
                }
            }
        }
        Ok(destroyed)
    }
}

impl SandboxFileOps for K8sProvider {
    async fn copy_to(
        &self,
        id: &SandboxId,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), ProviderError> {
        // Read the local file
        let file_data = tokio::fs::read(src).await.map_err(|e| {
            ProviderError::FileFailed(format!("failed to read local file {}: {e}", src.display()))
        })?;

        let file_name = src
            .file_name()
            .ok_or_else(|| ProviderError::FileFailed("source path has no filename".to_string()))?
            .to_string_lossy();

        // Create tar archive in memory
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let mut header = tar::Header::new_gnu();
            header.set_size(file_data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, file_name.as_ref(), &file_data[..])
                .map_err(|e| ProviderError::FileFailed(format!("failed to create tar: {e}")))?;
            builder
                .finish()
                .map_err(|e| ProviderError::FileFailed(format!("failed to finish tar: {e}")))?;
        }

        // Determine parent directory of destination
        let dest_path = std::path::Path::new(dest);
        let parent_dir = dest_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        // Exec tar -xf - -C {parent_dir} in the pod
        let command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("tar -xf - -C {}", shell_escape(&parent_dir)),
        ];

        let attach_params = AttachParams::default()
            .container("sandbox")
            .stdin(true)
            .stdout(true)
            .stderr(true);

        let mut attached = self
            .pods_api()
            .exec(id, command, &attach_params)
            .await
            .map_err(|e| match &e {
                kube::Error::Api(ae) if ae.code == 404 => ProviderError::NotFound(id.clone()),
                _ => ProviderError::FileFailed(format!("failed to exec tar in pod: {e}")),
            })?;

        // Write tar data to stdin
        let mut stdin_writer = attached
            .stdin()
            .ok_or_else(|| ProviderError::FileFailed("no stdin stream".to_string()))?;
        stdin_writer
            .write_all(&tar_buf)
            .await
            .map_err(|e| ProviderError::FileFailed(format!("failed to write tar to stdin: {e}")))?;
        stdin_writer
            .shutdown()
            .await
            .map_err(|e| ProviderError::FileFailed(format!("failed to close stdin: {e}")))?;

        // Read stderr to check for errors
        if let Some(mut stderr_reader) = attached.stderr() {
            let mut stderr_buf = Vec::new();
            let _ = stderr_reader.read_to_end(&mut stderr_buf).await;
            let stderr_str = String::from_utf8_lossy(&stderr_buf);
            if !stderr_str.is_empty() {
                return Err(ProviderError::FileFailed(format!(
                    "tar extraction failed: {stderr_str}"
                )));
            }
        }

        Ok(())
    }

    async fn copy_from(
        &self,
        id: &SandboxId,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), ProviderError> {
        let src_path = std::path::Path::new(src);
        let parent_dir = src_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        let filename = src_path
            .file_name()
            .ok_or_else(|| ProviderError::FileFailed("source path has no filename".to_string()))?
            .to_string_lossy();

        // Exec tar -cf - -C {parent} {filename} in the pod
        let command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!(
                "tar -cf - -C {} {}",
                shell_escape(&parent_dir),
                shell_escape(&filename)
            ),
        ];

        let attach_params = AttachParams::default()
            .container("sandbox")
            .stdout(true)
            .stderr(true);

        let mut attached = self
            .pods_api()
            .exec(id, command, &attach_params)
            .await
            .map_err(|e| match &e {
                kube::Error::Api(ae) if ae.code == 404 => ProviderError::NotFound(id.clone()),
                _ => ProviderError::FileFailed(format!("failed to exec tar in pod: {e}")),
            })?;

        // Read stdout (tar data)
        let mut stdout_reader = attached
            .stdout()
            .ok_or_else(|| ProviderError::FileFailed("no stdout stream".to_string()))?;
        let mut tar_buf = Vec::new();
        stdout_reader.read_to_end(&mut tar_buf).await.map_err(|e| {
            ProviderError::FileFailed(format!("failed to read tar from stdout: {e}"))
        })?;

        // Extract tar to local path
        let mut archive = tar::Archive::new(&tar_buf[..]);
        let mut entries = archive
            .entries()
            .map_err(|e| ProviderError::FileFailed(format!("failed to read tar entries: {e}")))?;

        if let Some(entry_result) = entries.next() {
            let mut entry = entry_result
                .map_err(|e| ProviderError::FileFailed(format!("failed to read tar entry: {e}")))?;
            let mut data = Vec::new();
            entry.read_to_end(&mut data).map_err(|e| {
                ProviderError::FileFailed(format!("failed to read tar entry data: {e}"))
            })?;
            tokio::fs::write(dest, &data).await.map_err(|e| {
                ProviderError::FileFailed(format!("failed to write to {}: {e}", dest.display()))
            })?;
        } else {
            return Err(ProviderError::FileFailed(
                "tar archive is empty — file not found in sandbox".to_string(),
            ));
        }

        Ok(())
    }
}

/// Build a Pod spec for a Roche sandbox.
fn build_pod(name: &str, namespace: &str, config: &crate::types::SandboxConfig) -> Pod {
    let mut labels = BTreeMap::new();
    labels.insert("roche.managed".to_string(), "true".to_string());
    labels.insert("roche.image".to_string(), config.image.clone());
    labels.insert("roche.sandbox".to_string(), name.to_string());

    let mut annotations = BTreeMap::new();
    if config.timeout_secs > 0 {
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + config.timeout_secs;
        annotations.insert("roche.expires".to_string(), expires_at.to_string());
    }

    // Security context
    let security_context = if config.writable {
        SecurityContext {
            allow_privilege_escalation: Some(false),
            run_as_non_root: Some(true),
            run_as_user: Some(1000),
            ..Default::default()
        }
    } else {
        SecurityContext {
            read_only_root_filesystem: Some(true),
            allow_privilege_escalation: Some(false),
            run_as_non_root: Some(true),
            run_as_user: Some(1000),
            ..Default::default()
        }
    };

    // Resource limits
    let mut limits = BTreeMap::new();
    if let Some(ref memory) = config.memory {
        limits.insert("memory".to_string(), Quantity(memory.clone()));
    }
    if let Some(cpus) = config.cpus {
        let millicores = (cpus * 1000.0) as u32;
        limits.insert("cpu".to_string(), Quantity(format!("{millicores}m")));
    }
    let resources = if limits.is_empty() {
        None
    } else {
        Some(ResourceRequirements {
            limits: Some(limits),
            ..Default::default()
        })
    };

    // Environment variables
    let env: Vec<EnvVar> = config
        .env
        .iter()
        .map(|(k, v)| EnvVar {
            name: k.clone(),
            value: Some(v.clone()),
            ..Default::default()
        })
        .collect();

    // Volumes and mounts for read-only root filesystem
    let mut volumes = Vec::new();
    let mut volume_mounts = Vec::new();
    if !config.writable {
        volumes.push(Volume {
            name: "tmp".to_string(),
            empty_dir: Some(EmptyDirVolumeSource {
                size_limit: Some(Quantity("64Mi".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        });
        volume_mounts.push(VolumeMount {
            name: "tmp".to_string(),
            mount_path: "/tmp".to_string(),
            ..Default::default()
        });
    }

    let container = Container {
        name: "sandbox".to_string(),
        image: Some(config.image.clone()),
        command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        security_context: Some(security_context),
        resources,
        env: if env.is_empty() { None } else { Some(env) },
        volume_mounts: if volume_mounts.is_empty() {
            None
        } else {
            Some(volume_mounts)
        },
        ..Default::default()
    };

    let active_deadline = if config.timeout_secs > 0 {
        Some(config.timeout_secs as i64)
    } else {
        None
    };

    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            labels: Some(labels),
            annotations: if annotations.is_empty() {
                None
            } else {
                Some(annotations)
            },
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![container],
            volumes: if volumes.is_empty() {
                None
            } else {
                Some(volumes)
            },
            active_deadline_seconds: active_deadline,
            restart_policy: Some("Never".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Build a deny-all NetworkPolicy for a sandbox pod.
fn build_deny_all_network_policy(pod_name: &str, namespace: &str) -> NetworkPolicy {
    let policy_name = format!("roche-deny-{pod_name}");
    NetworkPolicy {
        metadata: ObjectMeta {
            name: Some(policy_name),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: Some(NetworkPolicySpec {
            pod_selector: LabelSelector {
                match_labels: Some(BTreeMap::from([(
                    "roche.sandbox".to_string(),
                    pod_name.to_string(),
                )])),
                ..Default::default()
            },
            policy_types: Some(vec!["Ingress".to_string(), "Egress".to_string()]),
            ingress: None,
            egress: None,
        }),
    }
}

/// Map a Kubernetes pod phase string to a Roche SandboxStatus.
fn pod_phase_to_status(phase: Option<&str>) -> SandboxStatus {
    match phase {
        Some("Running") => SandboxStatus::Running,
        Some("Pending") => SandboxStatus::Running,
        Some("Succeeded") => SandboxStatus::Stopped,
        Some("Failed") => SandboxStatus::Failed,
        _ => SandboxStatus::Stopped,
    }
}

/// Escape a string for safe use in a shell command.
fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/' || c == '.')
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Parse the exit sentinel from command output.
///
/// The sentinel format is `ROCHE_EXIT:<code>\n` as the last line.
/// Returns (exit_code, output_without_sentinel).
fn parse_exit_sentinel(stdout: &str) -> (i32, String) {
    if let Some(pos) = stdout.rfind(EXIT_SENTINEL) {
        let sentinel_line = &stdout[pos + EXIT_SENTINEL.len()..];
        let code_str = sentinel_line.trim();
        if let Ok(code) = code_str.parse::<i32>() {
            let output = stdout[..pos].to_string();
            return (code, output);
        }
    }
    (-1, stdout.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SandboxConfig;
    use std::collections::HashMap;

    // --- Task 2 tests: config resolution ---
    // These tests modify env vars so must not run in parallel.
    use std::sync::Mutex;
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_resolve_namespace_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::remove_var("ROCHE_K8S_NAMESPACE");
        let ns = resolve_namespace();
        assert_eq!(ns, DEFAULT_NAMESPACE);
    }

    #[test]
    fn test_resolve_namespace_from_env() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::set_var("ROCHE_K8S_NAMESPACE", "custom-ns");
        let ns = resolve_namespace();
        assert_eq!(ns, "custom-ns");
        std::env::remove_var("ROCHE_K8S_NAMESPACE");
    }

    // --- Task 3 tests: pod spec builder ---

    #[test]
    fn test_build_pod_default_config() {
        let config = SandboxConfig::default();
        let pod = build_pod("test-pod", "roche-sandboxes", &config);

        // Check labels
        let labels = pod.metadata.labels.as_ref().unwrap();
        assert_eq!(labels.get("roche.managed").unwrap(), "true");
        assert_eq!(labels.get("roche.image").unwrap(), &config.image);
        assert_eq!(labels.get("roche.sandbox").unwrap(), "test-pod");

        // Check security context
        let container = &pod.spec.as_ref().unwrap().containers[0];
        let sc = container.security_context.as_ref().unwrap();
        assert_eq!(sc.read_only_root_filesystem, Some(true));
        assert_eq!(sc.allow_privilege_escalation, Some(false));
        assert_eq!(sc.run_as_non_root, Some(true));
        assert_eq!(sc.run_as_user, Some(1000));

        // Check /tmp volume mount
        let volumes = pod.spec.as_ref().unwrap().volumes.as_ref().unwrap();
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].name, "tmp");
        let vm = container.volume_mounts.as_ref().unwrap();
        assert_eq!(vm[0].mount_path, "/tmp");

        // Check sleep infinity command
        let cmd = container.command.as_ref().unwrap();
        assert_eq!(cmd, &["sleep", "infinity"]);
    }

    #[test]
    fn test_build_pod_with_resources_and_env() {
        let config = SandboxConfig {
            memory: Some("256Mi".to_string()),
            cpus: Some(1.5),
            writable: true,
            env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            ..Default::default()
        };
        let pod = build_pod("res-pod", "roche-sandboxes", &config);

        let container = &pod.spec.as_ref().unwrap().containers[0];

        // Check resources
        let limits = container
            .resources
            .as_ref()
            .unwrap()
            .limits
            .as_ref()
            .unwrap();
        assert_eq!(limits.get("memory").unwrap().0, "256Mi");
        assert_eq!(limits.get("cpu").unwrap().0, "1500m");

        // Check env vars
        let env = container.env.as_ref().unwrap();
        assert!(env
            .iter()
            .any(|e| e.name == "FOO" && e.value == Some("bar".to_string())));

        // writable=true means no readOnlyRootFilesystem
        let sc = container.security_context.as_ref().unwrap();
        assert!(sc.read_only_root_filesystem.is_none());

        // No /tmp volume when writable
        assert!(pod.spec.as_ref().unwrap().volumes.is_none());
    }

    #[test]
    fn test_build_pod_with_timeout() {
        let config = SandboxConfig {
            timeout_secs: 600,
            ..Default::default()
        };
        let pod = build_pod("timeout-pod", "roche-sandboxes", &config);

        // Check activeDeadlineSeconds
        assert_eq!(
            pod.spec.as_ref().unwrap().active_deadline_seconds,
            Some(600)
        );

        // Check roche.expires annotation exists
        let annotations = pod.metadata.annotations.as_ref().unwrap();
        assert!(annotations.contains_key("roche.expires"));
    }

    #[test]
    fn test_build_pod_no_timeout() {
        let config = SandboxConfig {
            timeout_secs: 0,
            ..Default::default()
        };
        let pod = build_pod("no-timeout-pod", "roche-sandboxes", &config);

        // No activeDeadlineSeconds when timeout_secs=0
        assert!(pod.spec.as_ref().unwrap().active_deadline_seconds.is_none());

        // No roche.expires annotation
        assert!(pod.metadata.annotations.is_none());
    }

    // --- Task 4 tests: NetworkPolicy, status mapping, exit sentinel ---

    #[test]
    fn test_build_network_policy() {
        let np = build_deny_all_network_policy("my-pod", "roche-sandboxes");

        assert_eq!(np.metadata.name.as_deref(), Some("roche-deny-my-pod"));
        assert_eq!(np.metadata.namespace.as_deref(), Some("roche-sandboxes"));

        let spec = np.spec.as_ref().unwrap();
        let match_labels = spec.pod_selector.match_labels.as_ref().unwrap();
        assert_eq!(match_labels.get("roche.sandbox").unwrap(), "my-pod");

        let policy_types = spec.policy_types.as_ref().unwrap();
        assert_eq!(policy_types, &["Ingress", "Egress"]);

        assert!(spec.ingress.is_none());
        assert!(spec.egress.is_none());
    }

    #[test]
    fn test_pod_phase_to_status() {
        assert_eq!(pod_phase_to_status(Some("Running")), SandboxStatus::Running);
        assert_eq!(pod_phase_to_status(Some("Pending")), SandboxStatus::Running);
        assert_eq!(
            pod_phase_to_status(Some("Succeeded")),
            SandboxStatus::Stopped
        );
        assert_eq!(pod_phase_to_status(Some("Failed")), SandboxStatus::Failed);
        assert_eq!(pod_phase_to_status(None), SandboxStatus::Stopped);
    }

    #[test]
    fn test_parse_exit_code() {
        let (code, output) = parse_exit_sentinel("hello\nROCHE_EXIT:0\n");
        assert_eq!(code, 0);
        assert_eq!(output, "hello\n");
    }

    #[test]
    fn test_parse_exit_code_nonzero() {
        let (code, output) = parse_exit_sentinel("error\nROCHE_EXIT:127\n");
        assert_eq!(code, 127);
        assert_eq!(output, "error\n");
    }

    #[test]
    fn test_parse_exit_code_missing() {
        let (code, output) = parse_exit_sentinel("no sentinel\n");
        assert_eq!(code, -1);
        assert_eq!(output, "no sentinel\n");
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "hello");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("/usr/bin/python3"), "/usr/bin/python3");
    }
}
