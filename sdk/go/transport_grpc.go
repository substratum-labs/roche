// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"context"
	"fmt"
	"sync"

	pb "github.com/substratum-labs/roche-go/gen/roche/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"
)

// grpcTransport implements Transport via gRPC to the roche-daemon.
type grpcTransport struct {
	port   int
	mu     sync.Mutex
	conn   *grpc.ClientConn
	client pb.SandboxServiceClient
}

// newGRPCTransport creates a gRPC transport targeting the given port.
// The connection is lazily established on first use.
func newGRPCTransport(port int) *grpcTransport {
	return &grpcTransport{port: port}
}

var _ Transport = (*grpcTransport)(nil)

// getClient returns the gRPC client, creating the connection on first call.
func (g *grpcTransport) getClient() (pb.SandboxServiceClient, error) {
	g.mu.Lock()
	defer g.mu.Unlock()

	if g.client != nil {
		return g.client, nil
	}

	target := fmt.Sprintf("localhost:%d", g.port)
	conn, err := grpc.NewClient(target, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, fmt.Errorf("roche grpc: failed to connect: %w", err)
	}
	g.conn = conn
	g.client = pb.NewSandboxServiceClient(conn)
	return g.client, nil
}

func (g *grpcTransport) Create(ctx context.Context, cfg SandboxConfig, provider string) (string, error) {
	client, err := g.getClient()
	if err != nil {
		return "", err
	}

	req := &pb.CreateRequest{
		Provider:    provider,
		Image:       cfg.Image,
		TimeoutSecs: cfg.TimeoutSecs,
		Network:     cfg.Network,
		Writable:    cfg.Writable,
		Env:         cfg.Env,
	}
	if cfg.Memory != "" {
		req.Memory = &cfg.Memory
	}
	if cfg.CPUs > 0 {
		req.Cpus = &cfg.CPUs
	}
	if cfg.Kernel != "" {
		req.Kernel = &cfg.Kernel
	}
	if cfg.Rootfs != "" {
		req.Rootfs = &cfg.Rootfs
	}
	for _, m := range cfg.Mounts {
		req.Mounts = append(req.Mounts, &pb.MountConfig{
			HostPath:      m.HostPath,
			ContainerPath: m.ContainerPath,
			Readonly:      m.Readonly,
		})
	}

	resp, err := client.Create(ctx, req)
	if err != nil {
		return "", mapGRPCError(err)
	}
	return resp.GetSandboxId(), nil
}

func (g *grpcTransport) Exec(ctx context.Context, sandboxID string, command []string, provider string, timeoutSecs *uint64) (*ExecOutput, error) {
	client, err := g.getClient()
	if err != nil {
		return nil, err
	}

	req := &pb.ExecRequest{
		SandboxId:   sandboxID,
		Command:     command,
		Provider:    provider,
	}
	if timeoutSecs != nil {
		req.TimeoutSecs = timeoutSecs
	}

	resp, err := client.Exec(ctx, req)
	if err != nil {
		return nil, mapGRPCError(err)
	}
	return &ExecOutput{
		ExitCode: resp.GetExitCode(),
		Stdout:   resp.GetStdout(),
		Stderr:   resp.GetStderr(),
	}, nil
}

func (g *grpcTransport) Destroy(ctx context.Context, sandboxIDs []string, provider string, all bool) ([]string, error) {
	client, err := g.getClient()
	if err != nil {
		return nil, err
	}

	resp, err := client.Destroy(ctx, &pb.DestroyRequest{
		SandboxIds: sandboxIDs,
		All:        all,
		Provider:   provider,
	})
	if err != nil {
		return nil, mapGRPCError(err)
	}
	return resp.GetDestroyedIds(), nil
}

func (g *grpcTransport) List(ctx context.Context, provider string) ([]SandboxInfo, error) {
	client, err := g.getClient()
	if err != nil {
		return nil, err
	}

	resp, err := client.List(ctx, &pb.ListRequest{
		Provider: provider,
	})
	if err != nil {
		return nil, mapGRPCError(err)
	}

	var infos []SandboxInfo
	for _, s := range resp.GetSandboxes() {
		info := SandboxInfo{
			ID:       s.GetId(),
			Status:   mapProtoStatus(s.GetStatus()),
			Provider: s.GetProvider(),
			Image:    s.GetImage(),
		}
		if s.ExpiresAt != nil {
			ea := s.GetExpiresAt()
			info.ExpiresAt = &ea
		}
		infos = append(infos, info)
	}
	return infos, nil
}

func (g *grpcTransport) Pause(ctx context.Context, sandboxID, provider string) error {
	client, err := g.getClient()
	if err != nil {
		return err
	}

	_, err = client.Pause(ctx, &pb.PauseRequest{
		SandboxId: sandboxID,
		Provider:  provider,
	})
	if err != nil {
		return mapGRPCError(err)
	}
	return nil
}

