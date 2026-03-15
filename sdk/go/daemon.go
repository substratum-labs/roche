package roche

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"syscall"
)

// daemonInfo holds the PID and port of a running roche-daemon.
type daemonInfo struct {
	PID  int `json:"pid"`
	Port int `json:"port"`
}

// detectDaemon reads ~/.roche/daemon.json and verifies the process is alive.
// Returns nil, nil if the daemon file does not exist.
// Returns an error if the file exists but is invalid or the process is dead.
func detectDaemon() (*daemonInfo, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return nil, fmt.Errorf("roche: cannot determine home directory: %w", err)
	}
	path := filepath.Join(home, ".roche", "daemon.json")

	info, err := parseDaemonFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	if !isProcessAlive(info.PID) {
		return nil, fmt.Errorf("roche: daemon process %d is not running", info.PID)
	}
	return info, nil
}

// parseDaemonFile reads and validates a daemon.json file at the given path.
func parseDaemonFile(path string) (*daemonInfo, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}

	var info daemonInfo
	if err := json.Unmarshal(data, &info); err != nil {
		return nil, fmt.Errorf("roche: invalid daemon.json: %w", err)
	}

	if info.Port == 0 {
		return nil, fmt.Errorf("roche: daemon.json has invalid port 0")
	}

	return &info, nil
}

// isProcessAlive checks whether a process with the given PID exists
// by sending signal 0.
func isProcessAlive(pid int) bool {
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	err = proc.Signal(syscall.Signal(0))
	return err == nil
}
