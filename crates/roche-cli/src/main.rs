use clap::{Parser, Subcommand};

pub mod proto {
    tonic::include_proto!("roche.v1");
}

#[derive(Parser)]
#[command(name = "roche", about = "Universal sandbox orchestrator for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Force direct provider access (skip daemon even if running)
    #[arg(long, global = true)]
    direct: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new sandbox
    Create {
        /// Provider to use
        #[arg(long, default_value = "docker")]
        provider: String,

        /// Container image
        #[arg(long, default_value = "python:3.12-slim")]
        image: String,

        /// Memory limit (e.g. "512m")
        #[arg(long)]
        memory: Option<String>,

        /// CPU limit (e.g. "1.0")
        #[arg(long)]
        cpus: Option<f64>,

        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,

        /// Enable network access (default: disabled for safety)
        #[arg(long)]
        network: bool,

        /// Enable writable filesystem (default: readonly for safety)
        #[arg(long)]
        writable: bool,

        /// Environment variables (KEY=VALUE, repeatable)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,

        /// Volume mounts (host:container[:ro|rw], repeatable)
        #[arg(long = "mount", value_name = "HOST:CONTAINER[:ro|rw]")]
        mounts: Vec<String>,

        /// Number of sandboxes to create
        #[arg(long, default_value = "1")]
        count: u32,

        /// Path to kernel image (required for firecracker provider)
        #[arg(long)]
        kernel: Option<String>,

        /// Path to rootfs image (required for firecracker provider)
        #[arg(long)]
        rootfs: Option<String>,
    },

    /// Execute a command in a sandbox
    Exec {
        /// Sandbox ID
        #[arg(long)]
        sandbox: String,

        /// Timeout override in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Command to execute
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Destroy sandboxes
    Destroy {
        /// Sandbox IDs (one or more)
        #[arg(required_unless_present = "all")]
        ids: Vec<String>,

        /// Destroy ALL roche-managed sandboxes
        #[arg(long)]
        all: bool,
    },

    /// Pause a sandbox (freeze all processes)
    Pause {
        /// Sandbox ID
        id: String,
    },

    /// Unpause a sandbox
    Unpause {
        /// Sandbox ID
        id: String,
    },

    /// List active sandboxes
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Garbage collect expired sandboxes
    Gc {
        /// Only list expired sandboxes, don't destroy
        #[arg(long)]
        dry_run: bool,

        /// Destroy ALL roche-managed sandboxes (ignore expiry)
        #[arg(long)]
        all: bool,
    },

    /// Copy files between host and sandbox (Docker only)
    Cp {
        /// Source path (local path or sandbox_id:/path)
        src: String,
        /// Destination path (local path or sandbox_id:/path)
        dest: String,
    },

    /// Manage the roche daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Manage sandbox pool (requires daemon)
    Pool {
        #[command(subcommand)]
        action: PoolAction,
    },
}

#[derive(Subcommand, Clone)]
enum DaemonAction {
    /// Start the daemon
    Start {
        /// Port to listen on
        #[arg(long, default_value = "50051")]
        port: u16,

        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
}

#[derive(Subcommand, Clone)]
enum PoolAction {
    /// Show pool status (idle/active counts)
    Status,
    /// Trigger immediate warmup for all pools
    Warmup,
    /// Destroy all idle sandboxes in pools
    Drain,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = run(cli).await;
    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn parse_env_vars(pairs: &[String]) -> Result<std::collections::HashMap<String, String>, String> {
    pairs
        .iter()
        .map(|s| {
            let (k, v) = s
                .split_once('=')
                .ok_or_else(|| format!("invalid env format: {s} (expected KEY=VALUE)"))?;
            Ok((k.to_string(), v.to_string()))
        })
        .collect()
}

fn parse_mount(s: &str) -> Result<roche_core::types::MountConfig, String> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    match parts.len() {
        2 => Ok(roche_core::types::MountConfig {
            host_path: parts[0].to_string(),
            container_path: parts[1].to_string(),
            readonly: true,
        }),
        3 => {
            let readonly = match parts[2] {
                "ro" => true,
                "rw" => false,
                other => return Err(format!("invalid mount mode: {other} (expected ro or rw)")),
            };
            Ok(roche_core::types::MountConfig {
                host_path: parts[0].to_string(),
                container_path: parts[1].to_string(),
                readonly,
            })
        }
        _ => Err(format!(
            "invalid mount format: {s} (expected host:container[:ro|rw])"
        )),
    }
}

fn parse_cp_path(s: &str) -> Option<(&str, &str)> {
    s.split_once(':')
}

// --- Daemon helpers ---

fn daemon_json_path() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("cannot find home directory")
        .join(".roche")
        .join("daemon.json")
}

