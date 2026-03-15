use clap::Parser;
use std::sync::Arc;
use tonic::transport::Server;

pub mod proto {
    tonic::include_proto!("roche.v1");
}

mod gc;
mod pool;
mod server;

#[derive(Parser)]
#[command(name = "roched", about = "Roche sandbox orchestrator daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "50051")]
    port: u16,

    /// Pool configuration (format: provider/image?min=N&max=N&total=N&idle_timeout=N)
    #[arg(long = "pool")]
    pools: Vec<String>,
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
    let svc = proto::sandbox_service_server::SandboxServiceServer::new(service);

    // Write daemon.json
    let roche_dir = dirs::home_dir()
        .expect("cannot find home directory")
        .join(".roche");
    std::fs::create_dir_all(&roche_dir)?;
    let daemon_json = roche_dir.join("daemon.json");
    let info = serde_json::json!({
        "pid": std::process::id(),
        "port": args.port
    });
    std::fs::write(&daemon_json, serde_json::to_string_pretty(&info)?)?;

    tracing::info!("roche-daemon listening on {}", addr);

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
