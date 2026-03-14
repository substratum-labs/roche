use crate::proto;
use roche_core::provider::docker::DockerProvider;
#[cfg(target_os = "linux")]
use roche_core::provider::firecracker::FirecrackerProvider;
use roche_core::provider::{ProviderError, SandboxFileOps, SandboxLifecycle, SandboxProvider};
use roche_core::types::{self, SandboxStatus};
use tonic::{Request, Response, Status};

pub struct SandboxServiceImpl {
    docker: DockerProvider,
    #[cfg(target_os = "linux")]
    firecracker: Option<FirecrackerProvider>,
}

impl SandboxServiceImpl {
    pub fn new() -> Self {
        Self {
            docker: DockerProvider::new(),
            #[cfg(target_os = "linux")]
            firecracker: FirecrackerProvider::new().ok(),
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
            #[cfg(target_os = "linux")]
            "firecracker" => {
                if let Some(ref $p) = $self.firecracker {
                    $body
                } else {
                    Err(Status::unavailable("Firecracker provider not available"))
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
        };

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
        let req = request.into_inner();
        let exec_req = types::ExecRequest {
            command: req.command,
            timeout_secs: req.timeout_secs,
        };
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            let output = p
                .exec(&req.sandbox_id, &exec_req)
                .await
                .map_err(provider_error_to_status)?;
            Ok(Response::new(proto::ExecResponse {
                exit_code: output.exit_code,
                stdout: output.stdout,
                stderr: output.stderr,
            }))
        })
    }

    async fn destroy(
        &self,
        request: Request<proto::DestroyRequest>,
    ) -> Result<Response<proto::DestroyResponse>, Status> {
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
        let req = request.into_inner();
        let provider_name = default_provider(&req.provider);

        with_provider!(self, provider_name, |p| {
            if req.all {
                let sandboxes = p.list().await.map_err(provider_error_to_status)?;
                let mut destroyed = Vec::new();
                for sb in &sandboxes {
                    if req.dry_run {
                        destroyed.push(sb.id.clone());
                    } else if p.destroy(&sb.id).await.is_ok() {
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
            (
                ProviderError::ExecFailed("x".into()),
                tonic::Code::Internal,
            ),
            (
                ProviderError::Unavailable("x".into()),
                tonic::Code::Unavailable,
            ),
            (ProviderError::Timeout(30), tonic::Code::DeadlineExceeded),
            (
                ProviderError::Unsupported("x".into()),
                tonic::Code::Unimplemented,
            ),
            (
                ProviderError::FileFailed("x".into()),
                tonic::Code::Internal,
            ),
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
