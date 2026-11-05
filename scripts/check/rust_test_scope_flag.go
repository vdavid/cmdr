package main

import (
	"fmt"
	"os"

	"cmdr/scripts/check/checks"
)

// handleOneShotFlags answers the flags that do one job against the repo and exit,
// before any check runs. One call in `main`, so its complexity stays put.
func handleOneShotFlags(flags *cliFlags, rootDir string) bool {
	return handlePrintRustTestScopeFlag(flags, rootDir) || handleEnsureE2EBinaryFlag(flags, rootDir)
}

// handlePrintRustTestScopeFlag answers `--print-rust-test-scope <base-ref>`: it
// prints the `CMDR_RUST_TEST_SCOPE` value for the change since that ref on stdout
// (one line: "all", "none", or space-separated package names) and why on stderr.
// `-` reads the changed paths from stdin instead.
//
// ❗ It never fails the caller. Any error (a ref git can't resolve, a missing
// cargo) prints "all" with a warning and exits 0, so a CI step that captures the
// line can only ever widen the run, never break or narrow it.
func handlePrintRustTestScopeFlag(flags *cliFlags, rootDir string) bool {
	if flags.printRustTestScope == "" {
		return false
	}
	scope, notes, err := checks.ComputeRustTestScope(rootDir, flags.printRustTestScope, os.Stdin)
	if err != nil {
		fmt.Fprintf(os.Stderr, "warning: couldn't scope the Rust tests, so they all run: %v\n", err)
	}
	for _, note := range notes {
		fmt.Fprintf(os.Stderr, "rust test scope: %s\n", note)
	}
	fmt.Println(scope.String())
	return true
}
