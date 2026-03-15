package roche

import "testing"

func TestApplyDefaultsZeroValue(t *testing.T) {
	cfg := SandboxConfig{}
	out := applyDefaults(cfg, "docker")
	if out.Provider != "docker" {
		t.Errorf("expected Provider=docker, got %q", out.Provider)
	}
	if out.Image != "python:3.12-slim" {
		t.Errorf("expected default Image, got %q", out.Image)
	}
	if out.TimeoutSecs != 300 {
		t.Errorf("expected TimeoutSecs=300, got %d", out.TimeoutSecs)
	}
}

func TestApplyDefaultsExplicitValues(t *testing.T) {
	cfg := SandboxConfig{
		Provider:    "k8s",
		Image:       "node:20",
		TimeoutSecs: 60,
		Network:     true,
	}
	out := applyDefaults(cfg, "docker")
	if out.Provider != "k8s" {
		t.Errorf("expected Provider=k8s, got %q", out.Provider)
	}
	if out.Image != "node:20" {
		t.Errorf("expected Image=node:20, got %q", out.Image)
	}
	if out.TimeoutSecs != 60 {
		t.Errorf("expected TimeoutSecs=60, got %d", out.TimeoutSecs)
	}
	if !out.Network {
		t.Error("expected Network=true")
	}
}

func TestApplyDefaultsEmptyProvider(t *testing.T) {
	cfg := SandboxConfig{Image: "alpine"}
	out := applyDefaults(cfg, "k8s")
	if out.Provider != "k8s" {
		t.Errorf("expected fallback to client provider k8s, got %q", out.Provider)
	}
}
