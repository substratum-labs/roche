# Kubernetes Provider Design

## Overview

Add a Kubernetes provider to Roche that creates sandbox Pods in a dedicated namespace, using the `kube` crate for all K8s API interactions. Each sandbox maps to a single Pod with AI-safe security defaults enforced via K8s-native primitives (SecurityContext, NetworkPolicy, ResourceQuota).

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| K8s client | `kube` crate (pure Rust API client) | Async-native, strongly typed, no external CLI dependency. Consistent with E2B provider style. |
| Namespace | Default `roche-sandboxes`, configurable | Dedicated namespace enables namespace-level NetworkPolicy and safe GC. Avoids polluting `default`. |
| Mounts | `ProviderError::Unsupported` | `hostPath` blocked by most PSA policies. Consistent with E2B/WASM. Users should use `SandboxFileOps` instead. |
| NetworkPolicy CNI | Create policy + document requirement | No reliable runtime detection of CNI support. Same trust model as Docker `--network none`. Warn in logs. |
| Pause/Unpause | `ProviderError::Unsupported` | K8s has no native Pod pause. |

## Architecture

```
K8sProvider
  ├── client: kube::Client          // from kubeconfig or in-cluster SA
  ├── namespace: String             // default "roche-sandboxes"
  └── feature flag: k8s = ["dep:kube", "dep:k8s-openapi"]
```

Each sandbox = 1 Pod (+ optional NetworkPolicy when `network=false`).

### Sandbox Identity

- Pod name: `roche-{uuid}` (e.g., `roche-a1b2c3d4-e5f6-...`)
- `SandboxId` = Pod name
- Labels: `roche.managed=true`, `roche.image={image}`, `roche.sandbox={pod-name}`
- Annotations: `roche.expires={unix_timestamp}` (only when timeout_secs > 0)

## Configuration

Priority: environment variable > config file > defaults.

| Source | Key | Example |
|---|---|---|
| Env var | `ROCHE_K8S_NAMESPACE` | `my-sandboxes` |
| Config file | `~/.roche/k8s.toml` | `namespace = "roche-sandboxes"` |
| Default | — | `roche-sandboxes` |

```toml
# ~/.roche/k8s.toml
namespace = "roche-sandboxes"
# kubeconfig path is optional; defaults to kube::Config::infer()
# kubeconfig = "~/.kube/config"
```

### Initialization (`new()`)

1. `kube::Config::infer().await` — auto-detects in-cluster SA or kubeconfig
2. Read namespace from env/config/default
3. Ensure namespace exists (create if not found, ignore AlreadyExists)
4. Log `tracing::warn!` reminding user to verify CNI NetworkPolicy support
5. Return `K8sProvider` or error if cluster unreachable

## Trait Implementations

### SandboxProvider

#### `create(config) -> SandboxId`

1. Validate config: reject `mounts` (return `Unsupported`)
2. Build Pod spec:
   ```yaml
   apiVersion: v1
   kind: Pod
   metadata:
     name: roche-{uuid}
     namespace: {namespace}
     labels:
       roche.managed: "true"
       roche.image: {image}
       roche.sandbox: {pod-name}
     annotations:
       roche.expires: "{now + timeout_secs}"  # only if timeout_secs > 0
   spec:
     activeDeadlineSeconds: {timeout_secs}     # only if timeout_secs > 0
     restartPolicy: Never
     containers:
       - name: sandbox
         image: {image}
         command: ["sleep", "infinity"]
         env: [{key: val, ...}]
         resources:
           limits:
             memory: {memory}    # if specified, e.g. "512Mi"
             cpu: {millicores}m  # if specified, cpus * 1000, e.g. "1500m" for 1.5 cores
         securityContext:
           readOnlyRootFilesystem: {!writable}
           allowPrivilegeEscalation: false
           runAsNonRoot: true
           runAsUser: 1000
         volumeMounts:           # only if !writable
           - name: tmp
             mountPath: /tmp
     volumes:                    # only if !writable
       - name: tmp
         emptyDir:
           sizeLimit: 64Mi
   ```
3. If `network=false`: create deny-all NetworkPolicy targeting this Pod
   ```yaml
   apiVersion: networking.k8s.io/v1
   kind: NetworkPolicy
   metadata:
     name: roche-deny-{pod-name}
     namespace: {namespace}
   spec:
     podSelector:
       matchLabels:
         roche.sandbox: {pod-name}
     policyTypes: [Ingress, Egress]
     # no ingress/egress rules = deny all
   ```
   Note: One NetworkPolicy per Pod is simpler for cleanup. The `podSelector` uses `roche.sandbox={pod-name}` to target only this specific Pod, allowing `network=true` and `network=false` sandboxes to coexist.
4. Wait for Pod phase=Running (poll with timeout, e.g. 60s)
5. Return Pod name as `SandboxId`

#### `exec(id, request) -> ExecOutput`

1. Use `kube::api::AttachedProcess` to exec command in Pod container `sandbox`
2. Wrap the user command in `sh -c '{cmd}; echo "ROCHE_EXIT:$?"'` to capture exit code, since K8s exec WebSocket does not directly return exit codes
3. Wrap with `tokio::time::timeout()` if `request.timeout_secs` is set
4. Collect stdout/stderr from the attached process streams
5. Parse exit code from the `ROCHE_EXIT:{code}` sentinel in stdout tail, strip it from output
6. Return `ExecOutput { exit_code, stdout, stderr }`
7. If Pod not found → `ProviderError::NotFound`

#### `destroy(id)`

1. Delete Pod; if 404 → return `ProviderError::NotFound`
2. Delete NetworkPolicy `roche-deny-{id}` (ignore NotFound — may not exist if `network=true`)

