// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
)

// CLITransport implements Transport by shelling out to the roche CLI binary.
type CLITransport struct {
	Binary string // path to the roche binary
}

// NewCLITransport creates a CLITransport using the given binary path.
// If binary is empty, the default "roche" is used.
func NewCLITransport(binary string) *CLITransport {
	if binary == "" {
		binary = defaultBinary
	}
	return &CLITransport{Binary: binary}
}

var _ Transport = (*CLITransport)(nil)

// run executes a CLI command and returns stdout. Non-zero exit → error.
func (c *CLITransport) run(ctx context.Context, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, c.Binary, args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()
	if err != nil {
		if isBinaryNotFound(err) {
			return "", fmt.Errorf("%w: %s not found in PATH", ErrUnavailable, c.Binary)
		}
		if mapped := mapCLIError(stderr.String()); mapped != nil {
			return "", mapped
		}
		return "", fmt.Errorf("roche cli: %w: %s", err, strings.TrimSpace(stderr.String()))
	}
	return strings.TrimSpace(stdout.String()), nil
}

// runUnchecked executes a CLI command and returns stdout, stderr, and exit code.
// A non-zero exit code is NOT treated as an error (used for exec).
func (c *CLITransport) runUnchecked(ctx context.Context, args ...string) (string, string, int, error) {
	cmd := exec.CommandContext(ctx, c.Binary, args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()
	if err != nil {
		if isBinaryNotFound(err) {
			return "", "", -1, fmt.Errorf("%w: %s not found in PATH", ErrUnavailable, c.Binary)
		}
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			return strings.TrimSpace(stdout.String()),
				strings.TrimSpace(stderr.String()),
				exitErr.ExitCode(),
				nil
		}
		return "", "", -1, fmt.Errorf("roche cli: %w: %s", err, strings.TrimSpace(stderr.String()))
	}
	return strings.TrimSpace(stdout.String()), strings.TrimSpace(stderr.String()), 0, nil
}

// isBinaryNotFound returns true if the error indicates the binary was not found.
func isBinaryNotFound(err error) bool {
	if errors.Is(err, exec.ErrNotFound) {
		return true
	}
	// On some platforms, exec with an absolute path to a nonexistent binary
	// returns a PathError rather than exec.ErrNotFound.
	var pathErr *exec.Error
	if errors.As(err, &pathErr) {
		return true
	}
	// Fallback: check the error message for common "not found" patterns.
	msg := err.Error()
	return strings.Contains(msg, "no such file or directory") ||
		strings.Contains(msg, "executable file not found")
}

// mapCLIError maps stderr text starting with "Error: " to sentinel errors.
// Returns nil if stderr does not start with "Error: ".
func mapCLIError(stderr string) error {
	stderr = strings.TrimSpace(stderr)
	if !strings.HasPrefix(stderr, "Error: ") {
		return nil
	}
	msg := strings.ToLower(stderr)
	switch {
	case strings.Contains(msg, "not found"):
		return fmt.Errorf("%w: %s", ErrNotFound, stderr)
	case strings.Contains(msg, "paused"):
		return fmt.Errorf("%w: %s", ErrPaused, stderr)
	case strings.Contains(msg, "unavailable") || strings.Contains(msg, "connection refused"):
		return fmt.Errorf("%w: %s", ErrUnavailable, stderr)
	case strings.Contains(msg, "timeout") || strings.Contains(msg, "timed out"):
		return fmt.Errorf("%w: %s", ErrTimeout, stderr)
	case strings.Contains(msg, "unsupported"):
		return fmt.Errorf("%w: %s", ErrUnsupported, stderr)
	default:
		return fmt.Errorf("roche cli: %s", stderr)
	}
}

// splitNonEmpty splits s by newlines and returns non-empty trimmed strings.
func splitNonEmpty(s string) []string {
	var out []string
	for _, line := range strings.Split(s, "\n") {
		line = strings.TrimSpace(line)
		if line != "" {
			out = append(out, line)
		}
	}
	return out
}

// --- arg builders ---

func (c *CLITransport) buildCreateArgs(cfg SandboxConfig, provider string) []string {
	args := []string{"create",
		"--provider", provider,
		"--image", cfg.Image,
		"--timeout", strconv.FormatUint(cfg.TimeoutSecs, 10),
	}
	if cfg.Network {
		args = append(args, "--network")
	}
	if cfg.Writable {
		args = append(args, "--writable")
	}
	if cfg.Memory != "" {
		args = append(args, "--memory", cfg.Memory)
	}
	if cfg.CPUs > 0 {
		args = append(args, "--cpus", strconv.FormatFloat(cfg.CPUs, 'f', -1, 64))
	}
	for k, v := range cfg.Env {
		args = append(args, "--env", k+"="+v)
	}
	for _, m := range cfg.Mounts {
		mode := "rw"
		if m.Readonly {
			mode = "ro"
		}
		args = append(args, "--mount", m.HostPath+":"+m.ContainerPath+":"+mode)
	}
	return args
}

