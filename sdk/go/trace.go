// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import "fmt"

// TraceLevel controls the detail level of execution traces.
type TraceLevel string

const (
	TraceLevelOff      TraceLevel = "off"
	TraceLevelSummary  TraceLevel = "summary"
	TraceLevelStandard TraceLevel = "standard"
	TraceLevelFull     TraceLevel = "full"
)

// ExecOptions holds optional parameters for Exec calls.
type ExecOptions struct {
	TimeoutSecs    *uint64
	TraceLevel     TraceLevel
	IdempotencyKey string
}

// ExecutionTrace contains structured trace data from a sandbox execution.
type ExecutionTrace struct {
	DurationSecs     float64
	ResourceUsage    ResourceUsage
	FileAccesses     []FileAccess
	NetworkAttempts  []NetworkAttempt
	BlockedOps       []BlockedOperation
	Syscalls         []SyscallEvent
	ResourceTimeline []ResourceSnapshot
}

// ResourceUsage holds aggregate resource consumption.
type ResourceUsage struct {
	PeakMemoryBytes uint64
	CPUTimeSecs     float64
	NetworkRxBytes  uint64
	NetworkTxBytes  uint64
}

// FileAccess represents a single file operation observed during execution.
type FileAccess struct {
	Path      string
	Op        string
	SizeBytes *uint64
}

// NetworkAttempt represents a network connection attempt.
type NetworkAttempt struct {
	Address  string
	Protocol string
	Allowed  bool
}

// BlockedOperation represents an operation blocked by sandbox policy.
type BlockedOperation struct {
	OpType string
	Detail string
}

// SyscallEvent represents a captured syscall.
type SyscallEvent struct {
	Name        string
	Args        []string
	Result      string
	TimestampMs uint64
}

// ResourceSnapshot is a point-in-time resource usage sample.
type ResourceSnapshot struct {
	TimestampMs uint64
	MemoryBytes uint64
	CPUPercent  float32
}

// Summary returns a concise LLM-friendly summary of the trace.
func (t *ExecutionTrace) Summary() string {
	if t == nil {
		return "no trace"
	}
	return fmt.Sprintf(
		"duration=%.3fs peak_mem=%d cpu=%.3fs files=%d net=%d blocked=%d",
		t.DurationSecs,
		t.ResourceUsage.PeakMemoryBytes,
		t.ResourceUsage.CPUTimeSecs,
		len(t.FileAccesses),
		len(t.NetworkAttempts),
		len(t.BlockedOps),
	)
}
