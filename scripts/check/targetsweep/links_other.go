//go:build !unix

package targetsweep

import "io/fs"

// linkCount reads as 1 off Unix. The sweep only ever runs in a Linux container; the check
// runner, which builds for Windows too, imports this package only for its names.
func linkCount(fs.FileInfo) uint64 { return 1 }
