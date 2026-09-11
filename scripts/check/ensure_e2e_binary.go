package main

import (
	"fmt"
	"os"
	"time"

	"cmdr/scripts/check/checks"
)

// handleEnsureE2EBinaryFlag serves `--ensure-e2e-binary`: it hands a caller outside
// the check run (the i18n screenshot run) the E2E binary for this tree, and reports
// whether it handled the invocation. The path is the only thing on stdout; progress
// and errors go to stderr. It waits out a warming worktree like any build, but skips
// the main-clone guard, which exists for auto-fixers: a build writes nothing git
// tracks. `--fresh` forces the compile.
func handleEnsureE2EBinaryFlag(flags *cliFlags, rootDir string) bool {
	if !flags.ensureE2EBinary {
		return false
	}
	awaitWorktreeWarming(rootDir)
	ctx := &checks.CheckContext{RootDir: rootDir, ReuseArtifacts: !cacheBypassed(flags)}
	start := time.Now()
	binaryPath, err := checks.EnsureE2EBinary(ctx, start.Unix(), os.Stderr)
	if err != nil {
		printError("Error: %v", err)
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "E2E binary ready (%s)\n", formatDuration(time.Since(start)))
	fmt.Println(binaryPath)
	return true
}
