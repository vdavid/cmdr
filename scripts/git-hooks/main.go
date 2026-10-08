// Command git-hooks is the logic behind the repo's git hooks (`.githooks/`). It keeps
// commits formatted the way CI's `oxfmt`, `rustfmt`, and `gofmt` lanes check them, so
// a forgotten formatter run never reaches `origin`, and at push time it also
// regenerates stale license notices and runs CI's size limits. Docs:
// docs/tooling/git-hooks.md.
package main

import (
	"fmt"
	"io"
	"os"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdin, os.Stderr))
}

// run executes one hook and returns the exit code git sees.
//
// A hook that can't do its job says so and returns 0. Formatting here is a
// convenience and CI stays the gate, so a hook that trips over itself must never
// stand between someone and their commit. The deliberate non-zero exits are
// pre-push stopping a push it added a commit to, and one that breaks CI's size limits.
func run(args []string, stdin io.Reader, stderr io.Writer) int {
	if len(args) != 2 {
		fmt.Fprintln(stderr, "usage: git-hooks <pre-commit|post-commit|pre-push> <repo-root>")
		return 0
	}
	hook, root := args[0], args[1]
	// Git starts a hook in the worktree root and may hand it a `GIT_INDEX_FILE`
	// relative to that root, while the wrapper script runs this binary from wherever
	// it built it.
	if err := os.Chdir(root); err != nil {
		fmt.Fprintf(stderr, "format hook: skipped, couldn't enter %s: %v\n", root, err)
		return 0
	}
	r := repo{root: root}

	var err error
	switch hook {
	case "pre-commit":
		err = preCommit(r, formatters(root), stderr)
	case "post-commit":
		err = postCommit(r)
	case "pre-push":
		var stopPush bool
		stopPush, err = prePush(r, formatters(root), newCheckRunner(root), stdin, stderr)
		if stopPush {
			return 1
		}
	default:
		err = fmt.Errorf("no such hook: %s", hook)
	}
	if err != nil {
		fmt.Fprintf(stderr, "format hook: skipped, %v\n", err)
	}
	return 0
}
