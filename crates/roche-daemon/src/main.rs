// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use clap::Parser;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::transport::Server;

pub mod proto {
    tonic::include_proto!("roche.v1");
}

mod gc;
mod pool;
mod server;

const SECCOMP_TRACE_PROFILE: &str = include_str!("seccomp-trace.json");

#[derive(Parser)]
#[command(name = "roched", about = "Roche sandbox orchestrator daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "50051")]
    port: u16,

    /// Pool configuration (format: provider/image?min=N&max=N&total=N&idle_timeout=N)
    #[arg(long = "pool")]
    pools: Vec<String>,

    /// Idle timeout in seconds (0 = disabled, run forever)
    #[arg(long, default_value = "0", env = "ROCHE_DAEMON_IDLE_TIMEOUT")]
    idle_timeout: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let addr = format!("127.0.0.1:{}", args.port).parse()?;

    // Load pool configs: CLI args override, then fall back to pool.toml
    let pool_configs = if !args.pools.is_empty() {
        let mut configs = Vec::new();
        for arg in &args.pools {
            match pool::config::parse_pool_arg(arg) {
                Ok(cfg) => configs.push(cfg),
                Err(e) => {
                    tracing::error!("invalid --pool arg '{arg}': {e}");
                    std::process::exit(1);
                }
            }
        }
        configs
    } else {
        pool::config::load_pool_toml()
    };

    // Create pool manager
    let pool_manager = Arc::new(pool::PoolManager::new(pool_configs));

    // Spawn pool background tasks
    let replenish_handle = tokio::spawn(pool::replenish::run_replenish_loop(
        pool_manager.inner.clone(),
        pool_manager.replenish_notify.clone(),
    ));
    let reaper_handle = tokio::spawn(pool::reaper::run_reaper_loop(pool_manager.inner.clone()));

    // Initial warmup
    pool_manager.initial_warmup().await;

    let service = server::SandboxServiceImpl::new(pool_manager.clone()).await;
    let last_rpc_ms = service.last_rpc_ms.clone();
    let svc = proto::sandbox_service_server::SandboxServiceServer::new(service);

    // Write daemon.json
    let roche_dir = dirs::home_dir()
        .expect("cannot find home directory")
        .join(".roche");
    std::fs::create_dir_all(&roche_dir)?;

    // Write seccomp trace profile if not present
    let seccomp_path = roche_dir.join("seccomp-trace.json");
    if !seccomp_path.exists() {
        std::fs::write(&seccomp_path, SECCOMP_TRACE_PROFILE)?;
    }

    let daemon_json = roche_dir.join("daemon.json");
    let info = serde_json::json!({
        "pid": std::process::id(),
        "port": args.port
    });
    std::fs::write(&daemon_json, serde_json::to_string_pretty(&info)?)?;

    tracing::info!("roche-daemon listening on {}", addr);

    // Spawn idle timeout monitor
    if args.idle_timeout > 0 {
        let last_rpc = last_rpc_ms.clone();
        let timeout_ms = args.idle_timeout * 1000;
        let daemon_json_for_idle = daemon_json.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                let last = last_rpc.load(Ordering::Relaxed);
                if last > 0 && now_ms - last > timeout_ms {
                    tracing::info!("idle timeout exceeded, shutting down");
                    let _ = std::fs::remove_file(&daemon_json_for_idle);
                    std::process::exit(0);
                }
            }
        });
    }

    // Spawn background GC
    let gc_handle = tokio::spawn(gc::run_gc_loop());

    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutting down");
    };

    Server::builder()
        .add_service(svc)
        .serve_with_shutdown(addr, shutdown)
        .await?;

    // Shutdown pool — drain idle sandboxes
    pool_manager.shutdown().await;

    gc_handle.abort();
    replenish_handle.abort();
    reaper_handle.abort();

    // Clean up daemon.json
    let _ = std::fs::remove_file(&daemon_json);

    Ok(())
}
