// Command smb-lease is the thin CLI seam onto the smblease library: bash
// callers (start.sh, stop.sh, e2e-linux.sh) shell out to it; the orchestrator
// imports the library directly in-process. It parses one verb and calls the
// matching smblease.* function — no lock logic lives here.
//
// Verbs:
//
//	acquire <holder-id> <mode>   register a lease + adopt-or-reconcile the stack
//	release <holder-id>          drop a lease; down the stack at zero
//	reconcile <mode>             additive up -d under the lock (no down)
//	status                       print lease + stack state
//
// On `acquire` success the stack is guaranteed adopted-or-up, so the bash caller
// skips its own `compose up` and proceeds straight to its TCP/health probe. Exit
// 0 = success; any non-zero exit is the caller's signal to fall back to the
// legacy direct up/down path (the Go-missing / helper-broken safety net).
package main

import (
	"fmt"
	"os"

	"cmdr/scripts/check/smblease"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	verb := os.Args[1]
	switch verb {
	case "acquire":
		if len(os.Args) != 4 {
			fmt.Fprintln(os.Stderr, "usage: smb-lease acquire <holder-id> <mode>")
			os.Exit(2)
		}
		holderID, mode := os.Args[2], os.Args[3]
		res, err := smblease.Acquire(holderID, mode)
		if err != nil {
			fmt.Fprintf(os.Stderr, "smb-lease acquire failed: %v\n", err)
			os.Exit(1)
		}
		// Report the decision so the caller's logs show whether it adopted or
		// reconciled; the stack is up either way, so the caller skips its `up`.
		fmt.Printf("%s\n", res.Action)
	case "release":
		if len(os.Args) != 3 {
			fmt.Fprintln(os.Stderr, "usage: smb-lease release <holder-id>")
			os.Exit(2)
		}
		if err := smblease.Release(os.Args[2]); err != nil {
			fmt.Fprintf(os.Stderr, "smb-lease release failed: %v\n", err)
			os.Exit(1)
		}
	case "reconcile":
		if len(os.Args) != 3 {
			fmt.Fprintln(os.Stderr, "usage: smb-lease reconcile <mode>")
			os.Exit(2)
		}
		if err := smblease.Reconcile(os.Args[2]); err != nil {
			fmt.Fprintf(os.Stderr, "smb-lease reconcile failed: %v\n", err)
			os.Exit(1)
		}
	case "status":
		if err := smblease.Status(); err != nil {
			fmt.Fprintf(os.Stderr, "smb-lease status failed: %v\n", err)
			os.Exit(1)
		}
	default:
		usage()
		os.Exit(2)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: smb-lease <acquire <holder-id> <mode> | release <holder-id> | reconcile <mode> | status>")
}
