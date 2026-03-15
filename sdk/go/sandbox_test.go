// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"context"
	"testing"
)

func TestSandboxID(t *testing.T) {
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: &mockTransport{}}
	if sb.ID() != "sb-42" {
		t.Fatalf("expected ID 'sb-42', got %q", sb.ID())
	}
}

func TestSandboxProvider(t *testing.T) {
	sb := &Sandbox{id: "sb-42", provider: "k8s", transport: &mockTransport{}}
	if sb.Provider() != "k8s" {
		t.Fatalf("expected provider 'k8s', got %q", sb.Provider())
	}
}

func TestSandboxExec(t *testing.T) {
	expected := &ExecOutput{ExitCode: 0, Stdout: "world", Stderr: ""}
	var gotID string
	var gotCmd []string
	mt := &mockTransport{
		execFn: func(_ context.Context, id string, cmd []string, _ string, _ *uint64) (*ExecOutput, error) {
			gotID = id
			gotCmd = cmd
			return expected, nil
		},
	}
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: mt}

	out, err := sb.Exec(context.Background(), []string{"echo", "world"})
	if err != nil {
		t.Fatalf("Exec() error: %v", err)
	}
	if gotID != "sb-42" {
		t.Fatalf("expected sandbox ID 'sb-42', got %q", gotID)
	}
	if len(gotCmd) != 2 || gotCmd[0] != "echo" || gotCmd[1] != "world" {
		t.Fatalf("expected command [echo world], got %v", gotCmd)
	}
	if out.Stdout != "world" {
		t.Fatalf("expected stdout 'world', got %q", out.Stdout)
	}
}

func TestSandboxPause(t *testing.T) {
	var called bool
	var gotID string
	mt := &mockTransport{
		pauseFn: func(_ context.Context, id, _ string) error {
			called = true
			gotID = id
			return nil
		},
	}
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: mt}

	err := sb.Pause(context.Background())
	if err != nil {
		t.Fatalf("Pause() error: %v", err)
	}
	if !called {
		t.Fatal("expected Pause to be called on transport")
	}
	if gotID != "sb-42" {
		t.Fatalf("expected sandbox ID 'sb-42', got %q", gotID)
	}
}

func TestSandboxUnpause(t *testing.T) {
	var called bool
	var gotID string
	mt := &mockTransport{
		unpauseFn: func(_ context.Context, id, _ string) error {
			called = true
			gotID = id
			return nil
		},
	}
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: mt}

	err := sb.Unpause(context.Background())
	if err != nil {
		t.Fatalf("Unpause() error: %v", err)
	}
	if !called {
		t.Fatal("expected Unpause to be called on transport")
	}
	if gotID != "sb-42" {
		t.Fatalf("expected sandbox ID 'sb-42', got %q", gotID)
	}
}

func TestSandboxDestroy(t *testing.T) {
	var gotIDs []string
	mt := &mockTransport{
		destroyFn: func(_ context.Context, ids []string, _ string, _ bool) ([]string, error) {
			gotIDs = ids
			return ids, nil
		},
	}
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: mt}

	err := sb.Destroy(context.Background())
	if err != nil {
		t.Fatalf("Destroy() error: %v", err)
	}
	if len(gotIDs) != 1 || gotIDs[0] != "sb-42" {
		t.Fatalf("expected destroy with [sb-42], got %v", gotIDs)
	}
}

func TestSandboxClose(t *testing.T) {
	var gotIDs []string
	mt := &mockTransport{
		destroyFn: func(_ context.Context, ids []string, _ string, _ bool) ([]string, error) {
			gotIDs = ids
			return ids, nil
		},
	}
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: mt}

	err := sb.Close(context.Background())
	if err != nil {
		t.Fatalf("Close() error: %v", err)
	}
	if len(gotIDs) != 1 || gotIDs[0] != "sb-42" {
		t.Fatalf("expected Close to call Destroy with [sb-42], got %v", gotIDs)
	}
}

func TestSandboxCopyTo(t *testing.T) {
	var gotID, gotHost, gotSandbox string
	mt := &mockTransport{
		copyToFn: func(_ context.Context, id, hostPath, sandboxPath, _ string) error {
			gotID = id
			gotHost = hostPath
			gotSandbox = sandboxPath
			return nil
		},
	}
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: mt}

	err := sb.CopyTo(context.Background(), "/tmp/local.txt", "/app/remote.txt")
	if err != nil {
		t.Fatalf("CopyTo() error: %v", err)
	}
	if gotID != "sb-42" {
		t.Fatalf("expected sandbox ID 'sb-42', got %q", gotID)
	}
	if gotHost != "/tmp/local.txt" {
		t.Fatalf("expected host path '/tmp/local.txt', got %q", gotHost)
	}
	if gotSandbox != "/app/remote.txt" {
		t.Fatalf("expected sandbox path '/app/remote.txt', got %q", gotSandbox)
	}
}

func TestSandboxCopyFrom(t *testing.T) {
	var gotID, gotSandbox, gotHost string
	mt := &mockTransport{
		copyFromFn: func(_ context.Context, id, sandboxPath, hostPath, _ string) error {
			gotID = id
			gotSandbox = sandboxPath
			gotHost = hostPath
			return nil
		},
	}
	sb := &Sandbox{id: "sb-42", provider: "docker", transport: mt}

	err := sb.CopyFrom(context.Background(), "/app/output.txt", "/tmp/output.txt")
	if err != nil {
		t.Fatalf("CopyFrom() error: %v", err)
	}
	if gotID != "sb-42" {
		t.Fatalf("expected sandbox ID 'sb-42', got %q", gotID)
	}
	if gotSandbox != "/app/output.txt" {
		t.Fatalf("expected sandbox path '/app/output.txt', got %q", gotSandbox)
	}
	if gotHost != "/tmp/output.txt" {
		t.Fatalf("expected host path '/tmp/output.txt', got %q", gotHost)
	}
}
