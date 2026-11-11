package filelock

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

// Two opens of one path exclude each other, which is what the stack leases and
// the CSV logs rely on across worktree processes: the lock is the open file's,
// not the process's.
func TestLockExcludesASecondOpenUntilUnlocked(t *testing.T) {
	path := filepath.Join(t.TempDir(), "lock")
	first, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o666)
	if err != nil {
		t.Fatal(err)
	}
	defer first.Close()
	second, err := os.OpenFile(path, os.O_RDWR, 0o666)
	if err != nil {
		t.Fatal(err)
	}
	defer second.Close()

	if err := Lock(first); err != nil {
		t.Fatalf("Lock(first): %v", err)
	}
	acquired := make(chan error, 1)
	go func() { acquired <- Lock(second) }()

	select {
	case err := <-acquired:
		t.Fatalf("the second open took the lock while the first held it (err %v)", err)
	case <-time.After(200 * time.Millisecond):
	}

	if err := Unlock(first); err != nil {
		t.Fatalf("Unlock(first): %v", err)
	}
	select {
	case err := <-acquired:
		if err != nil {
			t.Fatalf("Lock(second) after the first unlocked: %v", err)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("the second open never took the lock after the first unlocked")
	}
	_ = Unlock(second)
}
