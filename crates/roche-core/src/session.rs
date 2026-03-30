// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

//! Execution Sessions — stateful, multi-exec sandbox sessions with
//! dynamic permission control and budget tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Unique session identifier.
pub type SessionId = String;

/// Budget limits for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Budget {
    /// Maximum number of exec calls. 0 = unlimited.
    #[serde(default)]
    pub max_execs: u32,
    /// Maximum total execution time in seconds. 0 = unlimited.
    #[serde(default)]
    pub max_total_secs: u64,
    /// Maximum total output bytes. 0 = unlimited.
    #[serde(default)]
    pub max_output_bytes: u64,
}

/// Tracks budget consumption during a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BudgetUsage {
    pub exec_count: u32,
    pub total_secs: f64,
    pub output_bytes: u64,
}

/// Dynamic permissions that can change during a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DynamicPermissions {
    pub network: bool,
    pub network_allowlist: Vec<String>,
    pub writable: bool,
    pub fs_paths: Vec<String>,
}

/// A permission change request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionChange {
    /// Grant network access to a host.
    AllowHost(String),
    /// Revoke network access to a host.
    DenyHost(String),
    /// Grant write access to a path.
    AllowPath(String),
    /// Revoke write access to a path.
    DenyPath(String),
    /// Enable network access entirely.
    EnableNetwork,
    /// Disable network access entirely.
    DisableNetwork,
}

/// Session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: SessionId,
    pub sandbox_id: String,
    pub provider: String,
    pub permissions: DynamicPermissions,
    pub budget: Budget,
    pub usage: BudgetUsage,
    pub created_at_ms: u64,
}

/// Error from session operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

/// Manages active execution sessions.
pub struct SessionManager {
    sessions: Mutex<HashMap<SessionId, SessionState>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Create a new session.
    pub fn create(
        &self,
        sandbox_id: String,
        provider: String,
        permissions: DynamicPermissions,
        budget: Budget,
    ) -> SessionId {
        let id = format!("ses_{}", uuid_v4_short());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let state = SessionState {
            id: id.clone(),
            sandbox_id,
            provider,
            permissions,
            budget,
            usage: BudgetUsage::default(),
            created_at_ms: now_ms,
        };

        self.sessions.lock().unwrap().insert(id.clone(), state);
        id
    }

    /// Get session state.
    pub fn get(&self, id: &str) -> Result<SessionState, SessionError> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// Check budget before exec.
    pub fn check_budget(&self, id: &str) -> Result<(), SessionError> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        if session.budget.max_execs > 0 && session.usage.exec_count >= session.budget.max_execs {
            return Err(SessionError::BudgetExceeded(format!(
                "exec limit reached: {}/{}",
                session.usage.exec_count, session.budget.max_execs
            )));
        }
        if session.budget.max_total_secs > 0
            && session.usage.total_secs >= session.budget.max_total_secs as f64
        {
            return Err(SessionError::BudgetExceeded(format!(
                "time limit reached: {:.1}s/{}s",
                session.usage.total_secs, session.budget.max_total_secs
            )));
        }
        if session.budget.max_output_bytes > 0
            && session.usage.output_bytes >= session.budget.max_output_bytes
        {
            return Err(SessionError::BudgetExceeded(format!(
                "output limit reached: {}/{}",
                session.usage.output_bytes, session.budget.max_output_bytes
            )));
        }
        Ok(())
    }

    /// Record exec usage.
    pub fn record_usage(
        &self,
        id: &str,
        duration_secs: f64,
        output_bytes: u64,
    ) -> Result<(), SessionError> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        session.usage.exec_count += 1;
        session.usage.total_secs += duration_secs;
        session.usage.output_bytes += output_bytes;
        Ok(())
    }

    /// Apply a permission change to a session.
    pub fn change_permissions(
        &self,
        id: &str,
        change: PermissionChange,
    ) -> Result<DynamicPermissions, SessionError> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        match change {
            PermissionChange::AllowHost(host) => {
                if !session.permissions.network_allowlist.contains(&host) {
                    session.permissions.network_allowlist.push(host);
                }
                session.permissions.network = true;
            }
            PermissionChange::DenyHost(host) => {
                session.permissions.network_allowlist.retain(|h| h != &host);
            }
            PermissionChange::AllowPath(path) => {
                if !session.permissions.fs_paths.contains(&path) {
                    session.permissions.fs_paths.push(path);
                }
                session.permissions.writable = true;
            }
            PermissionChange::DenyPath(path) => {
                session.permissions.fs_paths.retain(|p| p != &path);
            }
            PermissionChange::EnableNetwork => {
                session.permissions.network = true;
            }
            PermissionChange::DisableNetwork => {
                session.permissions.network = false;
                session.permissions.network_allowlist.clear();
            }
        }

        Ok(session.permissions.clone())
    }

    /// Destroy a session.
    pub fn destroy(&self, id: &str) -> Result<SessionState, SessionError> {
        self.sessions
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    /// List active sessions.
    pub fn list(&self) -> Vec<SessionState> {
        self.sessions.lock().unwrap().values().cloned().collect()
    }
}

fn uuid_v4_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get() {
        let mgr = SessionManager::new();
        let id = mgr.create(
            "sb-1".into(),
            "docker".into(),
            DynamicPermissions::default(),
            Budget::default(),
        );
        let state = mgr.get(&id).unwrap();
        assert_eq!(state.sandbox_id, "sb-1");
    }

    #[test]
    fn test_budget_enforcement() {
        let mgr = SessionManager::new();
        let id = mgr.create(
            "sb-1".into(),
            "docker".into(),
            DynamicPermissions::default(),
            Budget {
                max_execs: 2,
                ..Default::default()
            },
        );
        assert!(mgr.check_budget(&id).is_ok());
        mgr.record_usage(&id, 1.0, 100).unwrap();
        assert!(mgr.check_budget(&id).is_ok());
        mgr.record_usage(&id, 1.0, 100).unwrap();
        assert!(mgr.check_budget(&id).is_err());
    }

    #[test]
    fn test_permission_changes() {
        let mgr = SessionManager::new();
        let id = mgr.create(
            "sb-1".into(),
            "docker".into(),
            DynamicPermissions::default(),
            Budget::default(),
        );
        let perms = mgr
            .change_permissions(&id, PermissionChange::AllowHost("api.openai.com".into()))
            .unwrap();
        assert!(perms.network);
        assert!(perms.network_allowlist.contains(&"api.openai.com".to_string()));

        let perms = mgr
            .change_permissions(&id, PermissionChange::DenyHost("api.openai.com".into()))
            .unwrap();
        assert!(!perms.network_allowlist.contains(&"api.openai.com".to_string()));
    }

    #[test]
    fn test_destroy() {
        let mgr = SessionManager::new();
        let id = mgr.create(
            "sb-1".into(),
            "docker".into(),
            DynamicPermissions::default(),
            Budget::default(),
        );
        assert!(mgr.destroy(&id).is_ok());
        assert!(mgr.get(&id).is_err());
    }

    #[test]
    fn test_output_budget() {
        let mgr = SessionManager::new();
        let id = mgr.create(
            "sb-1".into(),
            "docker".into(),
            DynamicPermissions::default(),
            Budget {
                max_output_bytes: 1000,
                ..Default::default()
            },
        );
        mgr.record_usage(&id, 0.5, 500).unwrap();
        assert!(mgr.check_budget(&id).is_ok());
        mgr.record_usage(&id, 0.5, 600).unwrap();
        assert!(mgr.check_budget(&id).is_err());
    }
}