#[derive(serde::Deserialize)]
struct DaemonInfo {
    pid: u32,
    port: u16,
}

fn read_daemon_info() -> Option<DaemonInfo> {
    let path = daemon_json_path();
    let json = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}

fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

async fn handle_daemon(action: DaemonAction) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::ProviderError;

    match action {
        DaemonAction::Start { port, foreground } => {
            if let Some(info) = read_daemon_info() {
                if is_process_alive(info.pid) {
                    eprintln!(
                        "Daemon already running (pid={}, port={})",
                        info.pid, info.port
                    );
                    std::process::exit(1);
                }
            }

            if foreground {
                let status = tokio::process::Command::new("roched")
                    .arg("--port")
                    .arg(port.to_string())
                    .status()
                    .await
                    .map_err(|e| {
                        ProviderError::ExecFailed(format!("failed to start daemon: {e}"))
                    })?;
                std::process::exit(status.code().unwrap_or(1));
            } else {
                let roche_dir = dirs::home_dir()
                    .expect("cannot find home directory")
                    .join(".roche");
                std::fs::create_dir_all(&roche_dir)
                    .map_err(|e| ProviderError::ExecFailed(e.to_string()))?;
                let log_file = std::fs::File::create(roche_dir.join("daemon.log"))
                    .map_err(|e| ProviderError::ExecFailed(e.to_string()))?;
                let err_file = log_file
                    .try_clone()
                    .map_err(|e| ProviderError::ExecFailed(e.to_string()))?;

                let child = std::process::Command::new("roched")
                    .arg("--port")
                    .arg(port.to_string())
                    .stdout(log_file)
                    .stderr(err_file)
                    .spawn()
                    .map_err(|e| {
                        ProviderError::ExecFailed(format!("failed to start daemon: {e}"))
                    })?;

                println!("Daemon started (pid={}, port={})", child.id(), port);
            }
        }
        DaemonAction::Stop => {
            let info = read_daemon_info()
                .ok_or_else(|| ProviderError::ExecFailed("No daemon running".to_string()))?;
            if !is_process_alive(info.pid) {
                let _ = std::fs::remove_file(daemon_json_path());
                eprintln!("No daemon running (stale pid file cleaned up)");
                std::process::exit(1);
            }
            unsafe {
                libc::kill(info.pid as i32, libc::SIGTERM);
            }
            println!("Daemon stopped (pid={})", info.pid);
        }
        DaemonAction::Status => match read_daemon_info() {
            Some(info) if is_process_alive(info.pid) => {
                println!("Daemon running (pid={}, port={})", info.pid, info.port);
            }
            Some(info) => {
                let _ = std::fs::remove_file(daemon_json_path());
                println!("Daemon not running (stale pid={}, cleaned up)", info.pid);
            }
            None => {
                println!("Daemon not running");
            }
        },
    }
    Ok(())
}

// --- gRPC client dispatch ---

