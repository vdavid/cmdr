package smblease

import (
	"fmt"
	"os"
	"syscall"
)

// flock holds an exclusive advisory lock on LockPath for the duration of a
// critical section. We use syscall.Flock(LOCK_EX) rather than flock(1) (absent
// on stock macOS) or shlock (create-or-fail, wrong shape): LOCK_EX is a
// hold-across-section mutex that works natively on both Darwin and Linux.
type flock struct {
	f *os.File
}

// acquireLock opens (creating if needed) LockPath and blocks until it holds an
// exclusive lock. The lock is process-associated via the open fd, so closing
// the fd (release) drops it even if the process later forks.
func acquireLock() (*flock, error) {
	// 0666 so any of the user's worktree processes can open the shared lock
	// file; the actual mutual exclusion is the advisory LOCK_EX, not file perms.
	f, err := os.OpenFile(LockPath(), os.O_CREATE|os.O_RDWR, 0o666)
	if err != nil {
		return nil, fmt.Errorf("open lock file %s: %w", LockPath(), err)
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX); err != nil {
		_ = f.Close()
		return nil, fmt.Errorf("flock %s: %w", LockPath(), err)
	}
	return &flock{f: f}, nil
}

// release drops the lock by closing the fd. Closing implicitly releases the
// flock; we also LOCK_UN explicitly for clarity and to release promptly even if
// the fd lingers in a buffered close.
func (l *flock) release() {
	if l == nil || l.f == nil {
		return
	}
	_ = syscall.Flock(int(l.f.Fd()), syscall.LOCK_UN)
	_ = l.f.Close()
	l.f = nil
}
