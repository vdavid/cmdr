//go:build unix

package targetsweep

import (
	"io/fs"
	"syscall"
)

// linkCount is a file's hard-link count, the signal of cargo's uplift.
func linkCount(info fs.FileInfo) uint64 {
	if st, ok := info.Sys().(*syscall.Stat_t); ok {
		return uint64(st.Nlink)
	}
	return 1
}