async fn try_daemon_dispatch(cli: &Cli) -> Option<Result<(), roche_core::provider::ProviderError>> {
    if cli.direct {
        return None;
    }

    let info = read_daemon_info()?;
    if !is_process_alive(info.pid) {
        return None;
    }

    let addr = format!("http://127.0.0.1:{}", info.port);
    let mut client = proto::sandbox_service_client::SandboxServiceClient::connect(addr)
        .await
        .ok()?;

    Some(run_via_grpc(&mut client, &cli.command).await)
}

async fn run_via_grpc(
    client: &mut proto::sandbox_service_client::SandboxServiceClient<tonic::transport::Channel>,
    command: &Commands,
) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::ProviderError;

    match command {
        Commands::Create {
            provider,
            image,
            memory,
            cpus,
            timeout,
            network,
            writable,
            env,
            mounts,
            count,
            kernel,
            rootfs,
        } => {
            let env_map = parse_env_vars(env).map_err(ProviderError::ExecFailed)?;
            let mount_configs: Vec<proto::MountConfig> = mounts
                .iter()
                .map(|s| {
                    let m = parse_mount(s).unwrap_or_else(|e| {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    });
                    proto::MountConfig {
                        host_path: m.host_path,
                        container_path: m.container_path,
                        readonly: m.readonly,
                    }
                })
                .collect();

            for _ in 0..*count {
                let resp = client
                    .create(proto::CreateRequest {
                        provider: provider.clone(),
                        image: image.clone(),
                        memory: memory.clone(),
                        cpus: *cpus,
                        timeout_secs: *timeout,
                        network: *network,
                        writable: *writable,
                        env: env_map.clone(),
                        mounts: mount_configs.clone(),
                        kernel: kernel.clone(),
                        rootfs: rootfs.clone(),
                    })
                    .await
                    .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                println!("{}", resp.into_inner().sandbox_id);
            }
        }
        Commands::Exec {
            sandbox,
            timeout,
            command,
        } => {
            let resp = client
                .exec(proto::ExecRequest {
                    sandbox_id: sandbox.clone(),
                    command: command.clone(),
                    timeout_secs: *timeout,
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
            let output = resp.into_inner();
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            if output.exit_code != 0 {
                std::process::exit(output.exit_code);
            }
        }
        Commands::Destroy { ids, all } => {
            client
                .destroy(proto::DestroyRequest {
                    sandbox_ids: ids.clone(),
                    all: *all,
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
        }
        Commands::List { json } => {
            let resp = client
                .list(proto::ListRequest {
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
            let sandboxes = resp.into_inner().sandboxes;
            if *json {
                let json_val: Vec<serde_json::Value> = sandboxes
                    .iter()
                    .map(|sb| {
                        serde_json::json!({
                            "id": sb.id,
                            "status": match sb.status {
                                1 => "running",
                                2 => "paused",
                                3 => "stopped",
                                4 => "failed",
                                _ => "unknown",
                            },
                            "provider": sb.provider,
                            "image": sb.image,
                            "expires_at": sb.expires_at,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json_val).unwrap());
            } else if sandboxes.is_empty() {
                println!("No active sandboxes.");
            } else {
                println!(
                    "{:<16} {:<10} {:<10} {:<10} IMAGE",
                    "ID", "STATUS", "PROVIDER", "EXPIRES"
                );
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                for sb in &sandboxes {
                    let status_str = match sb.status {
                        1 => "running",
                        2 => "paused",
                        3 => "stopped",
                        4 => "failed",
                        _ => "unknown",
                    };
                    let expires_str = match sb.expires_at {
                        Some(exp) if exp > now => {
                            let remaining = exp - now;
                            let mins = remaining / 60;
                            let secs = remaining % 60;
                            format!("{mins}m{secs:02}s")
                        }
                        Some(_) => "expired".to_string(),
                        None => "-".to_string(),
                    };
                    println!(
                        "{:<16} {:<10} {:<10} {:<10} {}",
                        sb.id, status_str, sb.provider, expires_str, sb.image,
                    );
                }
            }
        }
        Commands::Pause { id } => {
            client
                .pause(proto::PauseRequest {
                    sandbox_id: id.clone(),
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
        }
        Commands::Unpause { id } => {
            client
                .unpause(proto::UnpauseRequest {
                    sandbox_id: id.clone(),
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
        }
        Commands::Gc { dry_run, all } => {
            let resp = client
                .gc(proto::GcRequest {
                    dry_run: *dry_run,
                    all: *all,
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
            let destroyed = resp.into_inner().destroyed_ids;
            for id in &destroyed {
                if *dry_run {
                    println!("{id}");
                } else {
                    println!("destroyed: {id}");
                }
            }
            if destroyed.is_empty() && !*dry_run {
                println!("No expired sandboxes found.");
            }
        }
        Commands::Cp { src, dest } => {
            match (parse_cp_path(src), parse_cp_path(dest)) {
                (Some((sandbox_id, sandbox_path)), None) => {
                    client
                        .copy_from(proto::CopyFromRequest {
                            sandbox_id: sandbox_id.to_string(),
                            sandbox_path: sandbox_path.to_string(),
                            host_path: dest.clone(),
                            provider: "docker".to_string(),
                        })
                        .await
                        .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                }
                (None, Some((sandbox_id, sandbox_path))) => {
                    client
                        .copy_to(proto::CopyToRequest {
                            sandbox_id: sandbox_id.to_string(),
                            host_path: src.clone(),
                            sandbox_path: sandbox_path.to_string(),
                            provider: "docker".to_string(),
                        })
                        .await
                        .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
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
        }
        Commands::Pool { action } => match action {
            PoolAction::Status => {
                let resp = client
                    .pool_status(proto::PoolStatusRequest {})
                    .await
                    .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                let pools = resp.into_inner().pools;
                if pools.is_empty() {
                    println!("No pools configured.");
                } else {
                    println!(
                        "{:<14} {:<30} {:<6} {:<8} {:<10} {:<10}",
                        "PROVIDER", "IMAGE", "IDLE", "ACTIVE", "MAX_IDLE", "MAX_TOTAL"
                    );
                    for p in &pools {
                        println!(
                            "{:<14} {:<30} {:<6} {:<8} {:<10} {:<10}",
                            p.provider,
                            p.image,
                            p.idle_count,
                            p.active_count,
                            p.max_idle,
                            p.max_total
                        );
                    }
                }
            }
            PoolAction::Warmup => {
                client
                    .pool_warmup(proto::PoolWarmupRequest {})
                    .await
                    .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                println!("Pool warmup triggered.");
            }
            PoolAction::Drain => {
                let resp = client
                    .pool_drain(proto::PoolDrainRequest {})
                    .await
                    .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                println!(
                    "Drained {} idle sandboxes.",
                    resp.into_inner().destroyed_count
                );
            }
        },
        Commands::Daemon { .. } => unreachable!("daemon handled earlier"),
    }
    Ok(())
}

/// Run shared commands that work with any provider implementing SandboxProvider + SandboxLifecycle.
macro_rules! run_provider_commands {
    ($provider:expr, $command:expr) => {{
        let provider = $provider;
        match $command {
            Commands::Create {
                provider: provider_name,
                image,
                memory,
                cpus,
                timeout,
                network,
                writable,
                env,
                mounts,
                count,
                kernel,
                rootfs,
            } => {
                let env_map = parse_env_vars(&env)
                    .map_err(|e| {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    })
                    .unwrap();
                let mount_configs: Vec<_> = mounts
                    .iter()
                    .map(|s| {
                        parse_mount(s).unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            std::process::exit(1);
                        })
                    })
                    .collect();
                let config = roche_core::types::SandboxConfig {
                    provider: provider_name,
                    image,
                    memory,
                    cpus,
                    timeout_secs: timeout,
                    network,
                    writable,
                    env: env_map,
                    mounts: mount_configs,
                    kernel,
                    rootfs,
                };
                for _ in 0..count {
                    match provider.create(&config).await {
                        Ok(id) => println!("{id}"),
                        Err(e) => eprintln!("Error: {e}"),
                    }
                }
            }
            Commands::Exec {
                sandbox,
                timeout,
                command,
            } => {
                let request = roche_core::types::ExecRequest {
                    command,
                    timeout_secs: timeout,
                };
                let output = provider.exec(&sandbox, &request).await?;
                print!("{}", output.stdout);
                eprint!("{}", output.stderr);
                if output.exit_code != 0 {
                    std::process::exit(output.exit_code);
                }
            }
            Commands::Destroy { ids, all } => {
                let targets = if all {
                    provider.list().await?.into_iter().map(|sb| sb.id).collect()
                } else {
                    ids
                };
                for id in &targets {
                    match provider.destroy(id).await {
                        Ok(()) => {}
                        Err(e) => eprintln!("Error destroying {id}: {e}"),
                    }
                }
            }
            Commands::Pause { id } => {
                provider.pause(&id).await?;
            }
            Commands::Unpause { id } => {
                provider.unpause(&id).await?;
            }
            Commands::List { json } => {
                let sandboxes = provider.list().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&sandboxes).unwrap());
                } else if sandboxes.is_empty() {
                    println!("No active sandboxes.");
                } else {
                    println!(
                        "{:<16} {:<10} {:<10} {:<10} IMAGE",
                        "ID", "STATUS", "PROVIDER", "EXPIRES"
                    );
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    for sb in &sandboxes {
                        let expires_str = match sb.expires_at {
                            Some(exp) if exp > now => {
                                let remaining = exp - now;
                                let mins = remaining / 60;
                                let secs = remaining % 60;
                                format!("{mins}m{secs:02}s")
                            }
                            Some(_) => "expired".to_string(),
                            None => "-".to_string(),
                        };
                        println!(
                            "{:<16} {:<10} {:<10} {:<10} {}",
                            sb.id,
                            format!("{:?}", sb.status).to_lowercase(),
                            sb.provider,
                            expires_str,
                            sb.image,
                        );
                    }
                }
            }
            Commands::Gc { dry_run, all } => {
                if all {
                    let sandboxes = provider.list().await?;
                    for sb in &sandboxes {
                        if dry_run {
                            println!("{}", sb.id);
                        } else {
                            provider.destroy(&sb.id).await?;
                            println!("destroyed: {}", sb.id);
                        }
                    }
                } else if dry_run {
                    let sandboxes = provider.list().await?;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    for sb in &sandboxes {
                        if let Some(exp) = sb.expires_at {
                            if exp <= now {
                                println!("{}", sb.id);
                            }
                        }
                    }
                } else {
                    let destroyed = provider.gc().await?;
                    for id in &destroyed {
                        println!("destroyed: {id}");
                    }
                    if destroyed.is_empty() {
                        println!("No expired sandboxes found.");
                    }
                }
            }
            Commands::Cp { .. } => {
                eprintln!("Error: file copy is only supported with the docker, e2b, and k8s providers");
                std::process::exit(1);
            }
            Commands::Pool { .. } => unreachable!("pool handled earlier"),
            Commands::Daemon { .. } => unreachable!("daemon handled earlier"),
        }
        Ok(())
    }};
}

async fn run(cli: Cli) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::docker::DockerProvider;
    use roche_core::provider::firecracker::FirecrackerProvider;
    use roche_core::provider::{SandboxLifecycle, SandboxProvider};

    // Handle daemon subcommand first
    if let Commands::Daemon { ref action } = cli.command {
        return handle_daemon(action.clone()).await;
    }

    // Handle pool subcommand (daemon-only)
    if let Commands::Pool { .. } = cli.command {
        if let Some(result) = try_daemon_dispatch(&cli).await {
            return result;
        }
        eprintln!("Error: pool commands require a running daemon");
        std::process::exit(1);
    }

    // Try daemon gRPC dispatch
    if let Some(result) = try_daemon_dispatch(&cli).await {
        return result;
    }

    // Fall through to direct provider access
    // Determine provider from the Create command, default to docker for others
    let provider_name = match &cli.command {
        Commands::Create { provider, .. } => provider.clone(),
        _ => "docker".to_string(),
    };

    match provider_name.as_str() {
        "firecracker" => {
            let provider = FirecrackerProvider::new()?;
            run_provider_commands!(provider, cli.command)
        }
        "wasm" => {
            use roche_core::provider::wasm::WasmProvider;
            let provider = WasmProvider::new()?;
            run_provider_commands!(provider, cli.command)
        }
        "k8s" => {
            use roche_core::provider::k8s::K8sProvider;
            let provider = K8sProvider::new().await?;
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
        "e2b" => {
            use roche_core::provider::e2b::E2bProvider;
            let provider = E2bProvider::new()?;
            // Handle Cp for E2B (supports file ops)
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
        _ => {
            // Handle Cp specially since it requires SandboxFileOps (Docker-only)
            if let Commands::Cp { ref src, ref dest } = cli.command {
                use roche_core::provider::SandboxFileOps;
                let provider = DockerProvider::new();
                match (parse_cp_path(src), parse_cp_path(dest)) {
                    (Some((sandbox_id, sandbox_path)), None) => {
                        let id = sandbox_id.to_string();
                        provider
                            .copy_from(&id, sandbox_path, std::path::Path::new(dest))
                            .await?;
                    }
                    (None, Some((sandbox_id, sandbox_path))) => {
                        let id = sandbox_id.to_string();
                        provider
                            .copy_to(&id, std::path::Path::new(src), sandbox_path)
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
                let provider = DockerProvider::new();
                run_provider_commands!(provider, cli.command)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_cp_path, parse_env_vars, parse_mount};

    #[test]
    fn test_parse_env_vars_happy_path() {
        let input = vec!["FOO=bar".to_string()];
        let result = parse_env_vars(&input).unwrap();
        assert_eq!(result.get("FOO").unwrap(), "bar");
    }

    #[test]
    fn test_parse_env_vars_value_with_equals() {
        let input = vec!["A=b=c".to_string()];
        let result = parse_env_vars(&input).unwrap();
        assert_eq!(result.get("A").unwrap(), "b=c");
    }

    #[test]
    fn test_parse_env_vars_malformed() {
        let input = vec!["NOEQUALS".to_string()];
        assert!(parse_env_vars(&input).is_err());
    }

    #[test]
    fn test_parse_env_vars_multiple() {
        let input = vec!["FOO=bar".to_string(), "BAZ=qux".to_string()];
        let result = parse_env_vars(&input).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("FOO").unwrap(), "bar");
        assert_eq!(result.get("BAZ").unwrap(), "qux");
    }

    #[test]
    fn test_parse_mount_with_mode() {
        let m = parse_mount("/host:/container:rw").unwrap();
        assert_eq!(m.host_path, "/host");
        assert_eq!(m.container_path, "/container");
        assert!(!m.readonly);
    }

    #[test]
    fn test_parse_mount_default_readonly() {
        let m = parse_mount("/host:/container").unwrap();
        assert!(m.readonly);
    }

    #[test]
    fn test_parse_mount_invalid() {
        assert!(parse_mount("nocolon").is_err());
        assert!(parse_mount("/host:/container:xx").is_err());
    }

    #[test]
    fn test_parse_cp_path() {
        assert_eq!(
            parse_cp_path("abc123:/app/file"),
            Some(("abc123", "/app/file"))
        );
        assert_eq!(parse_cp_path("./local.txt"), None);
    }
}
