// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import "context"

// Sandbox is a handle to a running sandbox instance.
// It holds the sandbox ID, provider, and transport for convenience methods.
type Sandbox struct {
	id        string
	provider  string
	transport Transport
}

// ID returns the sandbox identifier.
func (s *Sandbox) ID() string {
	return s.id
}

// Provider returns the provider that created this sandbox.
func (s *Sandbox) Provider() string {
	return s.provider
}

// Exec runs a command inside the sandbox.
// Pass an optional ExecOptions to control timeout, trace level, and idempotency.
func (s *Sandbox) Exec(ctx context.Context, command []string, opts ...*ExecOptions) (*ExecOutput, error) {
	var o *ExecOptions
	if len(opts) > 0 {
		o = opts[0]
	}
	return s.transport.Exec(ctx, s.id, command, s.provider, o)
}

// Pause freezes the sandbox.
func (s *Sandbox) Pause(ctx context.Context) error {
	return s.transport.Pause(ctx, s.id, s.provider)
}

// Unpause resumes a paused sandbox.
func (s *Sandbox) Unpause(ctx context.Context) error {
	return s.transport.Unpause(ctx, s.id, s.provider)
}

// Destroy removes the sandbox.
func (s *Sandbox) Destroy(ctx context.Context) error {
	_, err := s.transport.Destroy(ctx, []string{s.id}, s.provider, false)
	return err
}

// Close is an alias for Destroy.
func (s *Sandbox) Close(ctx context.Context) error {
	return s.Destroy(ctx)
}

// CopyTo copies a file or directory from the host into the sandbox.
func (s *Sandbox) CopyTo(ctx context.Context, hostPath, sandboxPath string) error {
	return s.transport.CopyTo(ctx, s.id, hostPath, sandboxPath, s.provider)
}

// CopyFrom copies a file or directory from the sandbox to the host.
func (s *Sandbox) CopyFrom(ctx context.Context, sandboxPath, hostPath string) error {
	return s.transport.CopyFrom(ctx, s.id, sandboxPath, hostPath, s.provider)
}
