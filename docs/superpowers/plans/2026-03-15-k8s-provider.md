# Kubernetes Provider Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Kubernetes provider to Roche that creates sandbox Pods with AI-safe security defaults, behind a `k8s` feature flag.

**Architecture:** Each sandbox is a Pod in a dedicated namespace (`roche-sandboxes`). Uses `kube` crate (v3.0) for all K8s API interactions. NetworkPolicy enforces `network=false`. SecurityContext enforces readonly FS and no-privilege-escalation. Exit codes captured via `sh -c` wrapper with sentinel parsing.

**Tech Stack:** `kube` 3.0, `k8s-openapi` 0.27, `tokio`, `uuid`

**Spec:** `docs/superpowers/specs/2026-03-14-k8s-provider-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/roche-core/Cargo.toml` | Modify | Add `kube`, `k8s-openapi` deps + `k8s` feature flag |
| `crates/roche-core/src/provider/mod.rs` | Modify | Add `#[cfg(feature = "k8s")] pub mod k8s;` |
| `crates/roche-core/src/provider/k8s.rs` | Create | K8sProvider struct, config, Pod/NetworkPolicy builders, all trait impls |
| `crates/roche-cli/Cargo.toml` | Modify | Enable `k8s` feature |
| `crates/roche-cli/src/main.rs` | Modify | Add `"k8s"` provider dispatch arm with Cp support |
| `crates/roche-daemon/Cargo.toml` | Modify | Enable `k8s` feature |
| `crates/roche-daemon/src/server.rs` | Modify | Add `K8sProvider` to `SandboxServiceImpl` + `with_provider!` macro |

---

## Chunk 1: Dependencies and Scaffolding

### Task 1: Add kube dependencies behind feature flag

**Files:**
- Modify: `crates/roche-core/Cargo.toml`

- [ ] **Step 1: Add kube and k8s-openapi as optional dependencies**

In `crates/roche-core/Cargo.toml`, add to `[dependencies]`:
```toml
kube = { version = "3.0", features = ["client", "runtime", "ws"], optional = true }
k8s-openapi = { version = "0.27", features = ["v1_32"], optional = true }
tar = { version = "0.4", optional = true }
toml = { version = "0.8", optional = true }
tracing = { version = "0.1", optional = true }
```

Add to `[features]`:
```toml
k8s = ["dep:kube", "dep:k8s-openapi", "dep:tar", "dep:toml", "dep:tracing"]
```

- [ ] **Step 2: Register the k8s module in provider/mod.rs**

In `crates/roche-core/src/provider/mod.rs`, add after the e2b line:
```rust
#[cfg(feature = "k8s")]
pub mod k8s;
```

- [ ] **Step 3: Create empty k8s.rs to verify compilation**

