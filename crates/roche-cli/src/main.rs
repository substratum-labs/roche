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

        /// Volume mounts (host:container[:ro|rw], repeatable)
        #[arg(long = "mount", value_name = "HOST:CONTAINER[:ro|rw]")]
        mounts: Vec<String>,
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

    /// Copy files between host and sandbox
    Cp {
        /// Source path (local path or sandbox_id:/path)
        src: String,
        /// Destination path (local path or sandbox_id:/path)
        dest: String,
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

async fn run(cli: Cli) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::docker::DockerProvider;
    use roche_core::provider::{SandboxLifecycle, SandboxProvider};
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
            mounts,
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
            let config = SandboxConfig {
                provider: provider_name,
                image,
                memory,
                cpus,
                timeout_secs: timeout,
                network,
                writable,
                env: env_map,
                mounts: mount_configs,
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
                println!("{:<16} {:<10} {:<10} {:<10} IMAGE", "ID", "STATUS", "PROVIDER", "EXPIRES");
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
        Commands::Cp { src, dest } => {
            use roche_core::provider::SandboxFileOps;

            match (parse_cp_path(&src), parse_cp_path(&dest)) {
                (Some((sandbox_id, sandbox_path)), None) => {
                    let id = sandbox_id.to_string();
                    provider
                        .copy_from(&id, sandbox_path, std::path::Path::new(&dest))
                        .await?;
                }
                (None, Some((sandbox_id, sandbox_path))) => {
                    let id = sandbox_id.to_string();
                    provider
                        .copy_to(&id, std::path::Path::new(&src), sandbox_path)
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
        }
    }

    Ok(())
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
