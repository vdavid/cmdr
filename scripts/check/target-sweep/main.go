// Command target-sweep runs the Linux target volume's mark-and-sweep (`targetsweep`)
// INSIDE the handoff container, on the copy before it swaps in. `linux-cache` cross-builds
// it for the container's architecture and mounts it at `/sweep`; it's never run on the
// host.
//
//	target-sweep <target-dir> seed|promote
//
// Prints one summary line. Exit 1 means the sweep couldn't read the tree (nothing was
// deleted, or only part of the plan was); the handoff goes ahead either way, since every
// deletion is safe for cargo on its own.
package main

import (
	"fmt"
	"os"
	"time"

	"cmdr/scripts/check/targetsweep"
)

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintln(os.Stderr, "usage: target-sweep <target-dir> seed|promote")
		os.Exit(2)
	}
	report, err := targetsweep.Run(os.Args[1], targetsweep.Mode(os.Args[2]), time.Now())
	if err != nil {
		fmt.Printf("didn't sweep the Linux cache: %v\n", err)
		os.Exit(1)
	}
	fmt.Println(report.Summary())
}
