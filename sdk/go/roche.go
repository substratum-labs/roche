package roche

import "context"

// Client is the main entry point for the Roche Go SDK.
// It manages transport selection and delegates operations to the underlying Transport.
type Client struct {
	transport Transport
	provider  string
}

// clientConfig holds the resolved configuration for building a Client.
type clientConfig struct {
	transport  Transport
	binary     string
	daemonPort int
	provider   string
	directMode bool
}

// Option configures the Client constructor.
type Option func(*clientConfig)

// WithTransport injects a pre-built Transport, bypassing auto-detection.
func WithTransport(t Transport) Option {
	return func(c *clientConfig) {
		c.transport = t
	}
}

// WithBinary overrides the CLI binary path (default: "roche").
func WithBinary(path string) Option {
	return func(c *clientConfig) {
		c.binary = path
	}
}

// WithDaemonPort overrides the daemon gRPC port for auto-detection.
func WithDaemonPort(port int) Option {
	return func(c *clientConfig) {
		c.daemonPort = port
	}
}

// WithProvider sets the default sandbox provider (e.g. "docker", "k8s").
func WithProvider(provider string) Option {
	return func(c *clientConfig) {
		c.provider = provider
	}
}

// WithDirectMode forces CLI transport, skipping daemon detection.
func WithDirectMode() Option {
	return func(c *clientConfig) {
		c.directMode = true
	}
}

// New creates a new Client with the given options.
// Transport resolution order:
//  1. Explicit transport via WithTransport
//  2. Direct CLI mode via WithDirectMode
//  3. Auto-detect daemon via detectDaemon → gRPC
//  4. Fallback to CLI transport
func New(opts ...Option) (*Client, error) {
	cfg := &clientConfig{
		provider: defaultProvider,
		binary:   defaultBinary,
	}
	for _, o := range opts {
		o(cfg)
	}

	var t Transport

	switch {
	case cfg.transport != nil:
		t = cfg.transport
	case cfg.directMode:
		t = NewCLITransport(cfg.binary)
	default:
		if cfg.daemonPort > 0 {
			t = newGRPCTransport(cfg.daemonPort)
		} else if info, err := detectDaemon(); err == nil && info != nil {
			t = newGRPCTransport(info.Port)
		} else {
			t = NewCLITransport(cfg.binary)
		}
	}

	return &Client{
		transport: t,
		provider:  cfg.provider,
	}, nil
}

// Create creates a new sandbox and returns a Sandbox handle.
func (c *Client) Create(ctx context.Context, cfg SandboxConfig) (*Sandbox, error) {
	cfg = applyDefaults(cfg, c.provider)
	id, err := c.transport.Create(ctx, cfg, cfg.Provider)
	if err != nil {
		return nil, err
	}
	return &Sandbox{
		id:        id,
		provider:  cfg.Provider,
		transport: c.transport,
	}, nil
}

// CreateID creates a new sandbox and returns only its ID.
func (c *Client) CreateID(ctx context.Context, cfg SandboxConfig) (string, error) {
	cfg = applyDefaults(cfg, c.provider)
	return c.transport.Create(ctx, cfg, cfg.Provider)
}

// Exec runs a command in the specified sandbox.
func (c *Client) Exec(ctx context.Context, sandboxID string, command []string) (*ExecOutput, error) {
	return c.transport.Exec(ctx, sandboxID, command, c.provider, nil)
}

// Destroy removes a single sandbox by ID.
func (c *Client) Destroy(ctx context.Context, sandboxID string) error {
	_, err := c.transport.Destroy(ctx, []string{sandboxID}, c.provider, false)
	return err
}

// DestroyMany removes multiple sandboxes by ID and returns the destroyed IDs.
func (c *Client) DestroyMany(ctx context.Context, sandboxIDs []string) ([]string, error) {
	return c.transport.Destroy(ctx, sandboxIDs, c.provider, false)
}

// List returns all active sandboxes.
func (c *Client) List(ctx context.Context) ([]SandboxInfo, error) {
	return c.transport.List(ctx, c.provider)
}

// GC triggers garbage collection and returns the list of destroyed sandbox IDs.
func (c *Client) GC(ctx context.Context, opts GCOptions) ([]string, error) {
	return c.transport.GC(ctx, c.provider, opts.DryRun, opts.All)
}

// PoolStatus returns the status of all sandbox pools.
func (c *Client) PoolStatus(ctx context.Context) ([]PoolInfo, error) {
	return c.transport.PoolStatus(ctx)
}

// PoolWarmup pre-creates sandboxes according to the given pool configs.
func (c *Client) PoolWarmup(ctx context.Context, pools []PoolConfig) error {
	return c.transport.PoolWarmup(ctx, pools)
}

// PoolDrain drains all idle sandboxes for the given provider and image.
func (c *Client) PoolDrain(ctx context.Context, provider, image string) error {
	return c.transport.PoolDrain(ctx, provider, image)
}

// Close releases resources held by the client's transport.
func (c *Client) Close() error {
	return c.transport.Close()
}
