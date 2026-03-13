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

        /// Environment variables (KEY=VALUE, repeatable)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,
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
            env,
        } => {
            let env_map = parse_env_vars(&env).map_err(|e| {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }).unwrap();
            let config = SandboxConfig {
                provider: provider_name,
                image,
                memory,
                cpus,
                timeout_secs: timeout,
                network,
                writable,
                env: env_map,
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

#[cfg(test)]
mod tests {
    use super::parse_env_vars;

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
}
