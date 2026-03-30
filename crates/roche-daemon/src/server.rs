// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::idempotency::IdempotencyCache;
use crate::pool::PoolManager;
use crate::proto;
use roche_core::provider::docker::DockerProvider;
use roche_core::provider::e2b::E2bProvider;
#[cfg(target_os = "linux")]
use roche_core::provider::firecracker::FirecrackerProvider;
use roche_core::provider::k8s::K8sProvider;
use roche_core::provider::wasm::WasmProvider;
use roche_core::provider::{ProviderError, SandboxFileOps, SandboxLifecycle, SandboxProvider};
use roche_core::sensor::{DockerSensor, SensorDispatch, TraceLevel};
use roche_core::types::{self, SandboxStatus};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

pub struct SandboxServiceImpl {
    docker: DockerProvider,
    e2b: Option<E2bProvider>,
    k8s: Option<K8sProvider>,
    #[cfg(target_os = "linux")]
    firecracker: Option<FirecrackerProvider>,
    wasm: Option<WasmProvider>,
    pool_manager: Arc<PoolManager>,
    pub last_rpc_ms: Arc<AtomicU64>,
    docker_sensor: SensorDispatch,
    none_sensor: SensorDispatch,
    idempotency_cache: IdempotencyCache,
}

impl SandboxServiceImpl {
    pub async fn new(pool_manager: Arc<PoolManager>) -> Self {
        Self {
            docker: DockerProvider::new(),
            e2b: E2bProvider::new().ok(),
            k8s: K8sProvider::new().await.ok(),
            #[cfg(target_os = "linux")]
            firecracker: FirecrackerProvider::new().ok(),
            wasm: WasmProvider::new().ok(),
            pool_manager,
            last_rpc_ms: Arc::new(AtomicU64::new(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            )),
            docker_sensor: SensorDispatch::Docker(DockerSensor),
            none_sensor: SensorDispatch::None,
            idempotency_cache: IdempotencyCache::new(),
        }
    }

    fn touch_last_rpc(&self) {
        self.last_rpc_ms.store(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            Ordering::Relaxed,
        );
    }

    fn get_sensor(&self, provider: &str) -> &SensorDispatch {
        match provider {
            "docker" => &self.docker_sensor,
            _ => &self.none_sensor,
        }
    }
}

fn provider_error_to_status(err: ProviderError) -> Status {
    match &err {
        ProviderError::NotFound(_) => Status::not_found(err.to_string()),
        ProviderError::CreateFailed(_) => Status::internal(err.to_string()),
        ProviderError::ExecFailed(_) => Status::internal(err.to_string()),
        ProviderError::Unavailable(_) => Status::unavailable(err.to_string()),
        ProviderError::Timeout(_) => Status::deadline_exceeded(err.to_string()),
        ProviderError::Unsupported(_) => Status::unimplemented(err.to_string()),
        ProviderError::FileFailed(_) => Status::internal(err.to_string()),
        ProviderError::Paused(_) => Status::failed_precondition(err.to_string()),
    }
}

fn sandbox_status_to_proto(status: SandboxStatus) -> i32 {
    match status {
        SandboxStatus::Running => proto::SandboxStatus::Running as i32,
        SandboxStatus::Paused => proto::SandboxStatus::Paused as i32,
        SandboxStatus::Stopped => proto::SandboxStatus::Stopped as i32,
        SandboxStatus::Failed => proto::SandboxStatus::Failed as i32,
    }
}