Create `crates/roche-core/src/provider/k8s.rs` with just:
```rust
// Kubernetes provider — Pod-based sandboxes via kube crate.
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p roche-core --features k8s`
Expected: Compiles successfully (the module is empty but valid)

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/Cargo.toml crates/roche-core/src/provider/mod.rs crates/roche-core/src/provider/k8s.rs Cargo.lock
git commit -m "feat: add kube dependencies and k8s provider module scaffold"
```

---

## Chunk 2: K8sProvider struct, config, and Pod/NetworkPolicy builders

### Task 2: Config resolution and K8sProvider struct

**Files:**
- Create: `crates/roche-core/src/provider/k8s.rs`

- [ ] **Step 1: Write tests for config resolution**

Add to `k8s.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_namespace_default() {
        // Clear env to ensure default
        std::env::remove_var("ROCHE_K8S_NAMESPACE");
        let ns = resolve_namespace();
        assert_eq!(ns, DEFAULT_NAMESPACE);
    }

    #[test]
    fn test_resolve_namespace_from_env() {
        std::env::set_var("ROCHE_K8S_NAMESPACE", "custom-ns");
        let ns = resolve_namespace();
        assert_eq!(ns, "custom-ns");
        std::env::remove_var("ROCHE_K8S_NAMESPACE");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p roche-core --features k8s -- k8s::tests`
Expected: FAIL — `resolve_namespace` not defined

- [ ] **Step 3: Implement K8sProvider struct and config resolution**

Write the top of `k8s.rs`:
```rust
use crate::provider::{ProviderError, SandboxFileOps, SandboxLifecycle, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo, SandboxStatus};
use k8s_openapi::api::core::v1::{
    Container, EmptyDirVolumeSource, EnvVar, Pod, PodSpec, Volume, VolumeMount,
};
use k8s_openapi::api::networking::v1::{
    NetworkPolicy, NetworkPolicySpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use kube::Client;
use std::collections::BTreeMap;

const DEFAULT_NAMESPACE: &str = "roche-sandboxes";
const EXIT_SENTINEL: &str = "ROCHE_EXIT:";

/// Kubernetes sandbox provider.
///
/// Each sandbox is a Pod in a dedicated namespace. Uses `kube` crate for
/// all K8s API interactions. NetworkPolicy enforces network isolation.
///
/// Namespace resolution: `ROCHE_K8S_NAMESPACE` env var → `~/.roche/k8s.toml` → `roche-sandboxes`.
pub struct K8sProvider {
    client: Client,
    namespace: String,
}

fn resolve_namespace() -> String {
    if let Ok(ns) = std::env::var("ROCHE_K8S_NAMESPACE") {
        if !ns.is_empty() {
            return ns;
        }
    }

    // Try config file
    if let Some(config_dir) = dirs::home_dir() {
        let config_path = config_dir.join(".roche").join("k8s.toml");
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            #[derive(serde::Deserialize)]
            struct K8sConfig {
                namespace: Option<String>,
            }
            if let Ok(config) = toml::from_str::<K8sConfig>(&content) {
                if let Some(ns) = config.namespace {
                    if !ns.is_empty() {
                        return ns;
                    }
                }
            }
        }
    }

    DEFAULT_NAMESPACE.to_string()
}

impl K8sProvider {
    /// Create a new K8s provider. Connects to the cluster via kubeconfig or in-cluster SA.
    pub async fn new() -> Result<Self, ProviderError> {
        let client = Client::try_default().await.map_err(|e| {
            ProviderError::Unavailable(format!("K8s cluster not reachable: {e}"))
        })?;
        let namespace = resolve_namespace();

        // Ensure namespace exists
        let ns_api: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(client.clone());
        let ns = k8s_openapi::api::core::v1::Namespace {
            metadata: kube::core::ObjectMeta {
                name: Some(namespace.clone()),
                ..Default::default()
            },
            ..Default::default()
        };
        match ns_api.create(&PostParams::default(), &ns).await {
            Ok(_) => {}
            Err(kube::Error::Api(ae)) if ae.code == 409 => {} // AlreadyExists
            Err(kube::Error::Api(ae)) if ae.code == 403 => {} // Forbidden — namespace may already exist
            Err(e) => {
                return Err(ProviderError::Unavailable(format!(
                    "Failed to ensure namespace '{namespace}': {e}"
                )));
            }
        }

        tracing::warn!(
            "K8s provider: NetworkPolicy enforcement requires a CNI plugin (Calico/Cilium). \
             Verify your cluster supports NetworkPolicy before relying on network=false isolation."
        );

        Ok(Self { client, namespace })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p roche-core --features k8s -- k8s::tests`
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/provider/k8s.rs
git commit -m "feat(k8s): add K8sProvider struct and config resolution"
```

### Task 3: Pod spec builder

**Files:**
- Modify: `crates/roche-core/src/provider/k8s.rs`

- [ ] **Step 1: Write tests for Pod spec builder**

Add to the `tests` module:
```rust
    #[test]
    fn test_build_pod_default_config() {
        let config = SandboxConfig::default();
        let pod_name = "roche-test-1234";
        let pod = build_pod(pod_name, "test-ns", &config);

        let meta = pod.metadata;
        assert_eq!(meta.name.as_deref(), Some(pod_name));
        assert_eq!(meta.namespace.as_deref(), Some("test-ns"));

        let labels = meta.labels.unwrap();
        assert_eq!(labels.get("roche.managed"), Some(&"true".to_string()));
        assert_eq!(labels.get("roche.sandbox"), Some(&pod_name.to_string()));

        let spec = pod.spec.unwrap();
        assert_eq!(spec.restart_policy.as_deref(), Some("Never"));

        let container = &spec.containers[0];
        assert_eq!(container.name, "sandbox");
        assert_eq!(container.command.as_ref().unwrap(), &vec!["sleep".to_string(), "infinity".to_string()]);

        let sc = container.security_context.as_ref().unwrap();
        assert_eq!(sc.read_only_root_filesystem, Some(true)); // writable=false default
        assert_eq!(sc.allow_privilege_escalation, Some(false));
        assert_eq!(sc.run_as_non_root, Some(true));
        assert_eq!(sc.run_as_user, Some(1000));

        // Should have /tmp volume mount
        let mounts = container.volume_mounts.as_ref().unwrap();
        assert_eq!(mounts[0].mount_path, "/tmp");
    }

    #[test]
    fn test_build_pod_with_resources_and_env() {
        let config = SandboxConfig {
            memory: Some("512m".to_string()),
            cpus: Some(1.5),
            env: [("FOO".to_string(), "bar".to_string())].into(),
            writable: true,
            ..Default::default()
        };
        let pod = build_pod("roche-test", "ns", &config);
        let container = &pod.spec.unwrap().containers[0];

        // Check resources
        let limits = container.resources.as_ref().unwrap().limits.as_ref().unwrap();
        assert!(limits.contains_key("memory"));
        assert!(limits.contains_key("cpu"));

        // Check env
        let env = container.env.as_ref().unwrap();
        assert!(env.iter().any(|e| e.name == "FOO" && e.value.as_deref() == Some("bar")));

        // writable=true: no readOnlyRootFilesystem
        let sc = container.security_context.as_ref().unwrap();
        assert_eq!(sc.read_only_root_filesystem, Some(false));

        // writable=true: no volume mounts
        assert!(container.volume_mounts.is_none() || container.volume_mounts.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_build_pod_with_timeout() {
        let config = SandboxConfig {
            timeout_secs: 600,
            ..Default::default()
        };
        let pod = build_pod("roche-test", "ns", &config);
        let spec = pod.spec.unwrap();
        assert_eq!(spec.active_deadline_seconds, Some(600));

        let annotations = pod.metadata.annotations.unwrap();
        assert!(annotations.contains_key("roche.expires"));
    }

    #[test]
    fn test_build_pod_no_timeout() {
        let config = SandboxConfig {
            timeout_secs: 0,
            ..Default::default()
        };
        let pod = build_pod("roche-test", "ns", &config);
        let spec = pod.spec.unwrap();
        assert!(spec.active_deadline_seconds.is_none());
        assert!(pod.metadata.annotations.is_none() ||
            !pod.metadata.annotations.as_ref().unwrap().contains_key("roche.expires"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p roche-core --features k8s -- k8s::tests`
Expected: FAIL — `build_pod` not defined

- [ ] **Step 3: Implement build_pod function**

Add to `k8s.rs` (before the `impl K8sProvider` block):
```rust
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

fn build_pod(name: &str, namespace: &str, config: &SandboxConfig) -> Pod {
    let mut labels = BTreeMap::new();
    labels.insert("roche.managed".to_string(), "true".to_string());
    labels.insert("roche.image".to_string(), config.image.clone());
    labels.insert("roche.sandbox".to_string(), name.to_string());

    let mut annotations = BTreeMap::new();
    if config.timeout_secs > 0 {
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + config.timeout_secs;
        annotations.insert("roche.expires".to_string(), expires_at.to_string());
    }

    let mut resource_limits = BTreeMap::new();
    if let Some(ref mem) = config.memory {
        resource_limits.insert("memory".to_string(), Quantity(mem.clone()));
    }
    if let Some(cpus) = config.cpus {
        let millicores = (cpus * 1000.0) as u32;
        resource_limits.insert("cpu".to_string(), Quantity(format!("{millicores}m")));
    }

    let env: Vec<EnvVar> = config
        .env
        .iter()
        .map(|(k, v)| EnvVar {
            name: k.clone(),
            value: Some(v.clone()),
            ..Default::default()
        })
        .collect();

    let security_context = k8s_openapi::api::core::v1::SecurityContext {
        read_only_root_filesystem: Some(!config.writable),
        allow_privilege_escalation: Some(false),
        run_as_non_root: Some(true),
        run_as_user: Some(1000),
        ..Default::default()
    };

    let mut volume_mounts = Vec::new();
    let mut volumes = Vec::new();
    if !config.writable {
        volume_mounts.push(VolumeMount {
            name: "tmp".to_string(),
            mount_path: "/tmp".to_string(),
            ..Default::default()
        });
        volumes.push(Volume {
            name: "tmp".to_string(),
            empty_dir: Some(EmptyDirVolumeSource {
                size_limit: Some(Quantity("64Mi".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        });
    }

    let container = Container {
        name: "sandbox".to_string(),
        image: Some(config.image.clone()),
        command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        env: if env.is_empty() { None } else { Some(env) },
        resources: if resource_limits.is_empty() {
            None
        } else {
            Some(k8s_openapi::api::core::v1::ResourceRequirements {
                limits: Some(resource_limits),
                ..Default::default()
            })
        },
        security_context: Some(security_context),
        volume_mounts: if volume_mounts.is_empty() {
            None
        } else {
            Some(volume_mounts)
        },
        ..Default::default()
    };

    Pod {
        metadata: kube::core::ObjectMeta {
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
            restart_policy: Some("Never".to_string()),
            active_deadline_seconds: if config.timeout_secs > 0 {
                Some(config.timeout_secs as i64)
            } else {
                None
            },
            containers: vec![container],
            volumes: if volumes.is_empty() {
                None
            } else {
                Some(volumes)
            },
            ..Default::default()
        }),
        ..Default::default()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p roche-core --features k8s -- k8s::tests`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/provider/k8s.rs
git commit -m "feat(k8s): add Pod spec builder with security defaults"
```

### Task 4: NetworkPolicy builder and status mapping

**Files:**
- Modify: `crates/roche-core/src/provider/k8s.rs`

- [ ] **Step 1: Write tests for NetworkPolicy builder and status mapping**

Add to tests module:
```rust
    #[test]
    fn test_build_network_policy() {
        let np = build_deny_all_network_policy("roche-test-pod", "test-ns");
        let meta = np.metadata;
        assert_eq!(meta.name.as_deref(), Some("roche-deny-roche-test-pod"));
        assert_eq!(meta.namespace.as_deref(), Some("test-ns"));

        let spec = np.spec.unwrap();
        let selector = spec.pod_selector;
        let labels = selector.match_labels.unwrap();
        assert_eq!(labels.get("roche.sandbox"), Some(&"roche-test-pod".to_string()));

        let policy_types = spec.policy_types.unwrap();
        assert!(policy_types.contains(&"Ingress".to_string()));
        assert!(policy_types.contains(&"Egress".to_string()));
        assert!(spec.ingress.is_none());
        assert!(spec.egress.is_none());
    }

    #[test]
    fn test_pod_phase_to_status() {
        assert_eq!(pod_phase_to_status(Some("Running")), SandboxStatus::Running);
        assert_eq!(pod_phase_to_status(Some("Pending")), SandboxStatus::Running);
        assert_eq!(pod_phase_to_status(Some("Succeeded")), SandboxStatus::Stopped);
        assert_eq!(pod_phase_to_status(Some("Failed")), SandboxStatus::Failed);
        assert_eq!(pod_phase_to_status(None), SandboxStatus::Stopped);
    }

    #[test]
    fn test_parse_exit_code() {
        let stdout = "hello world\nROCHE_EXIT:0\n";
        let (code, clean) = parse_exit_sentinel(stdout);
        assert_eq!(code, 0);
        assert_eq!(clean, "hello world\n");
    }

    #[test]
    fn test_parse_exit_code_nonzero() {
        let stdout = "error output\nROCHE_EXIT:127\n";
        let (code, clean) = parse_exit_sentinel(stdout);
        assert_eq!(code, 127);
        assert_eq!(clean, "error output\n");
    }

    #[test]
    fn test_parse_exit_code_missing() {
        let stdout = "no sentinel here\n";
        let (code, clean) = parse_exit_sentinel(stdout);
        assert_eq!(code, -1);
        assert_eq!(clean, "no sentinel here\n");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p roche-core --features k8s -- k8s::tests`
Expected: FAIL — functions not defined

- [ ] **Step 3: Implement NetworkPolicy builder, status mapping, and exit sentinel parser**

Add to `k8s.rs`:
```rust
fn build_deny_all_network_policy(pod_name: &str, namespace: &str) -> NetworkPolicy {
    let mut match_labels = BTreeMap::new();
    match_labels.insert("roche.sandbox".to_string(), pod_name.to_string());

    NetworkPolicy {
        metadata: kube::core::ObjectMeta {
            name: Some(format!("roche-deny-{pod_name}")),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: Some(NetworkPolicySpec {
            pod_selector: LabelSelector {
                match_labels: Some(match_labels),
                ..Default::default()
            },
            policy_types: Some(vec!["Ingress".to_string(), "Egress".to_string()]),
            ingress: None,
            egress: None,
        }),
    }
}

fn pod_phase_to_status(phase: Option<&str>) -> SandboxStatus {
    match phase {
        Some("Running") | Some("Pending") => SandboxStatus::Running,
        Some("Succeeded") => SandboxStatus::Stopped,
        Some("Failed") => SandboxStatus::Failed,
        _ => SandboxStatus::Stopped,
    }
}

/// Parse the ROCHE_EXIT:{code} sentinel from stdout.
/// Returns (exit_code, cleaned_stdout).
fn parse_exit_sentinel(stdout: &str) -> (i32, String) {
    if let Some(pos) = stdout.rfind(EXIT_SENTINEL) {
        let after = &stdout[pos + EXIT_SENTINEL.len()..];
        let code_str = after.trim();
        let code = code_str.parse::<i32>().unwrap_or(-1);
        let clean = stdout[..pos].to_string();
        (code, clean)
    } else {
        (-1, stdout.to_string())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p roche-core --features k8s -- k8s::tests`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/provider/k8s.rs
git commit -m "feat(k8s): add NetworkPolicy builder, status mapping, exit sentinel parser"
```

---

## Chunk 3: SandboxProvider trait implementation

### Task 5: Implement SandboxProvider::create and SandboxProvider::destroy

**Files:**
- Modify: `crates/roche-core/src/provider/k8s.rs`

- [ ] **Step 1: Implement create**

Add to `impl K8sProvider`:
```rust
    fn pods_api(&self) -> Api<Pod> {
        Api::namespaced(self.client.clone(), &self.namespace)
    }

    fn netpol_api(&self) -> Api<NetworkPolicy> {
        Api::namespaced(self.client.clone(), &self.namespace)
    }
```

Then implement `SandboxProvider`:
```rust
impl SandboxProvider for K8sProvider {
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        // Reject unsupported features
        if !config.mounts.is_empty() {
            return Err(ProviderError::Unsupported(
                "K8s provider does not support host mounts".to_string(),
            ));
        }

        let pod_name = format!("roche-{}", uuid::Uuid::new_v4());
        let pod = build_pod(&pod_name, &self.namespace, config);

        // Create the Pod
        self.pods_api()
            .create(&PostParams::default(), &pod)
            .await
            .map_err(|e| ProviderError::CreateFailed(format!("Failed to create Pod: {e}")))?;

        // Create NetworkPolicy if network=false
        if !config.network {
            let np = build_deny_all_network_policy(&pod_name, &self.namespace);
            self.netpol_api()
                .create(&PostParams::default(), &np)
                .await
                .map_err(|e| {
                    ProviderError::CreateFailed(format!("Failed to create NetworkPolicy: {e}"))
                })?;
        }

        // Wait for Pod to be Running (up to 60s)
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(60);
        loop {
            if tokio::time::Instant::now() > deadline {
                // Clean up the pod we created
                let _ = self.pods_api().delete(&pod_name, &DeleteParams::default()).await;
                return Err(ProviderError::Timeout(60));
            }

            if let Ok(p) = self.pods_api().get(&pod_name).await {
                if let Some(status) = &p.status {
                    if let Some(phase) = &status.phase {
                        if phase == "Running" {
                            return Ok(pod_name);
                        }
                        if phase == "Failed" || phase == "Succeeded" {
                            let _ = self.pods_api().delete(&pod_name, &DeleteParams::default()).await;
                            return Err(ProviderError::CreateFailed(format!(
                                "Pod entered {phase} phase before becoming Ready"
                            )));
                        }
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError> {
        // Delete Pod
        match self.pods_api().delete(id, &DeleteParams::default()).await {
            Ok(_) => {}
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                return Err(ProviderError::NotFound(id.clone()));
            }
            Err(e) => {
                return Err(ProviderError::ExecFailed(format!(
                    "Failed to delete Pod: {e}"
                )));
            }
        }

        // Delete NetworkPolicy (ignore NotFound)
        let np_name = format!("roche-deny-{id}");
        match self.netpol_api().delete(&np_name, &DeleteParams::default()).await {
            Ok(_) => {}
            Err(kube::Error::Api(ae)) if ae.code == 404 => {}
            Err(_) => {} // Best effort
        }

        Ok(())
    }

    // exec and list will be added in next tasks
    async fn exec(
        &self,
        _id: &SandboxId,
        _request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        todo!("exec implemented in Task 6")
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        todo!("list implemented in Task 7")
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p roche-core --features k8s`
Expected: Compiles (exec/list are `todo!()` stubs)

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/src/provider/k8s.rs
git commit -m "feat(k8s): implement SandboxProvider::create and destroy"
```

### Task 6: Implement SandboxProvider::exec

**Files:**
- Modify: `crates/roche-core/src/provider/k8s.rs`

- [ ] **Step 1: Implement exec**

Replace the `todo!()` exec with:
```rust
    async fn exec(
        &self,
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        use kube::api::AttachParams;
        use tokio::io::AsyncReadExt;

        // Build the sh -c wrapper command to capture exit code
        let cmd_str = request
            .command
            .iter()
            .map(|c| shell_escape(c))
            .collect::<Vec<_>>()
            .join(" ");
        let wrapped = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("{cmd_str}; echo \"{EXIT_SENTINEL}$?\""),
        ];

        let attach_params = AttachParams::default()
            .container("sandbox")
            .stdout(true)
            .stderr(true);

        let exec_fut = async {
            let mut attached = self
                .pods_api()
                .exec(id, &wrapped, &attach_params)
                .await
                .map_err(|e| match &e {
                    kube::Error::Api(ae) if ae.code == 404 => {
                        ProviderError::NotFound(id.clone())
                    }
                    _ => ProviderError::ExecFailed(format!("K8s exec failed: {e}")),
                })?;

            let stdout_reader = async {
                let mut buf = Vec::new();
                if let Some(mut s) = attached.stdout() {
                    s.read_to_end(&mut buf).await.map_err(|e| {
                        ProviderError::ExecFailed(format!("Failed to read stdout: {e}"))
                    })?;
                }
                Ok::<Vec<u8>, ProviderError>(buf)
            };

            let stderr_reader = async {
                let mut buf = Vec::new();
                if let Some(mut s) = attached.stderr() {
                    s.read_to_end(&mut buf).await.map_err(|e| {
                        ProviderError::ExecFailed(format!("Failed to read stderr: {e}"))
                    })?;
                }
                Ok::<Vec<u8>, ProviderError>(buf)
            };

            let (stdout_buf, stderr_buf) = tokio::try_join!(stdout_reader, stderr_reader)?;

            let stdout_raw = String::from_utf8_lossy(&stdout_buf).to_string();
            let stderr = String::from_utf8_lossy(&stderr_buf).to_string();

            let (exit_code, stdout) = parse_exit_sentinel(&stdout_raw);

            Ok::<ExecOutput, ProviderError>(ExecOutput {
                exit_code,
                stdout,
                stderr,
            })
        };

        if let Some(timeout_secs) = request.timeout_secs {
            tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                exec_fut,
            )
            .await
            .map_err(|_| ProviderError::Timeout(timeout_secs))?
        } else {
            exec_fut.await
        }
    }
```

Also add the shell_escape helper:
```rust
/// Minimal shell escaping for use in sh -c wrappers.
fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/' || c == '.') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
```

- [ ] **Step 2: Add shell_escape test**

Add to tests module:
```rust
    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "hello");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("/usr/bin/python3"), "/usr/bin/python3");
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p roche-core --features k8s -- k8s::tests`
Expected: All tests pass

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p roche-core --features k8s`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/provider/k8s.rs
git commit -m "feat(k8s): implement SandboxProvider::exec with exit code sentinel"
```

### Task 7: Implement SandboxProvider::list

**Files:**
- Modify: `crates/roche-core/src/provider/k8s.rs`

- [ ] **Step 1: Implement list**

Replace the `todo!()` list with:
```rust
    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        let lp = ListParams::default().labels("roche.managed=true");
        let pods = self
            .pods_api()
            .list(&lp)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("Failed to list Pods: {e}")))?;

        let infos = pods
            .items
            .into_iter()
            .filter_map(|pod| {
                let name = pod.metadata.name?;
                let labels = pod.metadata.labels.unwrap_or_default();
                let annotations = pod.metadata.annotations.unwrap_or_default();
                let image = labels
                    .get("roche.image")
                    .cloned()
                    .unwrap_or_default();
                let expires_at = annotations
                    .get("roche.expires")
                    .and_then(|s| s.parse::<u64>().ok());
                let phase = pod
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.as_deref())
                    .map(String::from);
                let status = pod_phase_to_status(phase.as_deref());

                Some(SandboxInfo {
                    id: name,
                    status,
                    provider: "k8s".to_string(),
                    image,
                    expires_at,
                })
            })
            .collect();

        Ok(infos)
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p roche-core --features k8s`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/src/provider/k8s.rs
git commit -m "feat(k8s): implement SandboxProvider::list"
```

