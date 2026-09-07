package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

func TestWarmingWorkerPID(t *testing.T) {
	tests := []struct {
		name string
		body string
		want int
	}{
		{"a sentinel as new-worktree.sh writes it", "pid=4242\nstarted=2026-09-07T08:41:48Z\nworktree=/tmp/wt\n", 4242},
		{"pid on a later line", "started=2026-09-07T08:41:48Z\npid=7\n", 7},
		{"tolerates surrounding whitespace", "  pid=99  \n", 99},
		{"no pid line at all", "started=2026-09-07T08:41:48Z\n", 0},
		{"unparseable pid is not a live worker", "pid=nonsense\n", 0},
		{"empty file", "", 0},
		// A partially-written sentinel must not read as a live pid: the caller
		// treats 0 as a dead worker and proceeds, which is the safe direction.
		{"truncated mid-write", "pid=", 0},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := warmingWorkerPID([]byte(tc.body)); got != tc.want {
				t.Errorf("warmingWorkerPID(%q) = %d, want %d", tc.body, got, tc.want)
			}
		})
	}
}

func TestProcessAlive(t *testing.T) {
	if !processAlive(os.Getpid()) {
		t.Error("processAlive(self) = false, want true")
	}
	// pid 1 (launchd) is alive but owned by root, so signalling it fails with
	// EPERM. That has to count as alive, or a sentinel written by a job running
	// as someone else would read as dead.
	if !processAlive(1) {
		t.Error("processAlive(1) = false, want true (EPERM means alive)")
	}

	// A real exited process, rather than a pid guessed to be free.
	cmd := exec.Command("true")
	if err := cmd.Start(); err != nil {
		t.Fatalf("starting probe process: %v", err)
	}
	pid := cmd.Process.Pid
	_ = cmd.Wait()
	if processAlive(pid) {
		t.Errorf("processAlive(%d) = true after the process exited, want false", pid)
	}
}

func TestAwaitWorktreeWarmingReturnsImmediatelyWithoutSentinel(t *testing.T) {
	dir := t.TempDir()
	done := make(chan struct{})
	go func() {
		awaitWorktreeWarming(dir)
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("awaitWorktreeWarming blocked with no sentinel present")
	}
}

func TestAwaitWorktreeWarmingClearsStaleSentinel(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, warmingSentinel)

	cmd := exec.Command("true")
	if err := cmd.Start(); err != nil {
		t.Fatalf("starting probe process: %v", err)
	}
	deadPID := cmd.Process.Pid
	_ = cmd.Wait()

	body := "pid=" + itoa(deadPID) + "\nstarted=2026-09-07T00:00:00Z\n"
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("writing sentinel: %v", err)
	}

	done := make(chan struct{})
	go func() {
		awaitWorktreeWarming(dir)
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("awaitWorktreeWarming blocked on a sentinel whose worker is dead")
	}

	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Error("stale sentinel survived; every later run in this worktree would wait on it")
	}
}

func TestAwaitWorktreeWarmingWaitsForLiveWorker(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, warmingSentinel)

	// A live worker: sleeps while we assert the wait, then we remove the
	// sentinel for it the way the script's EXIT trap does.
	cmd := exec.Command("sleep", "30")
	if err := cmd.Start(); err != nil {
		t.Fatalf("starting probe process: %v", err)
	}
	defer func() { _ = cmd.Process.Kill(); _ = cmd.Wait() }()

	body := "pid=" + itoa(cmd.Process.Pid) + "\nstarted=2026-09-07T00:00:00Z\n"
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("writing sentinel: %v", err)
	}

	done := make(chan struct{})
	go func() {
		awaitWorktreeWarming(dir)
		close(done)
	}()

	select {
	case <-done:
		t.Fatal("awaitWorktreeWarming returned while the warming worker was still alive")
	case <-time.After(1500 * time.Millisecond):
	}

	if err := os.Remove(path); err != nil {
		t.Fatalf("removing sentinel: %v", err)
	}
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("awaitWorktreeWarming did not return after the sentinel was removed")
	}
}
