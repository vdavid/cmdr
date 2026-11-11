//go:build unix

package filelock

import (
	"os"
	"syscall"
)

// Lock blocks until it holds an exclusive advisory lock on f. `flock(2)` rather
// than flock(1) (absent on stock macOS) or shlock (create-or-fail, the wrong
// shape): LOCK_EX is a hold-across-section mutex on both Darwin and Linux.
func Lock(f *os.File) error {
	return syscall.Flock(int(f.Fd()), syscall.LOCK_EX)
}

// Unlock drops the lock early. Closing f drops it too.
func Unlock(f *os.File) error {
	return syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
}