---

## Chunk 4: SandboxLifecycle, SandboxFileOps, and wiring

### Task 8: Implement SandboxLifecycle and SandboxFileOps

**Files:**
- Modify: `crates/roche-core/src/provider/k8s.rs`

- [ ] **Step 1: Implement SandboxLifecycle**

Add to `k8s.rs`:
```rust
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
                    if expires_at <= now {
                        if self.destroy(&name).await.is_ok() {
                            destroyed.push(name);
                        }
                    }
                }
            }
        }

        Ok(destroyed)
    }
}
```

- [ ] **Step 2: Implement SandboxFileOps**

Add to `k8s.rs`:
```rust
impl SandboxFileOps for K8sProvider {
    async fn copy_to(
        &self,
        id: &SandboxId,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), ProviderError> {
        use kube::api::AttachParams;
        use tokio::io::AsyncWriteExt;

        // Read local file into a tar archive in memory
        let file_data = tokio::fs::read(src).await.map_err(|e| {
            ProviderError::FileFailed(format!("Failed to read local file: {e}"))
        })?;

        let file_name = std::path::Path::new(dest)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ProviderError::FileFailed("Invalid destination path".to_string()))?;

        let parent_dir = std::path::Path::new(dest)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("/");

        // Create tar archive
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let mut header = tar::Header::new_gnu();
            header.set_size(file_data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, file_name, file_data.as_slice())
                .map_err(|e| ProviderError::FileFailed(format!("Failed to create tar: {e}")))?;
            builder
                .finish()
                .map_err(|e| ProviderError::FileFailed(format!("Failed to finalize tar: {e}")))?;
        }

        let attach_params = AttachParams::default()
            .container("sandbox")
            .stdin(true)
            .stdout(true)
            .stderr(true);

        let cmd = vec![
            "tar".to_string(),
            "-xf".to_string(),
            "-".to_string(),
            "-C".to_string(),
            parent_dir.to_string(),
        ];

        let mut attached = self
            .pods_api()
            .exec(id, &cmd, &attach_params)
            .await
            .map_err(|e| ProviderError::FileFailed(format!("K8s exec for copy failed: {e}")))?;

        if let Some(mut stdin) = attached.stdin() {
            stdin.write_all(&tar_buf).await.map_err(|e| {
                ProviderError::FileFailed(format!("Failed to write tar to stdin: {e}"))
            })?;
            stdin.shutdown().await.map_err(|e| {
                ProviderError::FileFailed(format!("Failed to close stdin: {e}"))
            })?;
        }

        attached.join().await.map_err(|e| {
            ProviderError::FileFailed(format!("copy_to exec failed: {e}"))
        })?;

        Ok(())
    }

    async fn copy_from(
        &self,
        id: &SandboxId,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), ProviderError> {
        use kube::api::AttachParams;
        use tokio::io::AsyncReadExt;

        let attach_params = AttachParams::default()
            .container("sandbox")
            .stdout(true)
            .stderr(true);

        let cmd = vec![
            "tar".to_string(),
            "-cf".to_string(),
            "-".to_string(),
            "-C".to_string(),
            std::path::Path::new(src)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("/")
                .to_string(),
            std::path::Path::new(src)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(src)
                .to_string(),
        ];

        let mut attached = self
            .pods_api()
            .exec(id, &cmd, &attach_params)
            .await
            .map_err(|e| ProviderError::FileFailed(format!("K8s exec for copy failed: {e}")))?;

        let mut tar_buf = Vec::new();
        if let Some(mut stdout) = attached.stdout() {
            stdout.read_to_end(&mut tar_buf).await.map_err(|e| {
                ProviderError::FileFailed(format!("Failed to read tar from stdout: {e}"))
            })?;
        }

        // Extract tar to destination
        let mut archive = tar::Archive::new(tar_buf.as_slice());
        let dest_parent = dest.parent().unwrap_or(std::path::Path::new("."));
        archive.unpack(dest_parent).map_err(|e| {
            ProviderError::FileFailed(format!("Failed to extract tar: {e}"))
        })?;

        Ok(())
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p roche-core --features k8s`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add crates/roche-core/src/provider/k8s.rs
git commit -m "feat(k8s): implement SandboxLifecycle and SandboxFileOps"
```

### Task 9: Wire K8s provider into CLI

**Files:**
- Modify: `crates/roche-cli/Cargo.toml`
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Enable k8s feature in CLI**

In `crates/roche-cli/Cargo.toml`, change:
```toml
roche-core = { version = "0.1.0", path = "../roche-core", features = ["wasmtime", "e2b", "k8s"] }
```

- [ ] **Step 2: Add k8s provider dispatch in CLI main.rs**

In `crates/roche-cli/src/main.rs`, add a new match arm before the `"e2b"` arm (around line 841):
```rust
        "k8s" => {
            use roche_core::provider::k8s::K8sProvider;
            let provider = K8sProvider::new().await?;
            // Handle Cp for K8s (supports file ops via tar)
            if let Commands::Cp { ref src, ref dest } = cli.command {
                use roche_core::provider::SandboxFileOps;
                match (parse_cp_path(src), parse_cp_path(dest)) {
                    (Some((sandbox_id, sandbox_path)), None) => {
                        provider
                            .copy_from(
                                &sandbox_id.to_string(),
                                sandbox_path,
                                std::path::Path::new(dest),
                            )
                            .await?;
                    }
                    (None, Some((sandbox_id, sandbox_path))) => {
                        provider
                            .copy_to(
                                &sandbox_id.to_string(),
                                std::path::Path::new(src),
                                sandbox_path,
                            )
                            .await?;
                    }
                    (Some(_), Some(_)) => {
                        eprintln!("Error: both source and destination cannot be sandbox paths");
                        std::process::exit(1);
                    }
                    (None, None) => {
                        eprintln!("Error: one of source or destination must be a sandbox path (sandbox_id:/path)");
                        std::process::exit(1);
                    }
                }
                Ok(())
            } else {
                run_provider_commands!(provider, cli.command)
            }
        }
