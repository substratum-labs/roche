// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

//! Intent-based code analysis engine.
//!
//! Analyzes code to infer:
//! - Best provider (WASM for pure compute, Docker for packages/network, Firecracker for isolation)
//! - Minimal permission set (network allowlist, writable paths)
//! - Resource hints (memory, timeout)

use serde::{Deserialize, Serialize};

/// Result of analyzing code intent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeIntent {
    /// Recommended provider.
    pub provider: ProviderHint,
    /// Whether network access is needed.
    pub needs_network: bool,
    /// Specific hosts the code will likely contact.
    pub network_hosts: Vec<String>,
    /// Whether filesystem writes are needed.
    pub needs_writable: bool,
    /// Specific paths that need write access.
    pub writable_paths: Vec<String>,
    /// Whether package installation is detected.
    pub needs_packages: bool,
    /// Detected package manager.
    pub package_manager: Option<String>,
    /// Suggested memory hint (e.g., "256m" for data-heavy code).
    pub memory_hint: Option<String>,
    /// Detected language.
    pub language: String,
    /// Confidence score 0.0–1.0.
    pub confidence: f64,
    /// Human-readable explanation of the analysis.
    pub reasoning: Vec<String>,
}

/// Provider recommendation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderHint {
    /// Pure compute, no I/O — can use WASM for sub-ms startup.
    Wasm,
    /// Needs packages, network, or system tools — use Docker.
    #[default]
    Docker,
    /// Untrusted code needing strong isolation — use Firecracker.
    Firecracker,
}

/// Analyze code and infer execution intent.
pub fn analyze(code: &str, language: &str) -> CodeIntent {
    let mut intent = CodeIntent {
        language: language.to_string(),
        provider: ProviderHint::Wasm, // start optimistic
        confidence: 0.5,
        ..Default::default()
    };

    // Run all analyzers
    analyze_network(&mut intent, code, language);
    analyze_packages(&mut intent, code, language);
    analyze_filesystem(&mut intent, code, language);
    analyze_resources(&mut intent, code, language);

    // Determine provider based on what we found
    determine_provider(&mut intent);

    intent
}

fn analyze_network(intent: &mut CodeIntent, code: &str, language: &str) {
    // Detect HTTP/network libraries
    let network_indicators: &[(&str, &[&str])] = &[
        (
            "python",
            &[
                "import requests",
                "import urllib",
                "import httpx",
                "import aiohttp",
                "from requests",
                "from urllib",
                "from httpx",
                "from aiohttp",
                "import socket",
                "import http.client",
                "import http.server",
                "import urllib3",
                "import ftplib",
                "import smtplib",
                "import xmlrpc",
                "import grpc",
                "from urllib3",
                "from grpc",
            ],
        ),
        (
            "node",
            &[
                "require('http')",
                "require('https')",
                "require('axios')",
                "require('node-fetch')",
                "import fetch",
                "import axios",
            ],
        ),
        ("bash", &["curl ", "wget ", "nc ", "ssh "]),
    ];

    for (lang, indicators) in network_indicators {
        if *lang == language || language == "auto" {
            for indicator in *indicators {
                if code.contains(indicator) {
                    intent.needs_network = true;
                    intent
                        .reasoning
                        .push(format!("Network access needed: found `{indicator}`"));
                    break;
                }
            }
        }
    }

    // Extract URLs/hosts from code
    extract_hosts(intent, code);
}

fn extract_hosts(intent: &mut CodeIntent, code: &str) {
    // Simple pattern: look for quoted strings containing domain-like patterns
    let patterns = ["https://", "http://", "api.", "www."];

    for line in code.lines() {
        for pattern in &patterns {
            if let Some(pos) = line.find(pattern) {
                if let Some(host) = extract_host_from_url(&line[pos..]) {
                    if !intent.network_hosts.contains(&host) {
                        intent.network_hosts.push(host.clone());
                        intent.needs_network = true;
                        intent.reasoning.push(format!("Detected host: {host}"));
                    }
                }
            }
        }
    }
}

