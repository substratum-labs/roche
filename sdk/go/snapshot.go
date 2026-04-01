// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
)

// Snapshot represents a committed image of a sandbox at a point in time.
type Snapshot struct {
	// SnapshotID is the Docker image ID (short hash) of the committed image.
	SnapshotID string
	// SandboxID is the container that was snapshotted.
	SandboxID string
	// Provider is the sandbox provider (e.g., "docker").
	Provider string
	// Image is the full image reference (e.g., "roche-snap-<container>").
	Image string
}

// SnapshotSandbox commits the current state of a running sandbox to a new
// Docker image. The sandbox keeps running; the returned Snapshot can later
// be used with RestoreSnapshot or deleted with DeleteSnapshot.
//
//	snap, err := roche.SnapshotSandbox(ctx, sandbox.ID())
func SnapshotSandbox(ctx context.Context, sandboxID string) (*Snapshot, error) {
	image := fmt.Sprintf("roche-snap-%s", sandboxID)

	cmd := exec.CommandContext(ctx, "docker", "commit", sandboxID, image)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("docker commit failed: %w: %s", err, strings.TrimSpace(string(out)))
	}

	// docker commit prints "sha256:<hash>\n"; extract the short ID.
	full := strings.TrimSpace(string(out))
	shortID := full
	if idx := strings.Index(full, ":"); idx >= 0 {
		shortID = full[idx+1:]
	}
	if len(shortID) > 12 {
		shortID = shortID[:12]
	}

	return &Snapshot{
		SnapshotID: shortID,
		SandboxID:  sandboxID,
		Provider:   "docker",
		Image:      image,
	}, nil
}

// RestoreSnapshot creates a new sandbox from a snapshot image, executes the
// given command, and destroys the sandbox. It is a one-shot restore.
//
//	out, err := roche.RestoreSnapshot(ctx, snap, []string{"python3", "-c", "print('hello')"})
func RestoreSnapshot(ctx context.Context, snap *Snapshot, command []string, opts ...ExecOptions) (*ExecOutput, error) {
	if snap == nil {
		return nil, fmt.Errorf("snapshot must not be nil")
	}

	var o ExecOptions
	if len(opts) > 0 {
		o = opts[0]
	}

	provider := snap.Provider
	if provider == "" {
		provider = detectProvider()
	}

	client, err := New(WithProvider(provider))
	if err != nil {
		return nil, fmt.Errorf("creating client for restore: %w", err)
	}

	sandbox, err := client.Create(ctx, SandboxConfig{
		Image: snap.Image,
	})
	if err != nil {
		return nil, fmt.Errorf("creating sandbox from snapshot %s: %w", snap.SnapshotID, err)
	}
	defer sandbox.Destroy(ctx)

	return sandbox.Exec(ctx, command, &o)
}

// DeleteSnapshot removes the Docker image created by SnapshotSandbox.
//
//	err := roche.DeleteSnapshot(ctx, snap)
func DeleteSnapshot(ctx context.Context, snap *Snapshot) error {
	if snap == nil {
		return fmt.Errorf("snapshot must not be nil")
	}

	cmd := exec.CommandContext(ctx, "docker", "rmi", snap.Image)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("docker rmi failed: %w: %s", err, strings.TrimSpace(string(out)))
	}
	return nil
}
