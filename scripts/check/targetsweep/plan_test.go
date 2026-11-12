package targetsweep

import (
	"slices"
	"strings"
	"testing"
	"time"
)

const gib = int64(1) << 30

var (
	now  = time.Date(2026, 11, 12, 12, 0, 0, 0, time.UTC)
	seed = now.Add(-5 * time.Hour)
	day  = 24 * time.Hour
)

func promotePolicy() Policy {
	return Policy{Now: now, Seed: seed, SeedKnown: true}
}

func liveSet(hashes ...string) Live {
	l := Live{Hashes: map[string]bool{}}
	for _, h := range hashes {
		l.Hashes[h] = true
	}
	return l
}

func unit(path string, age time.Duration) Unit {
	name := path[strings.LastIndex(path, "/")+1:]
	h, ok := unitHash(name)
	return Unit{Path: path, Hash: h, Hashed: ok, Newest: now.Add(-age), Links: 1, Size: 1,
		Fingerprint: strings.HasPrefix(path, "debug/.fingerprint/")}
}

func deleted(p Plan) []string {
	out := slices.Clone(p.Delete)
	slices.Sort(out)
	return out
}

func expectDeleted(t *testing.T, p Plan, want ...string) {
	t.Helper()
	slices.Sort(want)
	if got := deleted(p); !slices.Equal(got, want) {
		t.Errorf("deleted:\n  got  %v\n  want %v", got, want)
	}
}

// The third-party rule, exactly as the spike measured it (0 units rebuilt across all three
// lanes): a unit no stream names is dead.
func TestPlanDeletesUnitsNoStreamNames(t *testing.T) {
	l := Listing{Units: []Unit{
		unit("debug/deps/libserde-1111111111111111.rlib", 20*day),
		unit("debug/deps/libserde-9999999999999999.rlib", 20*day),
		unit("debug/deps/serde-9999999999999999.d", 20*day),
		unit("debug/build/ring-9999999999999999", 20*day),
		unit("debug/examples/bench-9999999999999999", 20*day),
	}}
	p := plan(l, liveSet("1111111111111111"), promotePolicy())
	expectDeleted(t, p,
		"debug/deps/libserde-9999999999999999.rlib",
		"debug/deps/serde-9999999999999999.d",
		"debug/build/ring-9999999999999999",
		"debug/examples/bench-9999999999999999",
	)
	if p.DeletedUnits != 4 {
		t.Errorf("DeletedUnits = %d, want 4", p.DeletedUnits)
	}
}

// Cargo uplifts a bin or an example by hard-linking `deps/<name>-<h>` to `<name>`, and the
// stream names only the uplifted copy. A hard-linked unit is therefore live, and so is
// everything else carrying its hash (its fingerprint, its `.d`): without that, the spike's
// hash-only prune rebuilt every bin and example.
func TestPlanKeepsHardLinkedUnitsAndEverythingSharingTheirHash(t *testing.T) {
	linked := unit("debug/deps/cmdr-2222222222222222", 20*day)
	linked.Links = 2
	staleBin := unit("debug/deps/cmdr-3333333333333333", 20*day) // its uplift moved on
	l := Listing{Units: []Unit{
		linked,
		unit("debug/deps/cmdr-2222222222222222.d", 20*day),
		unit("debug/.fingerprint/cmdr-2222222222222222", 40*day),
		staleBin,
	}}
	p := plan(l, liveSet(), promotePolicy())
	expectDeleted(t, p, "debug/deps/cmdr-3333333333333333")
}

// Whatever the checkout wrote after its seed is its own work: kept, whatever the streams
// say, so a run that recorded nothing (an older lane, a hand-run cargo) loses nothing.
func TestPlanKeepsUnitsWrittenAfterTheSeed(t *testing.T) {
	l := Listing{Units: []Unit{
		unit("debug/deps/libmine-4444444444444444.rlib", time.Hour),  // after the seed
		unit("debug/deps/libold-5555555555555555.rlib", 6*time.Hour), // before it
	}}
	p := plan(l, liveSet(), promotePolicy())
	expectDeleted(t, p, "debug/deps/libold-5555555555555555.rlib")
}

// Fingerprint dirs are a few KB, and they're the only record of a rustdoc unit (whose
// output lives under `doc/` and names no hash), so a young one stays regardless.
func TestPlanKeepsYoungFingerprintsWhateverTheStreamsSay(t *testing.T) {
	l := Listing{Units: []Unit{
		unit("debug/.fingerprint/cmdr-6666666666666666", 29*day),
		unit("debug/.fingerprint/cmdr-7777777777777777", 31*day),
		unit("debug/deps/libcmdr-6666666666666666.rmeta", 29*day),
	}}
	p := plan(l, liveSet(), promotePolicy())
	expectDeleted(t, p, "debug/.fingerprint/cmdr-7777777777777777", "debug/deps/libcmdr-6666666666666666.rmeta")
}

// An entry whose name carries no hash is something the sweep doesn't understand (a
// cdylib, a staticlib, a stray file): never guessed at, always kept.
func TestPlanKeepsEntriesWithoutAHash(t *testing.T) {
	l := Listing{Units: []Unit{
		unit("debug/deps/libcmdr_lib.so", 90*day),
		unit("debug/deps/libcmdr_lib.a", 90*day),
	}}
	if p := plan(l, liveSet(), promotePolicy()); len(p.Delete) != 0 {
		t.Errorf("deleted %v, want nothing", p.Delete)
	}
}

