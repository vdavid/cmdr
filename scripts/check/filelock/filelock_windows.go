package filelock

import (
	"os"

	"golang.org/x/sys/windows"
)

// wholeFile is the locked range's length in both 32-bit halves: every byte, as
// `flock` locks the whole file on Unix.
const wholeFile = ^uint32(0)

// Lock blocks until it holds an exclusive lock on f: `LockFileEx` with
// LOCKFILE_EXCLUSIVE_LOCK and no FAIL_IMMEDIATELY, the Windows twin of
// `flock(LOCK_EX)`.
func Lock(f *os.File) error {
	var overlapped windows.Overlapped
	return windows.LockFileEx(windows.Handle(f.Fd()), windows.LOCKFILE_EXCLUSIVE_LOCK, 0, wholeFile, wholeFile,
		&overlapped)
}

// Unlock drops the lock early. Closing f drops it too.
func Unlock(f *os.File) error {
	var overlapped windows.Overlapped
	return windows.UnlockFileEx(windows.Handle(f.Fd()), 0, wholeFile, wholeFile, &overlapped)
}
