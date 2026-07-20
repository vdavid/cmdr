//go:build !darwin

package main

import "fmt"

// measureBulk has no portable counterpart: getattrlistbulk(2) is macOS-only, and
// the spike it serves is about macOS re-anchor cost.
func measureBulk(dir string, bufBytes int) (Result, error) {
	_, _ = dir, bufBytes
	return Result{}, fmt.Errorf("the bulk method needs getattrlistbulk(2), which only exists on macOS")
}
