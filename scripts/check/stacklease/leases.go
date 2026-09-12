package stacklease

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// ---- lease-file helpers (all callers hold the flock) ----

func validateHolderID(holderID string) error {
	if holderID == "" {
		return fmt.Errorf("holder-id must not be empty")
	}
	// A holder-id becomes a filename in LeaseDir; reject path separators so a
	// caller can't escape the dir.
	if strings.ContainsAny(holderID, "/\\") || holderID == "." || holderID == ".." {
		return fmt.Errorf("invalid holder-id %q", holderID)
	}
	return nil
}

func (s *Stack) writeLease(holderID, mode string) error {
	body := fmt.Sprintf("stack=%s\nmode=%s\nwhen=%s\nwd=%s\n", s.Name, mode, time.Now().Format(time.RFC3339), workingDir())
	return os.WriteFile(filepath.Join(s.LeaseDir(), holderID), []byte(body), 0o644)
}

func (s *Stack) removeLease(holderID string) error {
	err := os.Remove(filepath.Join(s.LeaseDir(), holderID))
	if os.IsNotExist(err) {
		return nil // already gone; idempotent
	}
	return err
}

// sweepDeadLeases removes numeric-PID lease files whose process is gone. The
// "manual" sentinel and any non-numeric holder-id are skipped by construction.
// Called ONLY under the acquire lock — never on a timer.
func (s *Stack) sweepDeadLeases() {
	holders, err := s.listLeaseHolders()
	if err != nil {
		Logf("WARN: %s lease dir unreadable during sweep (%v); skipping sweep", s.Name, err)
		return
	}
	for _, h := range holders {
		pid, err := strconv.Atoi(h)
		if err != nil {
			continue // non-numeric (e.g. "manual") → never swept
		}
		if !processAlive(pid) {
			if rmErr := os.Remove(filepath.Join(s.LeaseDir(), h)); rmErr == nil {
				InfoLogf("swept dead %s lease %d (process gone)", s.Name, pid)
			}
		}
	}
}

// processAlive reports whether pid names a live process via kill(pid, 0).
// Accepts the PID-reuse caveat by design: a recycled PID reads as alive and
// won't be swept, lingering the stack a bit longer — the benign direction.
func processAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	// On Unix, FindProcess always succeeds; Signal(0) is the liveness probe.
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	err = proc.Signal(syscall.Signal(0))
	if err == nil {
		return true
	}
	// EPERM means the process exists but we can't signal it → still alive.
	return err == syscall.EPERM
}

func (s *Stack) listLeaseHolders() ([]string, error) {
	entries, err := os.ReadDir(s.LeaseDir())
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []string
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		out = append(out, e.Name())
	}
	sort.Strings(out)
	return out, nil
}

func (s *Stack) leaseCount() (int, error) {
	holders, err := s.listLeaseHolders()
	if err != nil {
		return 0, err
	}
	return len(holders), nil
}

func (s *Stack) otherLeaseCount(self string) int {
	holders, err := s.listLeaseHolders()
	if err != nil {
		return 0
	}
	n := 0
	for _, h := range holders {
		if h != self {
			n++
		}
	}
	return n
}
