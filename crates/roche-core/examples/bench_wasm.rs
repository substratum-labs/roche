// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs
//
// WASM vs Docker provider benchmark.
//
// Usage (from repo root):
//   cargo run --release --example bench_wasm
//
// Requires Docker running for Docker benchmarks.

use roche_core::provider::wasm::WasmProvider;
use roche_core::provider::SandboxProvider;
use roche_core::types::{ExecRequest, SandboxConfig};
use std::io::Write;
use std::time::Instant;

const WAT_HELLO: &str = r#"
(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "proc_exit"
    (func $proc_exit (param i32)))
  (memory (export "memory") 1)

  ;; "hello\n" at offset 100
  (data (i32.const 100) "hello\n")

  ;; iov: ptr=100, len=6
  (data (i32.const 0) "\64\00\00\00\06\00\00\00")

  (func (export "_start")
    ;; fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=200)
    (drop (call $fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 200)))
    (call $proc_exit (i32.const 0))
  )
)
"#;

const WAT_NOOP: &str = r#"
(module
  (import "wasi_snapshot_preview1" "proc_exit"
    (func $proc_exit (param i32)))
  (memory (export "memory") 1)
  (func (export "_start")
    (call $proc_exit (i32.const 0))
  )
)
"#;

fn write_wat_to_file(wat: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join("roche-bench");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.wat"));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(wat.as_bytes()).unwrap();
    path.to_string_lossy().to_string()
}

fn bench_wasm_create_destroy(provider: &WasmProvider, wasm_path: &str, n: usize) -> Vec<f64> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut times = Vec::with_capacity(n);

    for _ in 0..n {
        let t0 = Instant::now();
        let id = rt
            .block_on(provider.create(&SandboxConfig {
                provider: "wasm".into(),
                image: wasm_path.into(),
                ..Default::default()
            }))
            .unwrap();
        rt.block_on(provider.destroy(&id)).unwrap();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    times
}

fn bench_wasm_exec(provider: &WasmProvider, wasm_path: &str, n: usize) -> Vec<f64> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let id = rt
        .block_on(provider.create(&SandboxConfig {
            provider: "wasm".into(),
            image: wasm_path.into(),
            ..Default::default()
        }))
        .unwrap();

    let mut times = Vec::with_capacity(n);
    let req = ExecRequest {
        command: vec!["test".into()],
        timeout_secs: None,
        idempotency_key: None,
    };

    for _ in 0..n {
        let t0 = Instant::now();
        let out = rt.block_on(provider.exec(&id, &req)).unwrap();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(out.exit_code, 0);
    }
    rt.block_on(provider.destroy(&id)).unwrap();
    times
}

fn bench_docker_create_destroy(n: usize) -> Vec<f64> {
    use roche_core::provider::docker::DockerProvider;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let provider = DockerProvider::new();
    let mut times = Vec::with_capacity(n);

    for _ in 0..n {
        let t0 = Instant::now();
        let id = rt
            .block_on(provider.create(&SandboxConfig {
                provider: "docker".into(),
                image: "python:3.12-slim".into(),
                ..Default::default()
            }))
            .unwrap();
        rt.block_on(provider.destroy(&id)).unwrap();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    times
}

fn bench_docker_exec(n: usize) -> Vec<f64> {
    use roche_core::provider::docker::DockerProvider;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let provider = DockerProvider::new();

    let id = rt
        .block_on(provider.create(&SandboxConfig {
            provider: "docker".into(),
            image: "python:3.12-slim".into(),
            ..Default::default()
        }))
        .unwrap();

    let mut times = Vec::with_capacity(n);
    let req = ExecRequest {
        command: vec!["python3".into(), "-c".into(), "print('hello')".into()],
        timeout_secs: None,
        idempotency_key: None,
    };

    for _ in 0..n {
        let t0 = Instant::now();
        let out = rt.block_on(provider.exec(&id, &req)).unwrap();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(out.exit_code, 0);
    }
    rt.block_on(provider.destroy(&id)).unwrap();
    times
}

fn print_stats(name: &str, times: &[f64]) {
    if times.is_empty() {
        println!("  {name}: SKIP");
        return;
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];

    println!(
        "  {name:<45} n={:<4} mean={:>8.2}ms  median={:>8.2}ms  min={:>8.2}ms  max={:>8.2}ms  p95={:>8.2}ms",
        times.len(), mean, median, min, max, p95
    );
}

fn main() {
    println!("\n=== Roche WASM vs Docker Benchmark ===\n");

    // --- WASM ---
    let provider = WasmProvider::new().unwrap();

    let noop_path = write_wat_to_file(WAT_NOOP, "noop");
    let hello_path = write_wat_to_file(WAT_HELLO, "hello");

    println!("--- WASM Provider ---\n");

    let times = bench_wasm_create_destroy(&provider, &noop_path, 50);
    print_stats("wasm create+destroy (noop.wat)", &times);

    let times = bench_wasm_exec(&provider, &noop_path, 50);
    print_stats("wasm exec (noop)", &times);

    let times = bench_wasm_exec(&provider, &hello_path, 50);
    print_stats("wasm exec (hello → stdout)", &times);

    // --- Docker ---
    println!("\n--- Docker Provider ---\n");

    let times = bench_docker_create_destroy(5);
    print_stats("docker create+destroy (python:3.12-slim)", &times);

    let times = bench_docker_exec(10);
    print_stats("docker exec (python print('hello'))", &times);

    // --- Comparison ---
    println!("\n--- Speedup ---\n");

    let wasm_exec = bench_wasm_exec(&provider, &hello_path, 100);
    let wasm_mean = wasm_exec.iter().sum::<f64>() / wasm_exec.len() as f64;

    let docker_exec = bench_docker_exec(10);
    let docker_mean = docker_exec.iter().sum::<f64>() / docker_exec.len() as f64;

    println!("  WASM exec mean:   {:>8.2}ms", wasm_mean);
    println!("  Docker exec mean: {:>8.2}ms", docker_mean);
    println!("  Speedup:          {:>8.1}x", docker_mean / wasm_mean);

    // Cleanup
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("roche-bench"));
}
