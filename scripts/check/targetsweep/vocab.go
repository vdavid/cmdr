// Package targetsweep is the mark-and-sweep that keeps the Linux lanes' cargo target
// volume from growing without bound. Cargo has no garbage collection for a target
// directory (rust-lang/cargo#5026), and every dependency bump, release, or feature change
// leaves a full generation of units and incremental dirs behind that nothing reads again.
//
// The lanes MARK: each Linux cargo run saves its JSON message stream into the volume
// (`LiveDir`), which names every unit the run's build graph holds. The worktree handoff
// SWEEPS (`scripts/check/linux-cache/handoff.go`, on the copy before it swaps in, at seed
// and at promotion): a unit no stream names, no hard link holds, and nothing wrote since
// the volume's seed goes. The decision is `Plan`, a pure function over a listing; `Run` is
// the filesystem around it, executed inside a container by `target-sweep`.
//
// Mechanism, guards, and the measurements behind every number:
// `scripts/check/checks/DETAILS.md` § "The target volume sweeps itself".
package targetsweep

import "time"

// The volume-relative names the lanes write and the sweep reads.
const (
	// LiveDir holds the cargo JSON message streams, one complete stream per lane plus a
	// partial one per run that didn't finish its build.
	LiveDir = ".cmdr-live"
	// StampFile holds the Unix second the volume's history in this checkout began: its
	// seed, or its first lane run when nothing seeded it. Everything written later is the
	// checkout's own work.
	StampFile = ".cmdr-seeded-at"
)

// The lanes that record a stream. Each one's complete stream must be there before the
// sweep touches a build unit: a missing one would make every unit only that lane builds
// look dead.
const (
	LaneClippy = "clippy"
	LaneDoc    = "doc"
	LaneTests  = "tests"
)

// Lanes is every recording lane.
var Lanes = []string{LaneClippy, LaneDoc, LaneTests}

// CompleteStream is the file name of a lane's last finished build's stream.
func CompleteStream(lane string) string { return lane + ".jsonl" }

// PartialStreamPrefix starts the file name of a run's stream while it runs, and of one
// whose build didn't finish (a compile error, a kill).
func PartialStreamPrefix(lane string) string { return lane + ".partial-" }

// The sweep's policy. Measured reasons for each: the DETAILS section above.
const (
	// IncrementalWindow keeps an incremental dir any session touched this recently, so a
	// lane the worktree didn't run (often the tests lane) keeps its incremental state.
	IncrementalWindow = 72 * time.Hour
	// FingerprintWindow keeps fingerprint dirs this young whatever the streams say. They
	// are a few KB each, and they are the only record of units whose output no stream
	// names by hash (rustdoc's, under `doc/`).
	FingerprintWindow = 30 * 24 * time.Hour
	// Budget is the size past which the sweep drops incremental dirs older than the seed,
	// and then, if that's still not enough, the whole cache.
	Budget int64 = 40 << 30
)