fn trace_to_proto(trace: roche_core::sensor::ExecutionTrace) -> proto::ExecutionTrace {
    use roche_core::sensor::FileOp;

    proto::ExecutionTrace {
        duration_secs: trace.duration_secs,
        resource_usage: Some(proto::ResourceUsage {
            peak_memory_bytes: trace.resource_usage.peak_memory_bytes,
            cpu_time_secs: trace.resource_usage.cpu_time_secs,
            network_rx_bytes: trace.resource_usage.network_rx_bytes,
            network_tx_bytes: trace.resource_usage.network_tx_bytes,
        }),
        file_accesses: trace
            .file_accesses
            .into_iter()
            .map(|fa| proto::FileAccess {
                path: fa.path,
                op: match fa.op {
                    FileOp::Read => proto::FileOp::Read as i32,
                    FileOp::Write => proto::FileOp::Write as i32,
                    FileOp::Create => proto::FileOp::Create as i32,
                    FileOp::Delete => proto::FileOp::Delete as i32,
                },
                size_bytes: fa.size_bytes,
            })
            .collect(),
        network_attempts: trace
            .network_attempts
            .into_iter()
            .map(|na| proto::NetworkAttempt {
                address: na.address,
                protocol: na.protocol,
                allowed: na.allowed,
            })
            .collect(),
        blocked_ops: trace
            .blocked_ops
            .into_iter()
            .map(|bo| proto::BlockedOperation {
                op_type: bo.op_type,
                detail: bo.detail,
            })
            .collect(),
        syscalls: trace
            .syscalls
            .into_iter()
            .map(|sc| proto::SyscallEvent {
                name: sc.name,
                args: sc.args,
                result: sc.result,
                timestamp_ms: sc.timestamp_ms,
            })
            .collect(),
        resource_timeline: trace
            .resource_timeline
            .into_iter()
            .map(|rs| proto::ResourceSnapshot {
                timestamp_ms: rs.timestamp_ms,
                memory_bytes: rs.memory_bytes,
                cpu_percent: rs.cpu_percent,
            })
            .collect(),
    }
}

fn default_timeout(t: u64) -> u64 {
    if t == 0 {
        300
    } else {
        t
    }
}

fn default_provider(p: &str) -> &str {
    if p.is_empty() {
        "docker"
    } else {
        p
    }
}

/// Macro to dispatch to the correct provider based on the provider name string.
macro_rules! with_provider {
    ($self:expr, $provider_name:expr, |$p:ident| $body:expr) => {{
        match $provider_name {
            "e2b" => {
                if let Some(ref $p) = $self.e2b {
                    $body
                } else {
                    Err(Status::unavailable(
                        "E2B provider not available (set E2B_API_KEY or configure ~/.roche/e2b.toml)",
                    ))
                }
            }
            "k8s" => {
                if let Some(ref $p) = $self.k8s {
                    $body
                } else {
                    Err(Status::unavailable(
                        "K8s provider not available (check kubeconfig or in-cluster configuration)",
                    ))
                }
            }
            #[cfg(target_os = "linux")]
            "firecracker" => {
                if let Some(ref $p) = $self.firecracker {
                    $body
                } else {
                    Err(Status::unavailable("Firecracker provider not available"))
                }
            }
            "wasm" => {
                if let Some(ref $p) = $self.wasm {
                    $body
                } else {
                    Err(Status::unavailable("WASM provider not available"))
                }
            }
            _ => {
                let $p = &$self.docker;
                $body
            }
        }
    }};
}

#[tonic::async_trait]
impl proto::sandbox_service_server::SandboxService for SandboxServiceImpl {
    async fn create(
        &self,
        request: Request<proto::CreateRequest>,
    ) -> Result<Response<proto::CreateResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        let config = types::SandboxConfig {
            provider: req.provider.clone(),
            image: if req.image.is_empty() {
                "python:3.12-slim".to_string()
            } else {
                req.image
            },
            memory: req.memory,
            cpus: req.cpus,
            timeout_secs: default_timeout(req.timeout_secs),
            network: req.network,
            writable: req.writable,
            env: req.env,
            mounts: req
                .mounts
                .into_iter()
                .map(|m| types::MountConfig {
                    host_path: m.host_path,
                    container_path: m.container_path,
                    readonly: m.readonly,
                })
                .collect(),
            kernel: req.kernel,
            rootfs: req.rootfs,
            trace_enabled: true,
            network_allowlist: req.network_allowlist,
            fs_paths: req.fs_paths,
        };

        // Try pool first
        if let Some(id) = self.pool_manager.try_acquire(&config).await {
            return Ok(Response::new(proto::CreateResponse { sandbox_id: id }));
        }

