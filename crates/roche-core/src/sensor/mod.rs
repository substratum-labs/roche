// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

pub mod docker;
pub mod types;

pub use docker::{DockerSensor, DockerTraceCollector};
pub use types::*;

use crate::provider::ProviderError;
use crate::types::SandboxId;

/// Trait for sandbox sensors that collect execution traces.
#[allow(async_fn_in_trait)]
pub trait SandboxSensor {
    /// Start collecting trace data for a sandbox execution.
    async fn start_trace(
        &self,
        id: &SandboxId,
        level: TraceLevel,
    ) -> Result<TraceCollectorHandle, ProviderError>;
}

/// Handle to a running trace collector. Call `finish()` to get the trace
/// or `abort()` to discard it.
pub enum TraceCollectorHandle {
    Docker(DockerTraceCollector),
}

impl TraceCollectorHandle {
    /// Finish collecting and return the execution trace.
    pub async fn finish(self) -> Result<ExecutionTrace, ProviderError> {
        match self {
            Self::Docker(c) => c.finish().await,
        }
    }

    /// Abort trace collection without producing a result.
    pub async fn abort(self) {
        match self {
            Self::Docker(c) => c.abort().await,
        }
    }
}

/// Dispatch enum for selecting a sensor at runtime.
pub enum SensorDispatch {
    Docker(DockerSensor),
    None,
}

impl SensorDispatch {
    /// Start a trace if a sensor is configured; returns `None` for `SensorDispatch::None`.
    pub async fn start_trace(
        &self,
        id: &SandboxId,
        level: TraceLevel,
    ) -> Option<TraceCollectorHandle> {
        match self {
            Self::Docker(s) => s.start_trace(id, level).await.ok(),
            Self::None => None,
        }
    }
}
