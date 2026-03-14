use clap::Parser;

pub mod proto {
    tonic::include_proto!("roche.v1");
}

#[derive(Parser)]
#[command(name = "roche-daemon", about = "Roche sandbox orchestrator daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "50051")]
    port: u16,
}

#[tokio::main]
async fn main() {
    let _args = Args::parse();
    println!("roche-daemon placeholder");
}
