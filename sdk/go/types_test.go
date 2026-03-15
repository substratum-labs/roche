// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import "testing"

func TestSandboxConfigZeroValue(t *testing.T) {
	cfg := SandboxConfig{}
	if cfg.Provider != "" {
		t.Errorf("expected empty Provider, got %q", cfg.Provider)
	}
	if cfg.Image != "" {
		t.Errorf("expected empty Image, got %q", cfg.Image)
	}
	if cfg.TimeoutSecs != 0 {
		t.Errorf("expected 0 TimeoutSecs, got %d", cfg.TimeoutSecs)
	}
	if cfg.Network {
		t.Error("expected Network=false")
	}
	if cfg.Writable {
		t.Error("expected Writable=false")
	}
}

func TestNewMount(t *testing.T) {
	m := NewMount("/host/data", "/container/data")
	if m.HostPath != "/host/data" {
		t.Errorf("expected HostPath=/host/data, got %q", m.HostPath)
	}
	if m.ContainerPath != "/container/data" {
		t.Errorf("expected ContainerPath=/container/data, got %q", m.ContainerPath)
	}
	if !m.Readonly {
		t.Error("expected Readonly=true (AI-safe default)")
	}
}

func TestSandboxStatusConstants(t *testing.T) {
	tests := []struct {
		status SandboxStatus
		want   string
	}{
		{StatusRunning, "running"},
		{StatusPaused, "paused"},
		{StatusStopped, "stopped"},
		{StatusFailed, "failed"},
	}
	for _, tt := range tests {
		if string(tt.status) != tt.want {
			t.Errorf("got %q, want %q", tt.status, tt.want)
		}
	}
}