#### `list() -> Vec<SandboxInfo>`

1. List Pods with label selector `roche.managed=true` in namespace
2. Map Pod phase to SandboxStatus:
   - `Running` → `SandboxStatus::Running`
   - `Succeeded` / `Failed` → `SandboxStatus::Stopped` / `SandboxStatus::Failed`
   - `Pending` → `SandboxStatus::Running` (approximation — Pod may be scheduling or pulling image)
3. Extract `roche.expires` annotation → `expires_at`
4. Return `SandboxInfo { id: pod.name, status, provider: "k8s", image, expires_at }`

### SandboxLifecycle

- **pause**: `ProviderError::Unsupported("K8s does not support Pod pause")`
- **unpause**: `ProviderError::Unsupported("K8s does not support Pod unpause")`
- **gc**: List Pods with `roche.managed=true`, compare `roche.expires` annotation with current time, delete expired Pods and their NetworkPolicies

### SandboxFileOps

- **copy_to(id, src, dest)**: Read local file, exec `tar -xf - -C {parent_dir}` in Pod, pipe tar stream via stdin. Note: when `writable=false`, only `/tmp` is writable; writes to other paths will fail with `ProviderError::FileFailed`.
- **copy_from(id, sandbox_path, dest)**: Exec `tar -cf - {sandbox_path}` in Pod, read stdout stream, extract to local dest

## Security Mapping Summary

| Roche Setting | K8s Primitive | Notes |
|---|---|---|
| `network=false` | deny-all NetworkPolicy per Pod | Requires CNI plugin (Calico/Cilium/etc.) |
| `network=true` | No NetworkPolicy created | Default K8s allows all |
| `writable=false` | `readOnlyRootFilesystem: true` + emptyDir `/tmp` | 64Mi tmpfs for scratch space |
| `writable=true` | No readOnlyRootFilesystem | Default overlay FS |
| `memory` | `resources.limits.memory` | OOMKilled if exceeded |
| `cpus` | `resources.limits.cpu` | Throttled if exceeded |
| `timeout_secs` | `activeDeadlineSeconds` + `roche.expires` annotation | Pod terminated by kubelet |
| `mounts` | `ProviderError::Unsupported` | Blocked by PSA in most clusters |
| `no-new-privileges` | `allowPrivilegeEscalation: false` | Direct mapping |
| `runAsNonRoot` | `runAsNonRoot: true`, `runAsUser: 1000` | Defense in depth |
| `pids-limit` | No K8s-native equivalent | Recommend namespace-level `LimitRange` for PID limits; depends on `SupportPodPidsLimit` feature gate |

**CPU conversion**: `SandboxConfig.cpus` is `f64` representing whole cores (e.g., `1.5`). Convert to K8s millicores as `format!("{}m", (cpus * 1000.0) as u32)` → `"1500m"`.

## RBAC Requirements

The K8s provider requires the following permissions on the target namespace:

| Resource | Verbs |
|---|---|
| `pods` | `get`, `list`, `create`, `delete` |
| `pods/exec` | `create` |
| `networkpolicies` | `get`, `create`, `delete` |
| `namespaces` | `get`, `create` (only if `create_namespace` is enabled) |

Example Role:
```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: roche-sandbox-manager
  namespace: roche-sandboxes
rules:
  - apiGroups: [""]
    resources: ["pods", "pods/exec"]
    verbs: ["get", "list", "create", "delete"]
  - apiGroups: ["networking.k8s.io"]
    resources: ["networkpolicies"]
    verbs: ["get", "create", "delete"]
```

## Dependencies

```toml
# Cargo.toml (roche-core)
[features]
k8s = ["dep:kube", "dep:k8s-openapi"]

[dependencies]
kube = { version = "3.0", features = ["client", "runtime", "ws"], optional = true }
k8s-openapi = { version = "0.27", features = ["v1_32"], optional = true }
```

## Wiring

### provider/mod.rs
```rust
#[cfg(feature = "k8s")]
pub mod k8s;
```

### CLI (main.rs)
```rust
"k8s" => {
    use roche_core::provider::k8s::K8sProvider;
    let provider = K8sProvider::new().await?;
    // K8s supports file ops
    if let Commands::Cp { .. } = cli.command {
        // handle copy_to/copy_from
    }
    run_provider_commands!(provider, cli.command)
}
```

### Daemon (server.rs)
```rust
pub struct SandboxServiceImpl {
    // ...existing fields...
    k8s: Option<K8sProvider>,
}
// Add "k8s" arm to with_provider! macro
```

### CLI/Daemon Cargo.toml
```toml
roche-core = { ..., features = ["wasmtime", "e2b", "k8s"] }
```

## Error Mapping

| K8s API Error | ProviderError |
|---|---|
| Pod not found (404) | `NotFound` |
| Forbidden / Unauthorized | `Unavailable` (RBAC issue) |
| Pod creation failed | `CreateFailed` |
| Exec failed | `ExecFailed` |
| Timeout waiting for Running | `Timeout` |
| Mounts requested | `Unsupported` |
| Pause/Unpause | `Unsupported` |

## Testing

Unit tests (no cluster required):
- Config resolution (env > file > default)
- Pod spec builder: verify security context, resource limits, labels, annotations
- NetworkPolicy spec builder: verify deny-all structure
- Status mapping: Pod phase → SandboxStatus
- Error mapping: K8s API errors → ProviderError

Integration tests (requires cluster):
- Create/exec/destroy lifecycle
- Network isolation verification
- Resource limits enforcement
- GC of expired Pods
- File copy operations