```

Also update the Cp error message in `run_provider_commands!` macro to mention k8s:
```rust
Commands::Cp { .. } => {
    eprintln!("Error: file copy is only supported with the docker, e2b, and k8s providers");
    std::process::exit(1);
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p roche-cli`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add crates/roche-cli/Cargo.toml crates/roche-cli/src/main.rs
git commit -m "feat(k8s): wire K8s provider into CLI with Cp support"
```

### Task 10: Wire K8s provider into daemon

**Files:**
- Modify: `crates/roche-daemon/Cargo.toml`
- Modify: `crates/roche-daemon/src/server.rs`

- [ ] **Step 1: Enable k8s feature in daemon**

In `crates/roche-daemon/Cargo.toml`, change:
```toml
roche-core = { version = "0.1.0", path = "../roche-core", features = ["wasmtime", "e2b", "k8s"] }
```

- [ ] **Step 2: Add K8sProvider to daemon server**

In `crates/roche-daemon/src/server.rs`:

Add import:
```rust
use roche_core::provider::k8s::K8sProvider;
```

Add field to `SandboxServiceImpl`:
```rust
pub struct SandboxServiceImpl {
    docker: DockerProvider,
    e2b: Option<E2bProvider>,
    k8s: Option<K8sProvider>,
    #[cfg(target_os = "linux")]
    firecracker: Option<FirecrackerProvider>,
    wasm: Option<WasmProvider>,
    pool_manager: Arc<PoolManager>,
}
```

Update `new()` — note K8sProvider::new() is async, so wrap in a blocking-compatible way:
```rust
pub async fn new(pool_manager: Arc<PoolManager>) -> Self {
    Self {
        docker: DockerProvider::new(),
        e2b: E2bProvider::new().ok(),
        k8s: K8sProvider::new().await.ok(),
        #[cfg(target_os = "linux")]
        firecracker: FirecrackerProvider::new().ok(),
        wasm: WasmProvider::new().ok(),
        pool_manager,
    }
}
```

Note: If `new()` is currently sync, it needs to become `async` since `K8sProvider::new()` is async. Check if callers need updating.

Add `"k8s"` arm to `with_provider!` macro:
```rust
"k8s" => {
    if let Some(ref $p) = $self.k8s {
        $body
    } else {
        Err(Status::unavailable(
            "K8s provider not available (check kubeconfig or in-cluster configuration)",
        ))
    }
}
```

- [ ] **Step 3: Update caller of SandboxServiceImpl::new() to async**

In `crates/roche-daemon/src/main.rs:62`, change:
```rust
let service = server::SandboxServiceImpl::new(pool_manager.clone());
```
to:
```rust
let service = server::SandboxServiceImpl::new(pool_manager.clone()).await;
```

- [ ] **Step 4: Note on daemon copy_to/copy_from**

The current daemon `copy_to`/`copy_from` gRPC handlers in `server.rs` are hardcoded to `self.docker`. These do NOT dispatch via `with_provider!` because the `CopyToRequest`/`CopyFromRequest` proto messages lack a `provider` field. For now, K8s file operations will work through the CLI's direct provider path. Daemon-dispatched file ops for non-Docker providers is a separate enhancement (requires proto changes).

- [ ] **Step 5: Verify compilation**

Run: `cargo build -p roche-daemon`
Expected: Compiles

- [ ] **Step 6: Commit**

```bash
git add crates/roche-daemon/Cargo.toml crates/roche-daemon/src/server.rs crates/roche-daemon/src/main.rs Cargo.lock
git commit -m "feat(k8s): wire K8s provider into daemon gRPC server"
```

### Task 11: Full build and test verification

**Files:** None (verification only)

- [ ] **Step 1: Run full build**

Run: `cargo build`
Expected: All crates compile with zero errors

- [ ] **Step 2: Run all tests**

Run: `cargo test --lib`
Expected: All tests pass (including new K8s unit tests)

- [ ] **Step 3: Run clippy**

Run: `cargo clippy`
Expected: Zero warnings

- [ ] **Step 4: Fix any issues found in steps 1-3**

Address any compilation errors, test failures, or clippy warnings.

- [ ] **Step 5: Final commit if fixes were needed**

```bash
git add -A
git commit -m "fix(k8s): address build/test/clippy issues"
```
