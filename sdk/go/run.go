// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

package roche

import (
	"context"
	"os/exec"
	"strings"
)

// RunOptions configures a one-shot code execution. All fields are optional.
type RunOptions struct {
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

type langConfig struct {
	image   string
	command func(code string) []string
}

var languageConfigs = map[string]langConfig{
	"python": {
		image:   "python:3.12-slim",
		command: func(code string) []string { return []string{"python3", "-c", code} },
	},
	"node": {
		image:   "node:20-slim",
		command: func(code string) []string { return []string{"node", "-e", code} },
	},
	"bash": {
		image:   "ubuntu:22.04",
		command: func(code string) []string { return []string{"bash", "-c", code} },
	},
}

func detectLanguage(code string) string {
	indicators := map[string][]string{
		"python": {"import ", "def ", "print(", "from ", "class "},
		"node":   {"console.log", "require(", "const ", "let ", "function ", "=>"},
		"bash":   {"#!/bin/bash", "echo ", "grep ", "awk ", "curl "},
	}
	best := "python"
	bestScore := 0
	for lang, keywords := range indicators {
		score := 0
		for _, kw := range keywords {
			if strings.Contains(code, kw) {
				score++
			}
		}
		if score > bestScore {
			bestScore = score
			best = lang
		}
	}
	return best
}

func detectProvider() string {
	if _, err := exec.LookPath("docker"); err == nil {
		return "docker"
	}
	return "docker"
}

// Run executes code in a sandbox and returns the result. One-liner API.
//
//	result, err := roche.Run(ctx, "print(2+2)")
//	fmt.Println(result.Stdout) // "4\n"
func Run(ctx context.Context, code string, opts ...RunOptions) (*ExecOutput, error) {
	var o RunOptions
	if len(opts) > 0 {
		o = opts[0]
	}

	lang := o.Language
	if lang == "" || lang == "auto" {
		lang = detectLanguage(code)
	}

	config, ok := languageConfigs[lang]
	if !ok {
		config = languageConfigs["python"]
	}
	command := config.command(code)

	timeoutSecs := o.TimeoutSecs
	if timeoutSecs == 0 {
		timeoutSecs = 30
	}

	traceLevel := o.TraceLevel
	if traceLevel == "" {
		traceLevel = TraceLevelSummary
	}

	provider := detectProvider()

	client, err := New(WithProvider(provider))
	if err != nil {
		return nil, err
	}

	network := o.Network || len(o.NetworkAllowlist) > 0
	writable := o.Writable || len(o.FSPaths) > 0

	sandbox, err := client.Create(ctx, SandboxConfig{
		Image:            config.image,
		TimeoutSecs:      int(timeoutSecs),
		Network:          network,
		Writable:         writable,
		Memory:           o.Memory,
		NetworkAllowlist: o.NetworkAllowlist,
		FSPaths:          o.FSPaths,
	})
	if err != nil {
		return nil, err
	}
	defer sandbox.Destroy(ctx)

	return sandbox.Exec(ctx, command, &ExecOptions{
		TimeoutSecs: &timeoutSecs,
		TraceLevel:  traceLevel,
	})
}
