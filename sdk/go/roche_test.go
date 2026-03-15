// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"context"
	"testing"
)

// mockTransport is a configurable mock implementing Transport for tests.
type mockTransport struct {
	createFn     func(ctx context.Context, cfg SandboxConfig, provider string) (string, error)
	execFn       func(ctx context.Context, sandboxID string, command []string, provider string, timeoutSecs *uint64) (*ExecOutput, error)
	destroyFn    func(ctx context.Context, sandboxIDs []string, provider string, all bool) ([]string, error)
	listFn       func(ctx context.Context, provider string) ([]SandboxInfo, error)
	pauseFn      func(ctx context.Context, sandboxID, provider string) error
	unpauseFn    func(ctx context.Context, sandboxID, provider string) error
	gcFn         func(ctx context.Context, provider string, dryRun, all bool) ([]string, error)
	copyToFn     func(ctx context.Context, sandboxID, hostPath, sandboxPath, provider string) error
	copyFromFn   func(ctx context.Context, sandboxID, sandboxPath, hostPath, provider string) error
	poolStatusFn func(ctx context.Context) ([]PoolInfo, error)
	poolWarmupFn func(ctx context.Context, pools []PoolConfig) error
	poolDrainFn  func(ctx context.Context, provider, image string) error
	closeFn      func() error
}

func (m *mockTransport) Create(ctx context.Context, cfg SandboxConfig, provider string) (string, error) {
	if m.createFn != nil {
		return m.createFn(ctx, cfg, provider)
	}
	return "", nil
}

func (m *mockTransport) Exec(ctx context.Context, sandboxID string, command []string, provider string, timeoutSecs *uint64) (*ExecOutput, error) {
	if m.execFn != nil {
		return m.execFn(ctx, sandboxID, command, provider, timeoutSecs)
	}
	return &ExecOutput{}, nil
}

func (m *mockTransport) Destroy(ctx context.Context, sandboxIDs []string, provider string, all bool) ([]string, error) {
	if m.destroyFn != nil {
		return m.destroyFn(ctx, sandboxIDs, provider, all)
	}
	return sandboxIDs, nil
}

func (m *mockTransport) List(ctx context.Context, provider string) ([]SandboxInfo, error) {
	if m.listFn != nil {
		return m.listFn(ctx, provider)
	}
	return nil, nil
}

func (m *mockTransport) Pause(ctx context.Context, sandboxID, provider string) error {
	if m.pauseFn != nil {
		return m.pauseFn(ctx, sandboxID, provider)
	}
	return nil
}

func (m *mockTransport) Unpause(ctx context.Context, sandboxID, provider string) error {
	if m.unpauseFn != nil {
		return m.unpauseFn(ctx, sandboxID, provider)
	}
	return nil
}

func (m *mockTransport) GC(ctx context.Context, provider string, dryRun, all bool) ([]string, error) {
	if m.gcFn != nil {
		return m.gcFn(ctx, provider, dryRun, all)
	}
	return nil, nil
}

func (m *mockTransport) CopyTo(ctx context.Context, sandboxID, hostPath, sandboxPath, provider string) error {
	if m.copyToFn != nil {
		return m.copyToFn(ctx, sandboxID, hostPath, sandboxPath, provider)
	}
	return nil
}

func (m *mockTransport) CopyFrom(ctx context.Context, sandboxID, sandboxPath, hostPath, provider string) error {
	if m.copyFromFn != nil {
		return m.copyFromFn(ctx, sandboxID, sandboxPath, hostPath, provider)
	}
	return nil
}

func (m *mockTransport) PoolStatus(ctx context.Context) ([]PoolInfo, error) {
	if m.poolStatusFn != nil {
		return m.poolStatusFn(ctx)
	}
	return nil, nil
}

func (m *mockTransport) PoolWarmup(ctx context.Context, pools []PoolConfig) error {
	if m.poolWarmupFn != nil {
		return m.poolWarmupFn(ctx, pools)
	}
	return nil
}

func (m *mockTransport) PoolDrain(ctx context.Context, provider, image string) error {
	if m.poolDrainFn != nil {
		return m.poolDrainFn(ctx, provider, image)
	}
	return nil
}

func (m *mockTransport) Close() error {
	if m.closeFn != nil {
		return m.closeFn()
	}
	return nil
}

// Compile-time check.
var _ Transport = (*mockTransport)(nil)

func TestNewWithTransport(t *testing.T) {
	mt := &mockTransport{}
	client, err := New(WithTransport(mt))
	if err != nil {
		t.Fatalf("New() error: %v", err)
	}
	if client == nil {
		t.Fatal("expected non-nil client")
	}
	if client.transport != mt {
		t.Fatal("expected client to use the provided transport")
	}
}