func (g *grpcTransport) Unpause(ctx context.Context, sandboxID, provider string) error {
	client, err := g.getClient()
	if err != nil {
		return err
	}

	_, err = client.Unpause(ctx, &pb.UnpauseRequest{
		SandboxId: sandboxID,
		Provider:  provider,
	})
	if err != nil {
		return mapGRPCError(err)
	}
	return nil
}

func (g *grpcTransport) GC(ctx context.Context, provider string, dryRun, all bool) ([]string, error) {
	client, err := g.getClient()
	if err != nil {
		return nil, err
	}

	resp, err := client.Gc(ctx, &pb.GcRequest{
		DryRun:   dryRun,
		All:      all,
		Provider: provider,
	})
	if err != nil {
		return nil, mapGRPCError(err)
	}
	return resp.GetDestroyedIds(), nil
}

func (g *grpcTransport) CopyTo(ctx context.Context, sandboxID, hostPath, sandboxPath, provider string) error {
	client, err := g.getClient()
	if err != nil {
		return err
	}

	_, err = client.CopyTo(ctx, &pb.CopyToRequest{
		SandboxId:   sandboxID,
		HostPath:    hostPath,
		SandboxPath: sandboxPath,
		Provider:    provider,
	})
	if err != nil {
		return mapGRPCError(err)
	}
	return nil
}

func (g *grpcTransport) CopyFrom(ctx context.Context, sandboxID, sandboxPath, hostPath, provider string) error {
	client, err := g.getClient()
	if err != nil {
		return err
	}

	_, err = client.CopyFrom(ctx, &pb.CopyFromRequest{
		SandboxId:   sandboxID,
		SandboxPath: sandboxPath,
		HostPath:    hostPath,
		Provider:    provider,
	})
	if err != nil {
		return mapGRPCError(err)
	}
	return nil
}

func (g *grpcTransport) PoolStatus(ctx context.Context) ([]PoolInfo, error) {
	client, err := g.getClient()
	if err != nil {
		return nil, err
	}

	resp, err := client.PoolStatus(ctx, &pb.PoolStatusRequest{})
	if err != nil {
		return nil, mapGRPCError(err)
	}

	var pools []PoolInfo
	for _, p := range resp.GetPools() {
		pools = append(pools, PoolInfo{
			Provider:    p.GetProvider(),
			Image:       p.GetImage(),
			IdleCount:   p.GetIdleCount(),
			ActiveCount: p.GetActiveCount(),
			MaxIdle:     p.GetMaxIdle(),
			MaxTotal:    p.GetMaxTotal(),
		})
	}
	return pools, nil
}

func (g *grpcTransport) PoolWarmup(ctx context.Context, pools []PoolConfig) error {
	client, err := g.getClient()
	if err != nil {
		return err
	}

	_, err = client.PoolWarmup(ctx, &pb.PoolWarmupRequest{})
	if err != nil {
		return mapGRPCError(err)
	}
	return nil
}

func (g *grpcTransport) PoolDrain(ctx context.Context, provider, image string) error {
	client, err := g.getClient()
	if err != nil {
		return err
	}

	_, err = client.PoolDrain(ctx, &pb.PoolDrainRequest{})
	if err != nil {
		return mapGRPCError(err)
	}
	return nil
}

// Close closes the underlying gRPC connection.
func (g *grpcTransport) Close() error {
	g.mu.Lock()
	defer g.mu.Unlock()

	if g.conn != nil {
		err := g.conn.Close()
		g.conn = nil
		g.client = nil
		return err
	}
	return nil
}

// mapGRPCError converts a gRPC status error to the appropriate sentinel error.
// Returns nil if err is nil.
func mapGRPCError(err error) error {
	if err == nil {
		return nil
	}

	st, ok := status.FromError(err)
	if !ok {
		return err
	}

	msg := st.Message()
	switch st.Code() {
	case codes.NotFound:
		return fmt.Errorf("roche grpc: %s: %w", msg, ErrNotFound)
	case codes.FailedPrecondition:
		return fmt.Errorf("roche grpc: %s: %w", msg, ErrPaused)
	case codes.Unavailable:
		return fmt.Errorf("roche grpc: %s: %w", msg, ErrUnavailable)
	case codes.DeadlineExceeded:
		return fmt.Errorf("roche grpc: %s: %w", msg, ErrTimeout)
	case codes.Unimplemented:
		return fmt.Errorf("roche grpc: %s: %w", msg, ErrUnsupported)
	default:
		return fmt.Errorf("roche grpc: %s (code=%s)", msg, st.Code())
	}
}

// mapProtoStatus converts a proto SandboxStatus enum to the Go SandboxStatus string.
func mapProtoStatus(s pb.SandboxStatus) SandboxStatus {
	switch s {
	case pb.SandboxStatus_SANDBOX_STATUS_RUNNING:
		return StatusRunning
	case pb.SandboxStatus_SANDBOX_STATUS_PAUSED:
		return StatusPaused
	case pb.SandboxStatus_SANDBOX_STATUS_STOPPED:
		return StatusStopped
	case pb.SandboxStatus_SANDBOX_STATUS_FAILED:
		return StatusFailed
	default:
		return StatusFailed
	}
}
