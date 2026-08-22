// Command stack-lease is the thin CLI seam onto the stacklease library: bash
// callers (the fixtures' start.sh / stop.sh, e2e-linux.sh) shell out to it; the
// check runner imports the library directly in-process. It parses one verb, one
// stack name, and calls the matching method — no lock logic lives here.
//
// Verbs:
//
//	acquire <stack> <holder-id> <mode>   register a lease + adopt-or-reconcile the stack
//	release <stack> <holder-id>          drop a lease; down the stack at zero
//	reconcile <stack> <mode>             additive up -d under the lock (no down)
//	status [<stack>]                     print lease + stack state (every stack when omitted)
//
// On `acquire` success the stack is guaranteed adopted-or-up, so the bash caller
// skips its own `compose up` and proceeds straight to its TCP/health probe. Exit
// 0 = success; any non-zero exit is the caller's signal to fall back to the
// legacy direct up/down path (the Go-missing / helper-broken safety net).
package main

import (
	"fmt"
	"os"
	"strings"

	"cmdr/scripts/check/stacklease"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	if err := run(os.Args[1], os.Args[2:]); err != nil {
		fmt.Fprintf(os.Stderr, "stack-lease %s failed: %v\n", os.Args[1], err)
		os.Exit(1)
	}
}

func run(verb string, args []string) error {
	switch verb {
	case "acquire":
		return acquire(args)
	case "release":
		if len(args) != 2 {
			return usageErr("release <stack> <holder-id>")
		}
		stack, err := stacklease.Lookup(args[0])
		if err != nil {
			return err
		}
		return stack.Release(args[1])
	case "reconcile":
		if len(args) != 2 {
			return usageErr("reconcile <stack> <mode>")
		}
		stack, err := stacklease.Lookup(args[0])
		if err != nil {
			return err
		}
		return stack.Reconcile(args[1])
	case "status":
		return status(args)
	default:
		usage()
		os.Exit(2)
		return nil
	}
}

func acquire(args []string) error {
	if len(args) != 3 {
		return usageErr("acquire <stack> <holder-id> <mode>")
	}
	stack, err := stacklease.Lookup(args[0])
	if err != nil {
		return err
	}
	res, err := stack.Acquire(args[1], args[2])
	if err != nil {
		return err
	}
	// Report the decision so the caller's logs show whether it adopted or
	// reconciled; the stack is up either way, so the caller skips its `up`.
	fmt.Println(res.Action)
	return nil
}

// status prints one stack, or every registered stack when none is named.
func status(args []string) error {
	if len(args) > 1 {
		return usageErr("status [<stack>]")
	}
	if len(args) == 1 {
		stack, err := stacklease.Lookup(args[0])
		if err != nil {
			return err
		}
		return stack.PrintStatus()
	}
	for _, stack := range stacklease.All() {
		if err := stack.PrintStatus(); err != nil {
			return err
		}
	}
	return nil
}

func usageErr(form string) error {
	return fmt.Errorf("usage: stack-lease %s (stacks: %s)", form, strings.Join(stacklease.Names(), ", "))
}

func usage() {
	fmt.Fprintf(os.Stderr,
		"usage: stack-lease <acquire <stack> <holder-id> <mode> | release <stack> <holder-id> | reconcile <stack> <mode> | status [<stack>]>\nstacks: %s\n",
		strings.Join(stacklease.Names(), ", "))
}
