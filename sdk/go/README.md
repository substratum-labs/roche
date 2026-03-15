# Roche Go SDK

Go client library for [Roche](https://github.com/roche-dev/roche), a universal sandbox orchestrator for AI agents.

## Installation

```bash
go get github.com/substratum-labs/roche-go
```

## Quick Start

```go
package main

import (
	"context"
	"fmt"
	"log"

	roche "github.com/substratum-labs/roche-go"
)

func main() {
	ctx := context.Background()

	client, err := roche.New()
	if err != nil {
		log.Fatal(err)
	}
	defer client.Close()

	sandbox, err := client.Create(ctx, roche.SandboxConfig{
		Image: "python:3.12-slim",
	})
	if err != nil {
		log.Fatal(err)
	}
	defer sandbox.Close(ctx)

	out, err := sandbox.Exec(ctx, []string{"echo", "hello from sandbox"})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println(out.Stdout)
}
```

## AI-Safe Defaults

| Setting | Default | Description |
|---|---|---|
| Network | `false` | Network access disabled |
| Writable | `false` | Filesystem is read-only |
| Timeout | `300s` | Sandbox auto-expires after 5 minutes |

## Error Handling

```go
import "errors"

out, err := sandbox.Exec(ctx, []string{"ls"})
if errors.Is(err, roche.ErrNotFound) {
    // sandbox was destroyed
} else if errors.Is(err, roche.ErrTimeout) {
    // operation timed out
}
```

## Transport

The SDK auto-detects a running Roche daemon (via `~/.roche/daemon.json`) and uses gRPC. If no daemon is found, it falls back to the `roche` CLI binary.

```go
// Force CLI transport
client, _ := roche.New(roche.WithDirectMode())

// Custom binary path
client, _ := roche.New(roche.WithBinary("/usr/local/bin/roche"))

// Explicit daemon port
client, _ := roche.New(roche.WithDaemonPort(50051))
```
