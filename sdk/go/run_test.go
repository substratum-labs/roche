// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import "testing"

func TestDetectLanguage(t *testing.T) {
	tests := []struct {
		code string
		want string
	}{
		{"import os\nprint('hello')", "python"},
		{"console.log('hello')", "node"},
		{"#!/bin/bash\necho hello", "bash"},
		{"x = 1", "python"}, // ambiguous defaults to python
	}
	for _, tt := range tests {
		got := detectLanguage(tt.code)
		if got != tt.want {
			t.Errorf("detectLanguage(%q) = %q, want %q", tt.code, got, tt.want)
		}
	}
}

func TestDetectProvider(t *testing.T) {
	// Should not panic; returns "docker" as default
	p := detectProvider()
	if p != "docker" {
		t.Errorf("detectProvider() = %q, want 'docker'", p)
	}
}
