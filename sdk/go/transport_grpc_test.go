package roche

import (
	"errors"
	"testing"

	pb "github.com/roche-dev/roche-go/gen/roche/v1"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

func TestMapGRPCError(t *testing.T) {
	tests := []struct {
		name     string
		code     codes.Code
		sentinel error
	}{
		{"NotFound", codes.NotFound, ErrNotFound},
		{"FailedPrecondition", codes.FailedPrecondition, ErrPaused},
		{"Unavailable", codes.Unavailable, ErrUnavailable},
		{"DeadlineExceeded", codes.DeadlineExceeded, ErrTimeout},
		{"Unimplemented", codes.Unimplemented, ErrUnsupported},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			grpcErr := status.Error(tc.code, "test message")
			mapped := mapGRPCError(grpcErr)
			if mapped == nil {
				t.Fatalf("mapGRPCError returned nil for code %v", tc.code)
			}
			if !errors.Is(mapped, tc.sentinel) {
				t.Errorf("mapGRPCError(%v) = %v, want wrapping %v", tc.code, mapped, tc.sentinel)
			}
		})
	}
}

func TestMapGRPCErrorUnknownCode(t *testing.T) {
	grpcErr := status.Error(codes.Internal, "internal error")
	mapped := mapGRPCError(grpcErr)
	if mapped == nil {
		t.Fatal("expected non-nil error for codes.Internal")
	}
	// Should NOT match any sentinel
	for _, sentinel := range []error{ErrNotFound, ErrPaused, ErrUnavailable, ErrTimeout, ErrUnsupported} {
		if errors.Is(mapped, sentinel) {
			t.Errorf("mapped error should not match sentinel %v", sentinel)
		}
	}
}

func TestMapGRPCErrorNilInput(t *testing.T) {
	mapped := mapGRPCError(nil)
	if mapped != nil {
		t.Errorf("expected nil for nil input, got %v", mapped)
	}
}

func TestMapProtoStatus(t *testing.T) {
	tests := []struct {
		name   string
		proto  pb.SandboxStatus
		expect SandboxStatus
	}{
		{"Running", pb.SandboxStatus_SANDBOX_STATUS_RUNNING, StatusRunning},
		{"Paused", pb.SandboxStatus_SANDBOX_STATUS_PAUSED, StatusPaused},
		{"Stopped", pb.SandboxStatus_SANDBOX_STATUS_STOPPED, StatusStopped},
		{"Failed", pb.SandboxStatus_SANDBOX_STATUS_FAILED, StatusFailed},
		{"Unspecified", pb.SandboxStatus_SANDBOX_STATUS_UNSPECIFIED, StatusFailed},
		{"Unknown99", pb.SandboxStatus(99), StatusFailed},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := mapProtoStatus(tc.proto)
			if got != tc.expect {
				t.Errorf("mapProtoStatus(%v) = %q, want %q", tc.proto, got, tc.expect)
			}
		})
	}
}
