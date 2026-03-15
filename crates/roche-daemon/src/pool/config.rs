use serde::Deserialize;

/// Configuration for a single sandbox pool.
#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    pub provider: String,
    pub image: String,
    #[serde(default)]
    pub min_idle: usize,
    #[serde(default = "default_max_idle")]
    pub max_idle: usize,
    #[serde(default = "default_max_total")]
    pub max_total: usize,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
}

fn default_max_idle() -> usize {
    5
}
fn default_max_total() -> usize {
    20
}
fn default_idle_timeout_secs() -> u64 {
    600
}

/// Top-level structure for pool.toml.
#[derive(Debug, Deserialize)]
pub struct PoolFileConfig {
    #[serde(default)]
    pub pool: Vec<PoolConfig>,
}

/// Load pool configs from ~/.roche/pool.toml (if exists).
pub fn load_pool_toml() -> Vec<PoolConfig> {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".roche").join("pool.toml"),
        None => return vec![],
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    match toml::from_str::<PoolFileConfig>(&content) {
        Ok(cfg) => cfg.pool,
        Err(e) => {
            tracing::warn!("failed to parse pool.toml: {e}");
            vec![]
        }
    }
}

/// Parse a CLI --pool arg: "provider/image?key=value&key=value"
/// Example: "docker/python:3.12-slim?min=3&max=10&total=20&idle_timeout=600"
pub fn parse_pool_arg(arg: &str) -> Result<PoolConfig, String> {
    let (prefix, query) = match arg.split_once('?') {
        Some((p, q)) => (p, q),
        None => (arg, ""),
    };

    let (provider, image) = prefix
        .split_once('/')
        .ok_or_else(|| format!("invalid pool arg: expected provider/image, got '{prefix}'"))?;

    let mut config = PoolConfig {
        provider: provider.to_string(),
        image: image.to_string(),
        min_idle: 0,
        max_idle: default_max_idle(),
        max_total: default_max_total(),
        idle_timeout_secs: default_idle_timeout_secs(),
    };

    if !query.is_empty() {
        for pair in query.split('&') {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| format!("invalid pool param: '{pair}'"))?;
            match k {
                "min" => config.min_idle = v.parse().map_err(|_| format!("invalid min: {v}"))?,
                "max" => config.max_idle = v.parse().map_err(|_| format!("invalid max: {v}"))?,
                "total" => {
                    config.max_total = v.parse().map_err(|_| format!("invalid total: {v}"))?
                }
                "idle_timeout" => {
                    config.idle_timeout_secs =
                        v.parse().map_err(|_| format!("invalid idle_timeout: {v}"))?
                }
                other => return Err(format!("unknown pool param: '{other}'")),
            }
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pool_arg_full() {
        let cfg =
            parse_pool_arg("docker/python:3.12-slim?min=3&max=10&total=20&idle_timeout=300")
                .unwrap();
        assert_eq!(cfg.provider, "docker");
        assert_eq!(cfg.image, "python:3.12-slim");
        assert_eq!(cfg.min_idle, 3);
        assert_eq!(cfg.max_idle, 10);
        assert_eq!(cfg.max_total, 20);
        assert_eq!(cfg.idle_timeout_secs, 300);
    }

    #[test]
    fn test_parse_pool_arg_minimal() {
        let cfg = parse_pool_arg("docker/node:20-slim").unwrap();
        assert_eq!(cfg.provider, "docker");
        assert_eq!(cfg.image, "node:20-slim");
        assert_eq!(cfg.min_idle, 0);
        assert_eq!(cfg.max_idle, 5);
        assert_eq!(cfg.max_total, 20);
        assert_eq!(cfg.idle_timeout_secs, 600);
    }

    #[test]
    fn test_parse_pool_arg_partial_params() {
        let cfg = parse_pool_arg("docker/python:3.12-slim?min=2").unwrap();
        assert_eq!(cfg.min_idle, 2);
        assert_eq!(cfg.max_idle, 5);
    }

    #[test]
    fn test_parse_pool_arg_no_slash() {
        assert!(parse_pool_arg("docker-python").is_err());
    }

    #[test]
    fn test_parse_pool_arg_unknown_param() {
        assert!(parse_pool_arg("docker/img?foo=1").is_err());
    }

    #[test]
    fn test_toml_deserialization() {
        let toml_str = r#"
[[pool]]
provider = "docker"
image = "python:3.12-slim"
min_idle = 3
max_idle = 10

[[pool]]
provider = "docker"
image = "node:20-slim"
"#;
        let cfg: PoolFileConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.pool.len(), 2);
        assert_eq!(cfg.pool[0].min_idle, 3);
        assert_eq!(cfg.pool[0].max_idle, 10);
        assert_eq!(cfg.pool[0].max_total, 20);
        assert_eq!(cfg.pool[1].min_idle, 0);
    }
}
