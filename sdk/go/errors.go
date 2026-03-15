package roche

import "errors"

var (
	// ErrNotFound indicates the requested sandbox does not exist.
	ErrNotFound = errors.New("roche: sandbox not found")
	// ErrPaused indicates the sandbox is paused and cannot execute commands.
	ErrPaused = errors.New("roche: sandbox is paused")
	// ErrUnavailable indicates the provider backend is not reachable.
	ErrUnavailable = errors.New("roche: provider unavailable")
	// ErrTimeout indicates the operation exceeded its deadline.
	ErrTimeout = errors.New("roche: operation timed out")
	// ErrUnsupported indicates the operation is not supported by the provider.
	ErrUnsupported = errors.New("roche: operation not supported")
)
