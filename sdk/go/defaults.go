// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

const (
	defaultImage       = "python:3.12-slim"
	defaultTimeoutSecs = 300
	defaultProvider    = "docker"
	defaultBinary      = "roche"
)

// applyDefaults fills in zero-value fields of cfg with sensible defaults.
// clientProvider is used as the fallback when cfg.Provider is empty.
func applyDefaults(cfg SandboxConfig, clientProvider string) SandboxConfig {
	if cfg.Provider == "" {
		cfg.Provider = clientProvider
	}
	if cfg.Image == "" {
		cfg.Image = defaultImage
	}
	if cfg.TimeoutSecs == 0 {
		cfg.TimeoutSecs = defaultTimeoutSecs
	}
	return cfg
}