fn extract_host_from_url(s: &str) -> Option<String> {
    // Strip protocol
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);

    // Take until first non-hostname char
    let host: String = s
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == ':')
        .collect();

    // Strip port
    let host = host.split(':').next().unwrap_or(&host);

    // Basic validation: must have a dot and be reasonable length
    if host.contains('.') && host.len() > 3 && host.len() < 256 {
        Some(host.to_string())
    } else {
        None
    }
}

fn analyze_packages(intent: &mut CodeIntent, code: &str, language: &str) {
    let package_indicators: &[(&str, &str, &[&str])] = &[
        (
            "python",
            "pip",
            &["pip install", "pip3 install", "import subprocess"],
        ),
        (
            "python",
            "pip",
            &[
                "import pandas",
                "import numpy",
                "import scipy",
                "import sklearn",
                "import tensorflow",
                "import torch",
                "import matplotlib",
                "import seaborn",
                "import polars",
                "import xgboost",
                "import lightgbm",
                "import transformers",
                "import langchain",
                "import openai",
                "import anthropic",
            ],
        ),
        ("node", "npm", &["npm install", "npx ", "require('"]),
        (
            "bash",
            "apt",
            &["apt-get install", "apt install", "yum install", "apk add"],
        ),
    ];

    for (lang, pm, indicators) in package_indicators {
        if *lang == language || language == "auto" {
            for indicator in *indicators {
                if code.contains(indicator) {
                    intent.needs_packages = true;
                    intent.needs_network = true;
                    intent.package_manager = Some(pm.to_string());
                    intent
                        .reasoning
                        .push(format!("Package install needed: found `{indicator}`"));

                    // Add package registry to hosts
                    let registry = match *pm {
                        "pip" => "pypi.org",
                        "npm" => "registry.npmjs.org",
                        _ => "",
                    };
                    if !registry.is_empty() && !intent.network_hosts.contains(&registry.to_string())
                    {
                        intent.network_hosts.push(registry.to_string());
                    }
                    break;
                }
            }
        }
    }
}

fn analyze_filesystem(intent: &mut CodeIntent, code: &str, language: &str) {
    let write_indicators: &[(&str, &[&str])] = &[
        (
            "python",
            &[
                "open(",
                "with open",
                ".to_csv(",
                ".to_json(",
                ".to_parquet(",
                ".to_excel(",
                ".to_pickle(",
                ".savefig(",
                "os.makedirs(",
                "os.mkdir(",
                "os.rename(",
                "os.remove(",
                "shutil.copy(",
                "shutil.move(",
                "shutil.rmtree(",
                "pathlib",
            ],
        ),
        (
            "node",
            &[
                "fs.writeFile",
                "fs.writeSync",
                "fs.mkdir",
                "createWriteStream",
            ],
        ),
        ("bash", &[" > ", " >> ", "mkdir ", "touch ", "tee "]),
    ];

    for (lang, indicators) in write_indicators {
        if *lang == language || language == "auto" {
            for indicator in *indicators {
                if code.contains(indicator) {
                    intent.needs_writable = true;
                    intent
                        .reasoning
                        .push(format!("Filesystem write needed: found `{indicator}`"));
                    break;
                }
            }
        }
    }

    // Extract specific paths
    extract_writable_paths(intent, code);
}

fn extract_writable_paths(intent: &mut CodeIntent, code: &str) {
    // Look for common writable paths in string literals
    let common_paths = ["/tmp", "/output", "/data", "/workspace", "/home", "/app"];
    for path in &common_paths {
        if code.contains(path)
            && intent.needs_writable
            && !intent.writable_paths.contains(&path.to_string())
        {
            intent.writable_paths.push(path.to_string());
        }
    }

    // If writable but no specific paths found, default to /tmp
    if intent.needs_writable && intent.writable_paths.is_empty() {
        intent.writable_paths.push("/tmp".to_string());
    }
}

