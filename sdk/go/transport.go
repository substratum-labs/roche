package roche

import "context"

// Transport defines the low-level communication protocol with the Roche backend.
type Transport interface {
	Create(ctx context.Context, cfg SandboxConfig, provider string) (string, error)
	Exec(ctx context.Context, sandboxID string, command []string, provider string, timeoutSecs *uint64) (*ExecOutput, error)
	Destroy(ctx context.Context, sandboxIDs []string, provider string, all bool) ([]string, error)
	List(ctx context.Context, provider string) ([]SandboxInfo, error)
	Pause(ctx context.Context, sandboxID, provider string) error
	Unpause(ctx context.Context, sandboxID, provider string) error
	GC(ctx context.Context, provider string, dryRun, all bool) ([]string, error)
	CopyTo(ctx context.Context, sandboxID, hostPath, sandboxPath, provider string) error
	CopyFrom(ctx context.Context, sandboxID, sandboxPath, hostPath, provider string) error
	PoolStatus(ctx context.Context) ([]PoolInfo, error)
	PoolWarmup(ctx context.Context, pools []PoolConfig) error
	PoolDrain(ctx context.Context, provider, image string) error
	Close() error
}