func (c *CLITransport) buildExecArgs(sandboxID string, command []string, provider string, timeoutSecs *uint64) []string {
	args := []string{"exec",
		"--sandbox", sandboxID,
		"--provider", provider,
	}
	if timeoutSecs != nil {
		args = append(args, "--timeout", strconv.FormatUint(*timeoutSecs, 10))
	}
	args = append(args, "--")
	args = append(args, command...)
	return args
}

func (c *CLITransport) buildDestroyArgs(sandboxIDs []string, provider string, all bool) []string {
	args := []string{"destroy", "--provider", provider}
	if all {
		args = append(args, "--all")
	} else {
		args = append(args, sandboxIDs...)
	}
	return args
}

func (c *CLITransport) buildListArgs(provider string) []string {
	return []string{"list", "--provider", provider, "--json"}
}

func (c *CLITransport) buildCopyToArgs(sandboxID, hostPath, sandboxPath, provider string) []string {
	return []string{"cp", "--provider", provider, hostPath, sandboxID + ":" + sandboxPath}
}

func (c *CLITransport) buildCopyFromArgs(sandboxID, sandboxPath, hostPath, provider string) []string {
	return []string{"cp", "--provider", provider, sandboxID + ":" + sandboxPath, hostPath}
}

// --- Transport implementation ---

func (c *CLITransport) Create(ctx context.Context, cfg SandboxConfig, provider string) (string, error) {
	args := c.buildCreateArgs(cfg, provider)
	out, err := c.run(ctx, args...)
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(out), nil
}

func (c *CLITransport) Exec(ctx context.Context, sandboxID string, command []string, provider string, timeoutSecs *uint64) (*ExecOutput, error) {
	args := c.buildExecArgs(sandboxID, command, provider, timeoutSecs)
	stdout, stderr, exitCode, err := c.runUnchecked(ctx, args...)
	if err != nil {
		return nil, err
	}
	return &ExecOutput{
		ExitCode: int32(exitCode),
		Stdout:   stdout,
		Stderr:   stderr,
	}, nil
}

func (c *CLITransport) Destroy(ctx context.Context, sandboxIDs []string, provider string, all bool) ([]string, error) {
	args := c.buildDestroyArgs(sandboxIDs, provider, all)
	out, err := c.run(ctx, args...)
	if err != nil {
		return nil, err
	}
	return splitNonEmpty(out), nil
}

func (c *CLITransport) List(ctx context.Context, provider string) ([]SandboxInfo, error) {
	args := c.buildListArgs(provider)
	out, err := c.run(ctx, args...)
	if err != nil {
		return nil, err
	}
	if out == "" {
		return nil, nil
	}
	var infos []SandboxInfo
	if err := json.Unmarshal([]byte(out), &infos); err != nil {
		return nil, fmt.Errorf("roche cli: failed to parse list output: %w", err)
	}
	return infos, nil
}

func (c *CLITransport) Pause(ctx context.Context, sandboxID, provider string) error {
	_, err := c.run(ctx, "pause", "--sandbox", sandboxID, "--provider", provider)
	return err
}

func (c *CLITransport) Unpause(ctx context.Context, sandboxID, provider string) error {
	_, err := c.run(ctx, "unpause", "--sandbox", sandboxID, "--provider", provider)
	return err
}

func (c *CLITransport) GC(ctx context.Context, provider string, dryRun, all bool) ([]string, error) {
	args := []string{"gc", "--provider", provider}
	if dryRun {
		args = append(args, "--dry-run")
	}
	if all {
		args = append(args, "--all")
	}
	out, err := c.run(ctx, args...)
	if err != nil {
		return nil, err
	}
	return splitNonEmpty(out), nil
}

func (c *CLITransport) CopyTo(ctx context.Context, sandboxID, hostPath, sandboxPath, provider string) error {
	args := c.buildCopyToArgs(sandboxID, hostPath, sandboxPath, provider)
	_, err := c.run(ctx, args...)
	return err
}

func (c *CLITransport) CopyFrom(ctx context.Context, sandboxID, sandboxPath, hostPath, provider string) error {
	args := c.buildCopyFromArgs(sandboxID, sandboxPath, hostPath, provider)
	_, err := c.run(ctx, args...)
	return err
}

func (c *CLITransport) PoolStatus(ctx context.Context) ([]PoolInfo, error) {
	out, err := c.run(ctx, "pool", "status", "--json")
	if err != nil {
		return nil, err
	}
	if out == "" {
		return nil, nil
	}
	var infos []PoolInfo
	if err := json.Unmarshal([]byte(out), &infos); err != nil {
		return nil, fmt.Errorf("roche cli: failed to parse pool status output: %w", err)
	}
	return infos, nil
}

func (c *CLITransport) PoolWarmup(ctx context.Context, pools []PoolConfig) error {
	args := []string{"pool", "warmup"}
	for _, p := range pools {
		args = append(args, "--pool", fmt.Sprintf("%s:%s:%d", p.Provider, p.Image, p.Count))
	}
	_, err := c.run(ctx, args...)
	return err
}

func (c *CLITransport) PoolDrain(ctx context.Context, provider, image string) error {
	_, err := c.run(ctx, "pool", "drain", "--provider", provider, "--image", image)
	return err
}

// Close is a no-op for CLI transport.
func (c *CLITransport) Close() error {
	return nil
}
