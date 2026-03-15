// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"errors"
	"fmt"
	"testing"
)

func TestSentinelErrors(t *testing.T) {
	sentinels := []error{ErrNotFound, ErrPaused, ErrUnavailable, ErrTimeout, ErrUnsupported}
	for _, err := range sentinels {
		if err == nil {
			t.Error("sentinel error should not be nil")
		}
		if err.Error() == "" {
			t.Error("sentinel error should have a message")
		}
	}
}

func TestErrorsIsWrapped(t *testing.T) {
	wrapped := fmt.Errorf("exec failed: %w", ErrNotFound)
	if !errors.Is(wrapped, ErrNotFound) {
		t.Error("wrapped error should match ErrNotFound via errors.Is")
	}
}

func TestErrorsAreDistinct(t *testing.T) {
	if errors.Is(ErrNotFound, ErrPaused) {
		t.Error("ErrNotFound should not match ErrPaused")
	}
}
