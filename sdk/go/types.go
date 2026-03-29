// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

// SandboxConfig holds the configuration for creating a new sandbox.
// Zero values align with AI-safe defaults: network off, filesystem read-only.
type SandboxConfig struct {
	Provider    string
	Image       string
	Memory      string
	CPUs        float64
	TimeoutSecs uint64
	Network     bool
	Writable    bool
	Env         map[string]string
	Mounts      []Mount
	Kernel           string
	Rootfs           string
	NetworkAllowlist []string
	FSPaths          []string
}

// Mount represents a host directory mount into the sandbox.
type Mount struct {
	HostPath      string
	ContainerPath string
	Readonly      bool
}

// NewMount creates a Mount with Readonly=true (AI-safe default).
func NewMount(hostPath, containerPath string) Mount {
	return Mount{HostPath: hostPath, ContainerPath: containerPath, Readonly: true}
}

// ExecOutput holds the result of executing a command in a sandbox.
type ExecOutput struct {
	ExitCode int32
	Stdout   string
	Stderr   string
	Trace    *ExecutionTrace
}

// SandboxStatus represents the runtime status of a sandbox.
type SandboxStatus string

const (
	StatusRunning SandboxStatus = "running"
	StatusPaused  SandboxStatus = "paused"
	StatusStopped SandboxStatus = "stopped"
	StatusFailed  SandboxStatus = "failed"
)

// SandboxInfo contains metadata about an active sandbox.
type SandboxInfo struct {
	ID        string
	Status    SandboxStatus
	Provider  string
	Image     string
	ExpiresAt *uint64
}

// GCOptions configures garbage collection behavior.
type GCOptions struct {
	DryRun bool
	All    bool
}

// PoolInfo describes the state of a sandbox pool.
type PoolInfo struct {
	Provider    string
	Image       string
	IdleCount   uint32
	ActiveCount uint32
	MaxIdle     uint32
	MaxTotal    uint32
}

// PoolConfig configures a sandbox pool warmup.
type PoolConfig struct {
	Provider string
	Image    string
	Count    uint32
}
