package stacklease

import (
	"fmt"
	"os"
)

// stderrLogf is the package's one real printer: every default log sink
// (Logf, InfoLogf, and SetVerbose(true)'s restore) prints through it, so the
// "[stacklease] "-prefixed stderr line has exactly one definition.
func stderrLogf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "[stacklease] "+format+"\n", args...)
}

// Logf is the package's WARNING sink: every `WARN:`-prefixed line, printed
// unconditionally. Defaults to stderrLogf; the CLI main and tests can
// redirect it. Anything logged here is a human reading a leaked-stack
// situation needs to reconstruct what happened, so it is never silenced.
var Logf = stderrLogf

// InfoLogf is the package's routine-decision sink: adopt/reconcile rationale,
// swept-dead-lease notices, release/teardown notes, and republished-key
// confirmations. Defaults to the same real printer as Logf, so every direct
// caller (start.sh/stop.sh/e2e-linux.sh via the `stack-lease` CLI, `go run`,
// the test binary before a test silences it) keeps seeing what it always has.
// The check runner is the one caller that dials this down: SetVerbose(false)
// swaps it to a no-op so a `pnpm check` run that just adopts an already-serving
// stack (the common case) prints nothing here; -v or CI restore it.
var InfoLogf = stderrLogf

// SetVerbose turns InfoLogf on (the default) or off. The check runner calls
// this once at startup from its -v/--verbose flag (CI passes true too).
func SetVerbose(v bool) {
	if v {
		InfoLogf = stderrLogf
		return
	}
	InfoLogf = func(format string, args ...any) {}
}

// OnReconcileStart is called synchronously, under the stack's lock, the
// moment Acquire decides the stack needs `up -d` — before that (possibly
// slow) command runs. The check runner's orchestrator uses this to print one
// friendly line while Docker does the work it's waiting on; every other
// caller defaults to a no-op.
var OnReconcileStart = func(stackName string) {}

// OnTeardown is called synchronously, under the stack's lock, the moment
// Release decides this was the stack's last holder and is about to run
// `compose down`. The check runner's orchestrator uses this to print one
// friendly line when a stack it held actually goes down; every other caller
// defaults to a no-op.
var OnTeardown = func(stackName string) {}
