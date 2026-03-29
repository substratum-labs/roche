// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

use crate::proto;
use roche_core::provider::ProviderError;

/// Parsed retry policy from proto.
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff: BackoffStrategy,
    pub initial_delay_ms: u64,
    pub retry_on: Vec<RetryCondition>,
}

pub enum BackoffStrategy {
    None,
    Linear,
    Exponential,
}

#[derive(Debug, PartialEq)]
pub enum RetryCondition {
    Timeout,
    Oom,
    NonzeroExit,
}

impl RetryConfig {
    pub fn from_proto(policy: Option<&proto::RetryPolicy>) -> Self {
        let policy = match policy {
            Some(p) if p.max_attempts > 1 => p,
            _ => return Self::no_retry(),
        };

        let backoff = match policy.backoff.as_str() {
            "linear" => BackoffStrategy::Linear,
            "exponential" => BackoffStrategy::Exponential,
            _ => BackoffStrategy::None,
        };

        let retry_on: Vec<RetryCondition> = policy
            .retry_on
            .iter()
            .filter_map(|s| match s.as_str() {
                "timeout" => Some(RetryCondition::Timeout),
                "oom" => Some(RetryCondition::Oom),
                "nonzero_exit" => Some(RetryCondition::NonzeroExit),
                _ => None,
            })
            .collect();

        Self {
            max_attempts: policy.max_attempts,
            backoff,
            initial_delay_ms: if policy.initial_delay_ms > 0 {
                policy.initial_delay_ms
            } else {
                1000
            },
            retry_on,
        }
    }

    fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            backoff: BackoffStrategy::None,
            initial_delay_ms: 1000,
            retry_on: vec![],
        }
    }

    /// Calculate delay for the nth retry (0-indexed).
    pub fn delay_ms(&self, attempt: u32) -> u64 {
        match self.backoff {
            BackoffStrategy::None => self.initial_delay_ms,
            BackoffStrategy::Linear => self.initial_delay_ms * (attempt as u64 + 1),
            BackoffStrategy::Exponential => self.initial_delay_ms * 2u64.pow(attempt),
        }
    }

    /// Check if a provider error should trigger a retry.
    pub fn should_retry_error(&self, err: &ProviderError) -> bool {
        if self.retry_on.is_empty() {
            // Empty = retry on any error
            return true;
        }
        match err {
            ProviderError::Timeout(_) => self.retry_on.contains(&RetryCondition::Timeout),
            _ => false,
        }
    }

    /// Check if a non-zero exit code should trigger a retry.
    pub fn should_retry_exit(&self, exit_code: i32) -> bool {
        if exit_code == 0 {
            return false;
        }
        if self.retry_on.is_empty() {
            return true;
        }
        self.retry_on.contains(&RetryCondition::NonzeroExit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_retry_default() {
        let config = RetryConfig::from_proto(None);
        assert_eq!(config.max_attempts, 1);
    }

    #[test]
    fn test_exponential_backoff() {
        let config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential,
            initial_delay_ms: 100,
            retry_on: vec![],
        };
        assert_eq!(config.delay_ms(0), 100);
        assert_eq!(config.delay_ms(1), 200);
        assert_eq!(config.delay_ms(2), 400);
    }

    #[test]
    fn test_linear_backoff() {
        let config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::Linear,
            initial_delay_ms: 100,
            retry_on: vec![],
        };
        assert_eq!(config.delay_ms(0), 100);
        assert_eq!(config.delay_ms(1), 200);
        assert_eq!(config.delay_ms(2), 300);
    }

    #[test]
    fn test_should_retry_error_empty_retries_all() {
        let config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::None,
            initial_delay_ms: 100,
            retry_on: vec![],
        };
        assert!(config.should_retry_error(&ProviderError::Timeout(30)));
        assert!(config.should_retry_error(&ProviderError::ExecFailed("oom".into())));
    }

    #[test]
    fn test_should_retry_error_specific() {
        let config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::None,
            initial_delay_ms: 100,
            retry_on: vec![RetryCondition::Timeout],
        };
        assert!(config.should_retry_error(&ProviderError::Timeout(30)));
        assert!(!config.should_retry_error(&ProviderError::ExecFailed("other".into())));
    }

    #[test]
    fn test_should_retry_exit() {
        let config = RetryConfig {
            max_attempts: 3,
            backoff: BackoffStrategy::None,
            initial_delay_ms: 100,
            retry_on: vec![RetryCondition::NonzeroExit],
        };
        assert!(!config.should_retry_exit(0));
        assert!(config.should_retry_exit(1));
    }
}
