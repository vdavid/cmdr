package stacklease

import (
	"fmt"
	"os"

	"cmdr/scripts/check/filelock"
)

// flock holds an exclusive advisory lock on a stack's lock path for the duration
// of a critical section (`filelock`, which says why that lock).
type flock struct {
	f *os.File
}

// acquireLock opens (creating if needed) path and blocks until it holds an
// exclusive lock, which closing the file (release) drops.
func acquireLock(path string) (*flock, error) {
	// 0666 so any of the user's worktree processes can open the shared lock
	// file; the actual mutual exclusion is the lock, not file perms.
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o666)
	if err != nil {
		return nil, fmt.Errorf("open lock file %s: %w", path, err)
	}
	if err := filelock.Lock(f); err != nil {
		_ = f.Close()
		return nil, fmt.Errorf("flock %s: %w", path, err)
	}
	return &flock{f: f}, nil
}

// release drops the lock by closing the file. It also unlocks explicitly, to
// release promptly even if the close lingers.
func (l *flock) release() {
	if l == nil || l.f == nil {
		return
	}
	_ = filelock.Unlock(l.f)
	_ = l.f.Close()
	l.f = nil
}