fn analyze_resources(intent: &mut CodeIntent, code: &str, _language: &str) {
    // Data-heavy libraries suggest more memory
    let heavy_libs = [
        "pandas", "numpy", "scipy", "tensorflow", "torch", "polars",
        "xgboost", "lightgbm", "transformers",
    ];
    for lib in &heavy_libs {
        if code.contains(lib) {
            intent.memory_hint = Some("512m".to_string());
            intent
                .reasoning
                .push(format!("Memory hint 512m: data library `{lib}` detected"));
            break;
        }
    }
}

fn determine_provider(intent: &mut CodeIntent) {
    if intent.needs_network || intent.needs_packages || intent.needs_writable {
        intent.provider = ProviderHint::Docker;
        intent.confidence = 0.8;
    } else {
        // Pure compute — WASM is ideal
        intent.provider = ProviderHint::Wasm;
        intent.confidence = 0.7;
    }
    intent.reasoning.push(format!(
        "Provider: {:?} (confidence: {:.0}%)",
        intent.provider,
        intent.confidence * 100.0
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_compute_selects_wasm() {
        let intent = analyze("print(2 + 2)", "python");
        assert_eq!(intent.provider, ProviderHint::Wasm);
        assert!(!intent.needs_network);
        assert!(!intent.needs_writable);
    }

    #[test]
    fn test_requests_detects_network() {
        let intent = analyze(
            "import requests\nresponses = requests.get('https://api.openai.com/v1/chat')",
            "python",
        );
        assert_eq!(intent.provider, ProviderHint::Docker);
        assert!(intent.needs_network);
        assert!(intent.network_hosts.contains(&"api.openai.com".to_string()));
    }

    #[test]
    fn test_pip_install_detects_packages() {
        let intent = analyze(
            "import subprocess\nsubprocess.run(['pip', 'install', 'pandas'])",
            "python",
        );
        assert!(intent.needs_packages || intent.needs_network);
        assert_eq!(intent.provider, ProviderHint::Docker);
    }

    #[test]
    fn test_pandas_import_detects_memory() {
        let intent = analyze(
            "import pandas as pd\ndf = pd.read_csv('data.csv')",
            "python",
        );
        assert_eq!(intent.memory_hint, Some("512m".to_string()));
        assert_eq!(intent.provider, ProviderHint::Docker);
    }

    #[test]
    fn test_file_write_detects_writable() {
        let intent = analyze(
            "with open('/tmp/output.txt', 'w') as f:\n    f.write('hello')",
            "python",
        );
        assert!(intent.needs_writable);
        assert!(intent.writable_paths.contains(&"/tmp".to_string()));
    }

    #[test]
    fn test_curl_bash() {
        let intent = analyze(
            "curl https://api.github.com/repos/foo/bar | jq '.name'",
            "bash",
        );
        assert!(intent.needs_network);
        assert!(intent.network_hosts.contains(&"api.github.com".to_string()));
        assert_eq!(intent.provider, ProviderHint::Docker);
    }

    #[test]
    fn test_node_fetch() {
        let intent = analyze("import fetch from 'node-fetch';\nconst res = await fetch('https://jsonplaceholder.typicode.com/todos/1');", "node");
        assert!(intent.needs_network);
        assert!(intent
            .network_hosts
            .contains(&"jsonplaceholder.typicode.com".to_string()));
    }

    #[test]
    fn test_host_extraction() {
        let host = extract_host_from_url("https://api.openai.com/v1/chat");
        assert_eq!(host, Some("api.openai.com".to_string()));
    }

    #[test]
    fn test_host_extraction_with_port() {
        // localhost has no dot, so it's filtered out (we only extract FQDN hosts)
        let host = extract_host_from_url("http://localhost:8080/api");
        assert_eq!(host, None);

        let host = extract_host_from_url("http://api.example.com:8080/api");
        assert_eq!(host, Some("api.example.com".to_string()));
    }
}
