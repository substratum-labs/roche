// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

pub mod capability;
pub mod intent;
pub mod provider;
pub mod sensor;
pub mod session;
pub mod types;

pub use provider::capabilities::{validate_config, FieldSupport, ProviderCapabilities};
pub use provider::{SandboxFileOps, SandboxLifecycle, SandboxProvider};
pub use types::{
    ExecEvent, ExecOutput, ExecRequest, MountConfig, OutputLimit, RetryPolicy, SandboxConfig,
    SandboxId, SandboxInfo, SandboxStatus,
};

// Capability Wallet — the agent sandbox spec
pub use capability::{
    SandboxGrant, ComputeCapability, FilesystemCapability, NetworkCapability,
    OutputCapability, SecretsCapability, UsageReport,
};

// v0.4: Intent analysis + Session management
pub use intent::{analyze as analyze_intent, CodeIntent, ProviderHint};
pub use session::{
    Budget, BudgetUsage, DynamicPermissions, PermissionChange, SessionError, SessionId,
    SessionManager, SessionState,
};
