use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "roche", about = "Universal sandbox orchestrator for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
        command: Vec<String>,
    },

    /// Destroy a sandbox
    Destroy {
        /// Sandbox ID
        id: String,
    },

    /// List active sandboxes
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
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

async fn run(cli: Cli) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::docker::DockerProvider;
    use roche_core::provider::SandboxProvider;
    use roche_core::types::{ExecRequest, SandboxConfig};

    let provider = DockerProvider::new();

    match cli.command {
        Commands::Create {
            provider: provider_name,
            image,
            memory,
            cpus,
            timeout,
            network,
            writable,
        } => {
            let config = SandboxConfig {
                provider: provider_name,
                image,
                memory,
                cpus,
                timeout_secs: timeout,
                network,
                writable,
                ..Default::default()
            };
            let id = provider.create(&config).await?;
            println!("{id}");
        }
        Commands::Exec {
            sandbox,
            timeout,
            command,
        } => {
            let request = ExecRequest {
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
        Commands::Destroy { id } => {
            provider.destroy(&id).await?;
        }
        Commands::List { json } => {
            let sandboxes = provider.list().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&sandboxes).unwrap());
            } else if sandboxes.is_empty() {
                println!("No active sandboxes.");
            } else {
                println!("{:<16} {:<10} {:<10} IMAGE", "ID", "STATUS", "PROVIDER");
                for sb in &sandboxes {
                    println!(
                        "{:<16} {:<10} {:<10} {}",
                        sb.id,
                        format!("{:?}", sb.status).to_lowercase(),
                        sb.provider,
                        sb.image,
                    );
                }
            }
        }
    }

    Ok(())
}