func TestNewDefaultProvider(t *testing.T) {
	client, err := New(WithTransport(&mockTransport{}))
	if err != nil {
		t.Fatalf("New() error: %v", err)
	}
	if client.provider != "docker" {
		t.Fatalf("expected default provider 'docker', got %q", client.provider)
	}
}

func TestNewWithProvider(t *testing.T) {
	client, err := New(WithTransport(&mockTransport{}), WithProvider("k8s"))
	if err != nil {
		t.Fatalf("New() error: %v", err)
	}
	if client.provider != "k8s" {
		t.Fatalf("expected provider 'k8s', got %q", client.provider)
	}
}

func TestClientCreate(t *testing.T) {
	mt := &mockTransport{
		createFn: func(_ context.Context, _ SandboxConfig, _ string) (string, error) {
			return "sandbox-123", nil
		},
	}
	client, _ := New(WithTransport(mt))

	sb, err := client.Create(context.Background(), SandboxConfig{})
	if err != nil {
		t.Fatalf("Create() error: %v", err)
	}
	if sb.ID() != "sandbox-123" {
		t.Fatalf("expected sandbox ID 'sandbox-123', got %q", sb.ID())
	}
}

func TestClientExec(t *testing.T) {
	expected := &ExecOutput{ExitCode: 0, Stdout: "hello", Stderr: ""}
	mt := &mockTransport{
		execFn: func(_ context.Context, _ string, _ []string, _ string, _ *uint64) (*ExecOutput, error) {
			return expected, nil
		},
	}
	client, _ := New(WithTransport(mt))

	out, err := client.Exec(context.Background(), "sandbox-123", []string{"echo", "hello"})
	if err != nil {
		t.Fatalf("Exec() error: %v", err)
	}
	if out.Stdout != "hello" {
		t.Fatalf("expected stdout 'hello', got %q", out.Stdout)
	}
	if out.ExitCode != 0 {
		t.Fatalf("expected exit code 0, got %d", out.ExitCode)
	}
}

func TestClientDestroy(t *testing.T) {
	var gotIDs []string
	mt := &mockTransport{
		destroyFn: func(_ context.Context, ids []string, _ string, _ bool) ([]string, error) {
			gotIDs = ids
			return ids, nil
		},
	}
	client, _ := New(WithTransport(mt))

	err := client.Destroy(context.Background(), "sandbox-123")
	if err != nil {
		t.Fatalf("Destroy() error: %v", err)
	}
	if len(gotIDs) != 1 || gotIDs[0] != "sandbox-123" {
		t.Fatalf("expected destroy called with [sandbox-123], got %v", gotIDs)
	}
}

func TestClientDestroyMany(t *testing.T) {
	mt := &mockTransport{
		destroyFn: func(_ context.Context, ids []string, _ string, _ bool) ([]string, error) {
			return ids, nil
		},
	}
	client, _ := New(WithTransport(mt))

	ids := []string{"sb-1", "sb-2", "sb-3"}
	destroyed, err := client.DestroyMany(context.Background(), ids)
	if err != nil {
		t.Fatalf("DestroyMany() error: %v", err)
	}
	if len(destroyed) != 3 {
		t.Fatalf("expected 3 destroyed IDs, got %d", len(destroyed))
	}
}

func TestClientList(t *testing.T) {
	expected := []SandboxInfo{
		{ID: "sb-1", Status: StatusRunning, Provider: "docker"},
		{ID: "sb-2", Status: StatusPaused, Provider: "docker"},
	}
	mt := &mockTransport{
		listFn: func(_ context.Context, _ string) ([]SandboxInfo, error) {
			return expected, nil
		},
	}
	client, _ := New(WithTransport(mt))

	infos, err := client.List(context.Background())
	if err != nil {
		t.Fatalf("List() error: %v", err)
	}
	if len(infos) != 2 {
		t.Fatalf("expected 2 infos, got %d", len(infos))
	}
	if infos[0].ID != "sb-1" {
		t.Fatalf("expected first ID 'sb-1', got %q", infos[0].ID)
	}
}

func TestClientGC(t *testing.T) {
	var gotDryRun bool
	mt := &mockTransport{
		gcFn: func(_ context.Context, _ string, dryRun, _ bool) ([]string, error) {
			gotDryRun = dryRun
			return []string{"sb-old"}, nil
		},
	}
	client, _ := New(WithTransport(mt))

	ids, err := client.GC(context.Background(), GCOptions{DryRun: true})
	if err != nil {
		t.Fatalf("GC() error: %v", err)
	}
	if !gotDryRun {
		t.Fatal("expected dryRun=true to be passed through")
	}
	if len(ids) != 1 || ids[0] != "sb-old" {
		t.Fatalf("expected [sb-old], got %v", ids)
	}
}

func TestClientClose(t *testing.T) {
	mt := &mockTransport{}
	client, _ := New(WithTransport(mt))

	err := client.Close()
	if err != nil {
		t.Fatalf("Close() error: %v", err)
	}
}
