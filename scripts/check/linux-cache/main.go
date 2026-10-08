// Command linux-cache is the thin CLI seam the worktree hooks (`scripts/worktree-hooks/`)
// call to hand the Linux lanes' target volume between the main clone and a worktree. The
// logic is `handoff.go`; the volume names and labels are the lanes' own, from `checks`.
//
// Verbs:
//
//	seed <worktree> <main-clone>                  copy the main clone's volume into a new worktree's
//	promote <worktree> <main-clone> [--dry-run]   copy a merged worktree's volume into the main clone's
//
// Prints at most one line on stdout. Exit 0 covers every "nothing to do" and "skipped"
// outcome (no Docker, no volume, not merged, a build holding the cache); exit 1 means a
// copy went wrong, with the reason on stderr.
package main

import (
	"fmt"
	"os"
	"path/filepath"
)

func main() {
	msg, err := run(os.Args[1:])
	if err != nil {
		fmt.Fprintf(os.Stderr, "linux-cache: %v\n", err)
		os.Exit(1)
	}
	if msg != "" {
		fmt.Println(msg)
	}
}

func run(args []string) (string, error) {
	dryRun := false
	var positional []string
	for _, a := range args {
		if a == "--dry-run" {
			dryRun = true
			continue
		}
		positional = append(positional, a)
	}
	if len(positional) != 3 {
		return "", usageErr()
	}
	worktree, err := filepath.Abs(positional[1])
	if err != nil {
		return "", err
	}
	mainClone, err := filepath.Abs(positional[2])
	if err != nil {
		return "", err
	}
	switch positional[0] {
	case "seed":
		if dryRun {
			return "", usageErr()
		}
		return seed(worktree, mainClone)
	case "promote":
		return promote(worktree, mainClone, dryRun)
	default:
		return "", usageErr()
	}
}

func usageErr() error {
	return fmt.Errorf("usage: linux-cache seed <worktree> <main-clone> | promote <worktree> <main-clone> [--dry-run]")
}