        // Pool miss or bypass — direct create
        let provider_name = default_provider(&config.provider);
        with_provider!(self, provider_name, |p| {
            let id = p.create(&config).await.map_err(provider_error_to_status)?;
            Ok(Response::new(proto::CreateResponse { sandbox_id: id }))
        })
    }

    async fn exec(
        &self,
        request: Request<proto::ExecRequest>,
    ) -> Result<Response<proto::ExecResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();

        // Check idempotency cache
        let idempotency_key = req.idempotency_key.clone();
        if let Some(ref key) = idempotency_key {
            if let Some(cached) = self.idempotency_cache.get(key) {
                return Ok(Response::new(cached));
            }
        }

        let exec_req = types::ExecRequest {
            command: req.command,
            timeout_secs: req.timeout_secs,
            idempotency_key: idempotency_key.clone(),
        };
        let provider_name = default_provider(&req.provider);
        let trace_level = TraceLevel::from_proto(req.trace_level);

        // Start trace collector if tracing is enabled
        let sensor = self.get_sensor(provider_name);
        let collector = if trace_level != TraceLevel::Off {
            sensor.start_trace(&req.sandbox_id, trace_level).await
        } else {
            None
        };

        with_provider!(self, provider_name, |p| {
            let result = p.exec(&req.sandbox_id, &exec_req).await;

            match result {
                Ok(output) => {
                    // Finish trace collection
                    let trace = if let Some(c) = collector {
                        c.finish().await.ok().map(trace_to_proto)
                    } else {
                        None
                    };

                    let response = proto::ExecResponse {
                        exit_code: output.exit_code,
                        stdout: output.stdout,
                        stderr: output.stderr,
                        trace,
                    };

                    // Cache response for idempotent requests
                    if let Some(key) = idempotency_key {
                        self.idempotency_cache.put(key, response.clone());
                    }

                    Ok(Response::new(response))
                }
                Err(e) => {
                    // Abort trace collection on error
                    if let Some(c) = collector {
                        c.abort().await;
                    }
                    Err(provider_error_to_status(e))
                }
            }
        })
    }

    async fn destroy(
        &self,
        request: Request<proto::DestroyRequest>,
    ) -> Result<Response<proto::DestroyResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            let targets = if req.all {
                p.list()
                    .await
                    .map_err(provider_error_to_status)?
                    .into_iter()
                    .map(|sb| sb.id)
                    .collect()
            } else {
                req.sandbox_ids
            };
            let mut destroyed = Vec::new();
            for id in &targets {
                if p.destroy(id).await.is_ok() {
                    self.pool_manager.on_destroy(id).await;
                    destroyed.push(id.clone());
                }
            }
            Ok(Response::new(proto::DestroyResponse {
                destroyed_ids: destroyed,
            }))
        })
    }

    async fn list(
        &self,
        request: Request<proto::ListRequest>,
    ) -> Result<Response<proto::ListResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            let sandboxes = p.list().await.map_err(provider_error_to_status)?;
            let infos = sandboxes
                .into_iter()
                .map(|sb| proto::SandboxInfo {
                    id: sb.id,
                    status: sandbox_status_to_proto(sb.status),
                    provider: sb.provider,
                    image: sb.image,
                    expires_at: sb.expires_at,
                })
                .collect();
            Ok(Response::new(proto::ListResponse { sandboxes: infos }))
        })
    }

    async fn pause(
        &self,
        request: Request<proto::PauseRequest>,
    ) -> Result<Response<proto::PauseResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            p.pause(&req.sandbox_id)
                .await
                .map_err(provider_error_to_status)?;
            Ok(Response::new(proto::PauseResponse {}))
        })
    }

    async fn unpause(
        &self,
        request: Request<proto::UnpauseRequest>,
    ) -> Result<Response<proto::UnpauseResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            p.unpause(&req.sandbox_id)
                .await
                .map_err(provider_error_to_status)?;
            Ok(Response::new(proto::UnpauseResponse {}))
        })
    }

    async fn gc(
        &self,
        request: Request<proto::GcRequest>,
    ) -> Result<Response<proto::GcResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            if req.all {
                let sandboxes = p.list().await.map_err(provider_error_to_status)?;
                let mut destroyed = Vec::new();
                for sb in &sandboxes {
                    if req.dry_run || p.destroy(&sb.id).await.is_ok() {
                        destroyed.push(sb.id.clone());
                    }
                }
                Ok(Response::new(proto::GcResponse {
                    destroyed_ids: destroyed,
                }))
            } else if req.dry_run {
                let sandboxes = p.list().await.map_err(provider_error_to_status)?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let expired: Vec<String> = sandboxes
                    .into_iter()
                    .filter(|sb| sb.expires_at.is_some_and(|exp| exp <= now))
                    .map(|sb| sb.id)
                    .collect();
                Ok(Response::new(proto::GcResponse {
                    destroyed_ids: expired,
                }))
            } else {
                let destroyed = p.gc().await.map_err(provider_error_to_status)?;
                Ok(Response::new(proto::GcResponse {
                    destroyed_ids: destroyed,
                }))
            }
        })
    }

    async fn copy_to(
        &self,
        request: Request<proto::CopyToRequest>,
    ) -> Result<Response<proto::CopyToResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        self.docker
            .copy_to(
                &req.sandbox_id,
                std::path::Path::new(&req.host_path),
                &req.sandbox_path,
            )
            .await
            .map_err(provider_error_to_status)?;
        Ok(Response::new(proto::CopyToResponse {}))
    }

    async fn copy_from(
        &self,
        request: Request<proto::CopyFromRequest>,
    ) -> Result<Response<proto::CopyFromResponse>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();
        self.docker
            .copy_from(
                &req.sandbox_id,
                &req.sandbox_path,
                std::path::Path::new(&req.host_path),
            )
            .await
            .map_err(provider_error_to_status)?;
        Ok(Response::new(proto::CopyFromResponse {}))
    }

    async fn pool_status(
        &self,
        _request: Request<proto::PoolStatusRequest>,
    ) -> Result<Response<proto::PoolStatusResponse>, Status> {
        self.touch_last_rpc();
        let statuses = self.pool_manager.status().await;
        let pools = statuses
            .into_iter()
            .map(|s| proto::PoolInfo {
                provider: s.provider,
                image: s.image,
                idle_count: s.idle_count,
                active_count: s.active_count,
                max_idle: s.max_idle,
                max_total: s.max_total,
            })
            .collect();
        Ok(Response::new(proto::PoolStatusResponse { pools }))
    }

    async fn pool_warmup(
        &self,
        _request: Request<proto::PoolWarmupRequest>,
    ) -> Result<Response<proto::PoolWarmupResponse>, Status> {
        self.touch_last_rpc();
        self.pool_manager.warmup().await;
        Ok(Response::new(proto::PoolWarmupResponse {}))
    }

    async fn pool_drain(
        &self,
        _request: Request<proto::PoolDrainRequest>,
    ) -> Result<Response<proto::PoolDrainResponse>, Status> {
        self.touch_last_rpc();
        let destroyed = self.pool_manager.drain().await;
        Ok(Response::new(proto::PoolDrainResponse {
            destroyed_count: destroyed,
        }))
    }

    type ExecStreamStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<proto::ExecEvent, Status>> + Send>>;

    async fn exec_stream(
        &self,
        request: Request<proto::ExecStreamRequest>,
    ) -> Result<Response<Self::ExecStreamStream>, Status> {
        self.touch_last_rpc();
        let req = request.into_inner();

        let (tx, rx) = mpsc::channel::<Result<proto::ExecEvent, Status>>(32);

        let exec_req = types::ExecRequest {
            command: req.command.clone(),
            timeout_secs: req.timeout_secs,
            idempotency_key: req.idempotency_key.clone(),
        };
        let provider_name = default_provider(&req.provider).to_string();
        let sandbox_id = req.sandbox_id.clone();
        let trace_level = TraceLevel::from_proto(req.trace_level);
        let timeout_secs = req.timeout_secs.unwrap_or(300);

        // Clone what we need for the spawned task
        let docker = DockerProvider::new();
        let sensor = SensorDispatch::Docker(DockerSensor);

        tokio::spawn(async move {
            let start = Instant::now();

            // Start heartbeat task
            let heartbeat_tx = tx.clone();
            let heartbeat_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    let event = proto::ExecEvent {
                        event: Some(proto::exec_event::Event::Heartbeat(proto::Heartbeat {
                            elapsed_ms,
                            resources: Some(proto::ResourceSnapshot {
                                timestamp_ms: elapsed_ms,
                                memory_bytes: 0,
                                cpu_percent: 0.0,
                            }),
                        })),
                    };
                    if heartbeat_tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                }
            });

            // Start trace collector
            let collector = if trace_level != TraceLevel::Off {
                sensor.start_trace(&sandbox_id, trace_level).await
            } else {
                None
            };

            // Execute command — for now, run the full exec and stream the result
            // Future: use docker exec with streaming I/O
            let result = docker.exec(&sandbox_id, &exec_req).await;

            // Stop heartbeat
            heartbeat_handle.abort();

            match result {
                Ok(output) => {
                    // Send stdout chunk
                    if !output.stdout.is_empty() {
                        let _ = tx
                            .send(Ok(proto::ExecEvent {
                                event: Some(proto::exec_event::Event::Output(proto::OutputChunk {
                                    stream: "stdout".into(),
                                    data: output.stdout.into_bytes(),
                                })),
                            }))
                            .await;
                    }
                    // Send stderr chunk
                    if !output.stderr.is_empty() {
                        let _ = tx
                            .send(Ok(proto::ExecEvent {
                                event: Some(proto::exec_event::Event::Output(proto::OutputChunk {
                                    stream: "stderr".into(),
                                    data: output.stderr.into_bytes(),
                                })),
                            }))
                            .await;
                    }
                    // Finish trace
                    let trace = if let Some(c) = collector {
                        c.finish().await.ok().map(trace_to_proto)
                    } else {
                        None
                    };
                    // Send final result
                    let _ = tx
                        .send(Ok(proto::ExecEvent {
                            event: Some(proto::exec_event::Event::Result(proto::ExecResult {
                                exit_code: output.exit_code,
                                trace,
                            })),
                        }))
                        .await;
                }
                Err(e) => {
                    if let Some(c) = collector {
                        c.abort().await;
                    }
                    let _ = tx.send(Err(provider_error_to_status(e))).await;
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::ExecStreamStream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_error_to_status_mapping() {
        let cases: Vec<(ProviderError, tonic::Code)> = vec![
            (ProviderError::NotFound("x".into()), tonic::Code::NotFound),
            (
                ProviderError::CreateFailed("x".into()),
                tonic::Code::Internal,
            ),
            (ProviderError::ExecFailed("x".into()), tonic::Code::Internal),
            (
                ProviderError::Unavailable("x".into()),
                tonic::Code::Unavailable,
            ),
            (ProviderError::Timeout(30), tonic::Code::DeadlineExceeded),
            (
                ProviderError::Unsupported("x".into()),
                tonic::Code::Unimplemented,
            ),
            (ProviderError::FileFailed("x".into()), tonic::Code::Internal),
            (
                ProviderError::Paused("x".into()),
                tonic::Code::FailedPrecondition,
            ),
        ];

        for (err, expected_code) in cases {
            let status = provider_error_to_status(err);
            assert_eq!(status.code(), expected_code);
        }
    }

    #[test]
    fn test_sandbox_status_to_proto() {
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Running),
            proto::SandboxStatus::Running as i32
        );
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Paused),
            proto::SandboxStatus::Paused as i32
        );
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Stopped),
            proto::SandboxStatus::Stopped as i32
        );
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Failed),
            proto::SandboxStatus::Failed as i32
        );
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(0), 300);
        assert_eq!(default_timeout(60), 60);
    }

    #[test]
    fn test_default_provider() {
        assert_eq!(default_provider(""), "docker");
        assert_eq!(default_provider("firecracker"), "firecracker");
        assert_eq!(default_provider("docker"), "docker");
    }
}
