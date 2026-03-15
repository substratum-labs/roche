package roche

import (
	"os"
	"path/filepath"
	"testing"
)

func TestParseDaemonJSON(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "daemon.json")
	if err := os.WriteFile(path, []byte(`{"pid": 12345, "port": 50051}`), 0644); err != nil {
		t.Fatal(err)
	}

	info, err := parseDaemonFile(path)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if info.PID != 12345 {
		t.Errorf("PID = %d, want 12345", info.PID)
	}
	if info.Port != 50051 {
		t.Errorf("Port = %d, want 50051", info.Port)
	}
}

func TestParseDaemonJSONMissing(t *testing.T) {
	_, err := parseDaemonFile("/nonexistent/path/daemon.json")
	if err == nil {
		t.Fatal("expected error for missing file, got nil")
	}
}

func TestParseDaemonJSONMalformed(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "daemon.json")
	if err := os.WriteFile(path, []byte(`not valid json`), 0644); err != nil {
		t.Fatal(err)
	}

	_, err := parseDaemonFile(path)
	if err == nil {
		t.Fatal("expected error for malformed JSON, got nil")
	}
}

func TestParseDaemonJSONZeroPort(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "daemon.json")
	if err := os.WriteFile(path, []byte(`{"pid": 12345, "port": 0}`), 0644); err != nil {
		t.Fatal(err)
	}

	_, err := parseDaemonFile(path)
	if err == nil {
		t.Fatal("expected error for zero port, got nil")
	}
}