// The guard against a cascade: with an unsound live set (a lane's stream missing, empty,
// or unfinished), or a layout the sweep doesn't recognize, or no seed boundary to judge
// "the checkout's own work" by, no build unit goes. The incremental rule needs none of
// those and still runs.
func TestPlanLeavesUnitsAloneWhenItsInputsArentSound(t *testing.T) {
	l := Listing{
		Units:       []Unit{unit("debug/deps/libserde-9999999999999999.rlib", 20*day)},
		Incremental: []Incremental{{Path: "debug/incremental/cmdr-x", Newest: now.Add(-10 * day), Size: 1}},
	}
	cases := []struct {
		name   string
		live   Live
		layout string
		policy Policy
		want   string
	}{
		{"unsound streams", Live{Problem: "the tests lane has no finished build on record"}, "", promotePolicy(), "tests lane"},
		{"unknown layout", liveSet(), "a fingerprint entry has no hash", promotePolicy(), "fingerprint entry"},
		{"no seed stamp", liveSet(), "", Policy{Now: now}, "seed"},
	}
	for _, c := range cases {
		l.LayoutProblem = c.layout
		p := plan(l, c.live, c.policy)
		expectDeleted(t, p, "debug/incremental/cmdr-x")
		if !strings.Contains(p.UnitsSkipped, c.want) {
			t.Errorf("%s: UnitsSkipped = %q, want it to name %q", c.name, p.UnitsSkipped, c.want)
		}
	}
}

// An incremental dir any session touched since the seed, or in the last 72 hours, stays.
// The window keeps the state of a lane the worktree didn't run: dropping `incremental/`
// wholesale made a new worktree's first clippy-linux run 2m39s instead of 29 s.
func TestPlanKeepsRecentIncrementalDirs(t *testing.T) {
	l := Listing{Incremental: []Incremental{
		{Path: "debug/incremental/a", Newest: now.Add(-time.Hour), Size: 1},      // after the seed
		{Path: "debug/incremental/b", Newest: now.Add(-71 * time.Hour), Size: 1}, // inside the window
		{Path: "debug/incremental/c", Newest: now.Add(-73 * time.Hour), Size: 1}, // neither
	}}
	p := plan(l, liveSet(), promotePolicy())
	expectDeleted(t, p, "debug/incremental/c")

	// No stamp: the window alone decides.
	p = plan(l, liveSet(), Policy{Now: now})
	expectDeleted(t, p, "debug/incremental/c")
}

// The backstop, in order: over budget after the sweep, drop the incremental dirs older
// than the seed (the window's extras); still over, reset the cache.
func TestPlanBudgetBackstopRePrunesIncrementalThenResets(t *testing.T) {
	window := Incremental{Path: "debug/incremental/window", Newest: now.Add(-6 * time.Hour), Size: 20 * gib}
	mine := Incremental{Path: "debug/incremental/mine", Newest: now.Add(-time.Hour), Size: 5 * gib}

	l := Listing{Incremental: []Incremental{window, mine}, OtherBytes: 30 * gib}
	p := plan(l, liveSet(), promotePolicy())
	expectDeleted(t, p, "debug/incremental/window")
	if !p.Repruned || p.Reset || p.BytesAfter != 35*gib {
		t.Errorf("Repruned=%v Reset=%v BytesAfter=%d GiB, want true/false/35", p.Repruned, p.Reset, p.BytesAfter/gib)
	}

	l.OtherBytes = 50 * gib
	p = plan(l, liveSet(), promotePolicy())
	if !p.Reset || p.BytesAfter != 0 {
		t.Errorf("Reset=%v BytesAfter=%d, want a reset to 0", p.Reset, p.BytesAfter)
	}

	l.OtherBytes = 10 * gib
	if p = plan(l, liveSet(), promotePolicy()); p.Repruned || p.Reset || len(p.Delete) != 0 {
		t.Errorf("under budget: Repruned=%v Reset=%v deleted %v, want nothing", p.Repruned, p.Reset, p.Delete)
	}
}

// With no seed stamp nothing is provably the checkout's own, so the re-prune drops every
// incremental dir. That's what keeps a pre-stamp volume from going straight to a reset (a
// cold build) when its units can't be swept.
func TestPlanBudgetRePruneWithoutASeedDropsAllIncremental(t *testing.T) {
	l := Listing{
		Units:       []Unit{{Path: "debug/deps/big", Size: 30 * gib, Newest: now.Add(-day), Links: 1}},
		Incremental: []Incremental{{Path: "debug/incremental/recent", Newest: now.Add(-time.Hour), Size: 20 * gib}},
	}
	p := plan(l, Live{Problem: "x"}, Policy{Now: now})
	expectDeleted(t, p, "debug/incremental/recent")
	if !p.Repruned || p.Reset {
		t.Errorf("Repruned=%v Reset=%v, want a re-prune and no reset", p.Repruned, p.Reset)
	}
}

func TestPlanAccountsBytes(t *testing.T) {
	l := Listing{
		Units: []Unit{
			{Path: "debug/deps/liba-1111111111111111.rlib", Hash: "1111111111111111", Hashed: true, Newest: now.Add(-20 * day), Links: 1, Size: 3},
			{Path: "debug/deps/liba-9999999999999999.rlib", Hash: "9999999999999999", Hashed: true, Newest: now.Add(-20 * day), Links: 1, Size: 5},
		},
		Incremental: []Incremental{{Path: "debug/incremental/x", Newest: now.Add(-10 * day), Size: 7}},
		OtherBytes:  11,
	}
	p := plan(l, liveSet("1111111111111111"), promotePolicy())
	if p.BytesBefore != 26 || p.BytesAfter != 14 || p.DeletedUnits != 1 || p.DeletedIncremental != 1 {
		t.Errorf("before=%d after=%d units=%d incremental=%d, want 26/14/1/1", p.BytesBefore, p.BytesAfter, p.DeletedUnits, p.DeletedIncremental)
	}
}
