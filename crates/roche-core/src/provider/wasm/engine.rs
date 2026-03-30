// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::provider::ProviderError;
use crate::types::{ExecOutput, ExecRequest, SandboxConfig};

use wasmtime_wasi::pipe::MemoryOutputPipe;
use wasmtime_wasi::preview1::WasiP1Ctx;
use wasmtime_wasi::WasiCtxBuilder;

/// Wrapper around wasmtime::Engine for WASM module operations.
pub struct WasmEngine {
    engine: wasmtime::Engine,
}

impl WasmEngine {
    /// Create a new Wasmtime Engine with default config.
    pub fn new() -> Result<Self, ProviderError> {
        let engine = wasmtime::Engine::default();
        Ok(Self { engine })
    }

    /// Compile a .wasm file from disk into a wasmtime Module.
    ///
    /// Maps file-not-found and invalid WASM errors to `ProviderError::CreateFailed`.
    pub fn compile(&self, wasm_path: &str) -> Result<wasmtime::Module, ProviderError> {
        wasmtime::Module::from_file(&self.engine, wasm_path).map_err(|e| {
            ProviderError::CreateFailed(format!("failed to compile WASM module '{wasm_path}': {e}"))
        })
    }

    /// Execute a WASI module, capturing stdout/stderr and the exit code.
    ///
    /// Sets up argv, environment variables, preopened directories, and
    /// in-memory pipes for output capture before invoking `_start`.
    pub fn execute(
        &self,
        module: &wasmtime::Module,
        config: &SandboxConfig,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        let stdout_pipe = MemoryOutputPipe::new(usize::MAX);
        let stderr_pipe = MemoryOutputPipe::new(usize::MAX);

        // Build the WASI context.
        let mut wasi_builder = WasiCtxBuilder::new();

        // Set argv from request command.
        wasi_builder.args(&request.command);

        // Set environment variables from config.
        for (key, value) in &config.env {
            wasi_builder.env(key, value);
        }

        // Set up preopened directories from config mounts.
        for mount in &config.mounts {
            let mut dir_perms = wasmtime_wasi::DirPerms::READ;
            let mut file_perms = wasmtime_wasi::FilePerms::READ;

            if !mount.readonly {
                dir_perms |= wasmtime_wasi::DirPerms::MUTATE;
                file_perms |= wasmtime_wasi::FilePerms::WRITE;
            }

            wasi_builder
                .preopened_dir(
                    &mount.host_path,
                    &mount.container_path,
                    dir_perms,
                    file_perms,
                )
                .map_err(|e| {
                    ProviderError::ExecFailed(format!(
                        "failed to open mount '{}': {e}",
                        mount.host_path
                    ))
                })?;
        }

        // Wire up stdout/stderr capture.
        wasi_builder.stdout(stdout_pipe.clone());
        wasi_builder.stderr(stderr_pipe.clone());

        // Build the WASI P1 context and create the store.
        let wasi_ctx = wasi_builder.build_p1();
        let mut store = wasmtime::Store::new(&self.engine, wasi_ctx);

        // Create a linker and add WASI imports.
        let mut linker: wasmtime::Linker<WasiP1Ctx> = wasmtime::Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |ctx| ctx).map_err(|e| {
            ProviderError::ExecFailed(format!("failed to configure WASI linker: {e}"))
        })?;

        // Instantiate the module.
        let instance = linker.instantiate(&mut store, module).map_err(|e| {
            ProviderError::ExecFailed(format!("failed to instantiate WASM module: {e}"))
        })?;

        // Call _start.
        let exit_code: i32;

        let start = instance.get_typed_func::<(), ()>(&mut store, "_start");
        match start {
            Ok(func) => match func.call(&mut store, ()) {
                Ok(()) => {
                    exit_code = 0;
                }
                Err(e) => {
                    if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                        exit_code = exit.0;
                    } else {
                        return Err(ProviderError::ExecFailed(format!("WASM trap: {e}")));
                    }
                }
            },
            Err(_) => {
                return Err(ProviderError::ExecFailed("no _start export found".into()));
            }
        }

        // Collect captured output.
        let stdout_bytes = stdout_pipe.try_into_inner().unwrap_or_default();
        let stderr_bytes = stderr_pipe.try_into_inner().unwrap_or_default();

        let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
        let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();

        Ok(ExecOutput {
            exit_code,
            stdout,
            stderr,
            trace: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_engine_successfully() {
        let engine = WasmEngine::new();
        assert!(engine.is_ok(), "WasmEngine::new() should succeed");
    }

    #[test]
    fn test_compile_invalid_path_returns_create_failed() {
        let engine = WasmEngine::new().unwrap();
        let result = engine.compile("/nonexistent/path/to/module.wasm");
        assert!(result.is_err());
        match result.unwrap_err() {
            ProviderError::CreateFailed(msg) => {
                assert!(
                    msg.contains("/nonexistent/path/to/module.wasm"),
                    "error should mention the path, got: {msg}"
                );
            }
            other => panic!("expected CreateFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_compile_and_execute_minimal_wat_module() {
        let engine = WasmEngine::new().unwrap();

        // A minimal WAT module that imports proc_exit and calls it with 0.
        let wat = r#"
            (module
              (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
              (memory (export "memory") 1)
              (func (export "_start")
                (call $proc_exit (i32.const 0))
              )
            )
        "#;

        let module =
            wasmtime::Module::new(&engine.engine, wat).expect("WAT compilation should succeed");

        let config = SandboxConfig::default();
        let request = ExecRequest {
            command: vec!["test".to_string()],
            timeout_secs: None,
            idempotency_key: None,
        };

        let result = engine.execute(&module, &config, &request);
        assert!(result.is_ok(), "execute should succeed, got: {result:?}");

        let output = result.unwrap();
        assert_eq!(output.exit_code, 0, "exit code should be 0");
    }
}
