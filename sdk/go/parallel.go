// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"context"
	"sync"

	"golang.org/x/sync/errgroup"
)

// RunTask describes a single code execution task for RunParallel.
type RunTask struct {
	// Code to execute.
	Code string
	// Language hint: "python", "node", "bash", "auto". Default: "auto".
	Language string
	// Maximum execution time in seconds. Default: 30.
	TimeoutSecs uint64
	// Allow network access. Default: false.
	Network bool
	// Restrict network to these hosts (requires Network=true).
	NetworkAllowlist []string
	// Allow filesystem writes. Default: false.
	Writable bool
	// Writable filesystem paths.
	FSPaths []string
	// Memory limit (e.g., "256m").
	Memory string
	// Trace level. Default: "summary".
	TraceLevel TraceLevel
}

// ParallelOptions configures parallel execution behavior.
type ParallelOptions struct {
	// MaxConcurrency limits the number of tasks running at the same time.
	// Default: 5.
	MaxConcurrency int
}

// ParallelResult aggregates the results of a RunParallel call.
type ParallelResult struct {
	// Results holds the output for each task, in the same order as the input tasks.
	// A nil entry means the corresponding task failed (check Errors).
	Results []*ExecOutput
	// Errors holds the error for each task, in the same order as the input tasks.
	// A nil entry means the corresponding task succeeded.
	Errors []error
	// TotalSucceeded is the number of tasks that completed without error.
	TotalSucceeded int
	// TotalFailed is the number of tasks that returned an error.
	TotalFailed int
}

// RunParallel executes multiple code tasks concurrently, returning all results.
// Concurrency is controlled via ParallelOptions.MaxConcurrency (default 5).
//
//	result := roche.RunParallel(ctx, []roche.RunTask{
//	    {Code: "print(1+1)", Language: "python"},
//	    {Code: "console.log(2+2)", Language: "node"},
//	})
//	fmt.Println(result.TotalSucceeded) // 2
func RunParallel(ctx context.Context, tasks []RunTask, opts ...ParallelOptions) *ParallelResult {
	var o ParallelOptions
	if len(opts) > 0 {
		o = opts[0]
	}
	if o.MaxConcurrency <= 0 {
		o.MaxConcurrency = 5
	}

	n := len(tasks)
	pr := &ParallelResult{
		Results: make([]*ExecOutput, n),
		Errors:  make([]error, n),
	}
	if n == 0 {
		return pr
	}

	// Semaphore channel to limit concurrency.
	sem := make(chan struct{}, o.MaxConcurrency)

	var mu sync.Mutex
	g, gctx := errgroup.WithContext(ctx)

	for i, task := range tasks {
		i, task := i, task // capture loop vars
		g.Go(func() error {
			// Acquire semaphore slot.
			select {
			case sem <- struct{}{}:
			case <-gctx.Done():
				mu.Lock()
				pr.Errors[i] = gctx.Err()
				pr.TotalFailed++
				mu.Unlock()
				return nil
			}
			defer func() { <-sem }()

			out, err := Run(gctx, task.Code, RunOptions{
				Language:         task.Language,
				TimeoutSecs:      task.TimeoutSecs,
				Network:          task.Network,
				NetworkAllowlist: task.NetworkAllowlist,
				Writable:         task.Writable,
				FSPaths:          task.FSPaths,
				Memory:           task.Memory,
				TraceLevel:       task.TraceLevel,
			})

			mu.Lock()
			pr.Results[i] = out
			pr.Errors[i] = err
			if err != nil {
				pr.TotalFailed++
			} else {
				pr.TotalSucceeded++
			}
			mu.Unlock()

			return nil // never propagate; we collect per-task errors
		})
	}

	// errgroup goroutines never return non-nil, so this always returns nil.
	_ = g.Wait()

	return pr
}
