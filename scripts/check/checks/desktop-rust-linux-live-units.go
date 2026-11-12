package checks

import (
	"fmt"
	"strings"

	"cmdr/scripts/check/targetsweep"
)

// The Linux lanes MARK what their target volume still needs: each cargo run saves its JSON
// message stream into the volume, and the worktree handoff's sweep (`targetsweep`) deletes
// the units no stream names. Mechanism and evidence: `DETAILS.md` § "The target volume
// sweeps itself".

// cargoStream says where a recorded run's stderr goes.
type cargoStream int

const (
	// stdoutToRecord sends cargo's JSON (stdout) to the record and hands back stderr,
	// which is exactly what the lane printed before it recorded anything: the rendered
	// diagnostics and the progress lines. For a run under `json-render-diagnostics`.
	stdoutToRecord cargoStream = iota
	// bothToRecord sends both streams to the record and hands it all back, in order. For
	// the doc lane, which already reads cargo's JSON out of its output.
	bothToRecord
)

// recordedCargoScript wraps a cargo command line (no redirections) so its JSON message
// stream lands in the volume. The run writes a partial stream of its own (unique per run,
// so two runs can't clobber each other); only when cargo says the build finished does it
// become the lane's complete stream, replacing the last one. A run that stops before that
// (a compile error, a kill) leaves its partial stream behind, which is what vouches for
// the units it did build. It also stamps a volume nothing seeded, so the sweep can tell
// the checkout's own builds from what came before (`targetsweep.StampFile`).
//
// Message format never changes a unit's fingerprint: text, `json`, and
// `json-render-diagnostics` runs of the same command, in any order, rebuilt 0 units for
// clippy and for the nextest build (cargo 1.99.1, 2026-11-12, measured on a warm clone).
func recordedCargoScript(lane string, stream cargoStream, command string) string {
	return recordedCargoScriptIn(linuxContainerTargetDir, lane, stream, command)
}

// recordedCargoScriptIn is `recordedCargoScript` for any target dir, so a test can run it.
func recordedCargoScriptIn(targetDir, lane string, stream cargoStream, command string) string {
	dir := targetDir + "/" + targetsweep.LiveDir
	stamp := targetDir + "/" + targetsweep.StampFile
	record := fmt.Sprintf(`"$d/%s$(date +%%s%%N)-$$.jsonl"`, targetsweep.PartialStreamPrefix(lane))

	run := command + ` 2>&1 >"$f"`
	if stream == bothToRecord {
		run = command + ` >"$f" 2>&1; rc=$?; cat "$f"; (exit $rc)`
	}
	return fmt.Sprintf(`d=%[1]s; mkdir -p "$d"
[ -e %[2]s ] || { date +%%s > %[2]s.tmp && mv %[2]s.tmp %[2]s; }
f=%[3]s
%[4]s
rc=$?
grep -q '"reason":"build-finished","success":true' "$f" && mv -f "$f" "$d/%[5]s"
exit $rc`, shellQuote(dir), shellQuote(stamp), record, run, targetsweep.CompleteStream(lane))
}

// cargoCommand is a cargo command line for the container. Every argument is
// single-quoted: the contention re-run passes a nextest filter expression
// (`test(=a::b) + test(=c::d)`) whose spaces and parens `sh -c` would otherwise split.
func cargoCommand(args ...string) string {
	quoted := make([]string, 0, len(args))
	for _, a := range args {
		quoted = append(quoted, shellQuote(a))
	}
	return "cargo " + strings.Join(quoted, " ")
}

// withCargoFlag inserts a cargo flag before the `--` that starts the tool's own
// arguments, or at the end when there's none.
func withCargoFlag(args []string, flag ...string) []string {
	for i, a := range args {
		if a == "--" {
			out := append([]string{}, args[:i]...)
			out = append(out, flag...)
			return append(out, args[i:]...)
		}
	}
	return append(append([]string{}, args...), flag...)
}
