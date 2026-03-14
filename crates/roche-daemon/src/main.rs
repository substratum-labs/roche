use clap::Parser;
use tonic::transport::Server;

pub mod proto {
    tonic::include_proto!("roche.v1");
}

mod gc;
mod server;

#[derive(Parser)]
#[command(name = "roche-daemon", about = "Roche sandbox orchestrator daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "50051")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let addr = format!("127.0.0.1:{}", args.port).parse()?;

    let service = server::SandboxServiceImpl::new();
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

    gc_handle.abort();

    // Clean up daemon.json
    let _ = std::fs::remove_file(&daemon_json);

    Ok(())
}
