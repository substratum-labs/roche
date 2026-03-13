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

        /// Command to execute
        command: Vec<String>,
    },

    /// Destroy a sandbox
    Destroy {
        /// Sandbox ID
        id: String,
    },

    /// List active sandboxes
    List,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Create {
            provider,
            image,
            memory,
            cpus,
            timeout,
            network,
            writable,
        } => {
            println!(
                "Creating sandbox: provider={provider}, image={image}, \
                 memory={memory:?}, cpus={cpus:?}, timeout={timeout}s, \
                 network={network}, writable={writable}"
            );
            todo!("wire up provider.create()")
        }
        Commands::Exec { sandbox, command } => {
            println!("Executing in {sandbox}: {}", command.join(" "));
            todo!("wire up provider.exec()")
        }
        Commands::Destroy { id } => {
            println!("Destroying sandbox {id}");
            todo!("wire up provider.destroy()")
        }
        Commands::List => {
            println!("Listing sandboxes...");
            todo!("wire up provider.list()")
        }
    }
}
