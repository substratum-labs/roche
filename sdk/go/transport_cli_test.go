package roche

import (
	"context"
	"errors"
	"testing"
)

func TestCLITransportCreateArgs(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	cfg := SandboxConfig{
		Image:       "python:3.12",
		TimeoutSecs: 60,
		Network:     true,
		Writable:    true,
		Memory:      "512m",
		CPUs:        2.0,
		Env:         map[string]string{"FOO": "bar", "BAZ": "qux"},
		Mounts: []Mount{
			{HostPath: "/host", ContainerPath: "/container", Readonly: true},
			{HostPath: "/data", ContainerPath: "/mnt", Readonly: false},
		},
	}
	args := tr.buildCreateArgs(cfg, "docker")
	// Check required flags
	assertContainsSeq(t, args, "--provider", "docker")
	assertContainsSeq(t, args, "--image", "python:3.12")
	assertContainsSeq(t, args, "--timeout", "60")
	assertContains(t, args, "--network")
	assertContains(t, args, "--writable")
	assertContainsSeq(t, args, "--memory", "512m")
	assertContainsSeq(t, args, "--cpus", "2")
	// Env flags
	assertContainsSeq(t, args, "--env", "FOO=bar")
	assertContainsSeq(t, args, "--env", "BAZ=qux")
	// Mount flags
	assertContainsAny(t, args, "--mount", "/host:/container:ro")
	assertContainsAny(t, args, "--mount", "/data:/mnt:rw")
}

func TestCLITransportExecArgs(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	timeout := uint64(30)
	args := tr.buildExecArgs("abc123", []string{"echo", "hello"}, "docker", &timeout)
	assertContainsSeq(t, args, "--sandbox", "abc123")
	assertContainsSeq(t, args, "--provider", "docker")
	assertContainsSeq(t, args, "--timeout", "30")
	assertContains(t, args, "--")
	// command should be after --
	dashIdx := indexOf(args, "--")
	if dashIdx < 0 || dashIdx+2 >= len(args) {
		t.Fatal("expected command after --")
	}
	if args[dashIdx+1] != "echo" || args[dashIdx+2] != "hello" {
		t.Fatalf("unexpected command: %v", args[dashIdx+1:])
	}
}

func TestCLITransportExecArgsNoTimeout(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	args := tr.buildExecArgs("abc123", []string{"ls"}, "docker", nil)
	assertContainsSeq(t, args, "--sandbox", "abc123")
	assertNotContains(t, args, "--timeout")
}

func TestCLITransportDestroyArgs(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	args := tr.buildDestroyArgs([]string{"id1", "id2"}, "docker", false)
	assertContainsSeq(t, args, "--provider", "docker")
	assertContains(t, args, "id1")
	assertContains(t, args, "id2")
	assertNotContains(t, args, "--all")
}

func TestCLITransportDestroyAllArgs(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	args := tr.buildDestroyArgs(nil, "docker", true)
	assertContainsSeq(t, args, "--provider", "docker")
	assertContains(t, args, "--all")
}

func TestCLITransportListArgs(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	args := tr.buildListArgs("docker")
	assertContainsSeq(t, args, "--provider", "docker")
	assertContains(t, args, "--json")
}

func TestCLITransportCopyToArgs(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	args := tr.buildCopyToArgs("abc123", "/local/file.txt", "/sandbox/file.txt", "docker")
	assertContainsSeq(t, args, "--provider", "docker")
	assertContains(t, args, "/local/file.txt")
	assertContains(t, args, "abc123:/sandbox/file.txt")
}

func TestCLITransportCopyFromArgs(t *testing.T) {
	tr := &CLITransport{Binary: "roche"}
	args := tr.buildCopyFromArgs("abc123", "/sandbox/file.txt", "/local/file.txt", "docker")
	assertContainsSeq(t, args, "--provider", "docker")
	assertContains(t, args, "abc123:/sandbox/file.txt")
	assertContains(t, args, "/local/file.txt")
}

func TestMapCLIError(t *testing.T) {
	tests := []struct {
		name   string
		stderr string
		want   error
	}{
		{"not found", "Error: sandbox not found", ErrNotFound},
		{"paused", "Error: sandbox is paused", ErrPaused},
		{"unavailable", "Error: provider unavailable", ErrUnavailable},
		{"connection refused", "Error: connection refused", ErrUnavailable},
		{"timeout", "Error: operation timed out", ErrTimeout},
		{"timed out", "Error: request timeout", ErrTimeout},
		{"unsupported", "Error: unsupported operation", ErrUnsupported},
		{"unknown error", "Error: something weird happened", nil},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := mapCLIError(tt.stderr)
			if tt.want == nil {
				if got == nil {
					t.Fatal("expected non-nil error for unknown Error: prefix")
				}
				// Should be a generic error, not a sentinel
				if errors.Is(got, ErrNotFound) || errors.Is(got, ErrPaused) ||
					errors.Is(got, ErrUnavailable) || errors.Is(got, ErrTimeout) ||
					errors.Is(got, ErrUnsupported) {
					t.Fatalf("expected generic error, got sentinel: %v", got)
				}
			} else {
				if !errors.Is(got, tt.want) {
					t.Fatalf("expected %v, got %v", tt.want, got)
				}
			}
		})
	}
}

func TestMapCLIErrorNonRocheError(t *testing.T) {
	got := mapCLIError("some random output without error prefix")
	if got != nil {
		t.Fatalf("expected nil, got %v", got)
	}
}

func TestCLITransportBinaryNotFound(t *testing.T) {
	tr := &CLITransport{Binary: "/nonexistent/binary/roche-does-not-exist"}
	_, err := tr.Create(context.Background(), SandboxConfig{
		Image:       "python:3.12",
		TimeoutSecs: 60,
	}, "docker")
	if err == nil {
		t.Fatal("expected error")
	}
	if !errors.Is(err, ErrUnavailable) {
		t.Fatalf("expected ErrUnavailable, got %v", err)
	}
}

// --- helpers ---

func assertContains(t *testing.T, args []string, val string) {
	t.Helper()
	for _, a := range args {
		if a == val {
			return
		}
	}
	t.Fatalf("args %v does not contain %q", args, val)
}

func assertNotContains(t *testing.T, args []string, val string) {
	t.Helper()
	for _, a := range args {
		if a == val {
			t.Fatalf("args %v should not contain %q", args, val)
		}
	}
}

func assertContainsSeq(t *testing.T, args []string, key, val string) {
	t.Helper()
	for i := 0; i < len(args)-1; i++ {
		if args[i] == key && args[i+1] == val {
			return
		}
	}
	t.Fatalf("args %v does not contain %q %q in sequence", args, key, val)
}

func assertContainsAny(t *testing.T, args []string, key, val string) {
	t.Helper()
	for i := 0; i < len(args)-1; i++ {
		if args[i] == key && args[i+1] == val {
			return
		}
	}
	t.Fatalf("args %v does not contain %q %q", args, key, val)
}

func indexOf(args []string, val string) int {
	for i, a := range args {
		if a == val {
			return i
		}
	}
	return -1
}
