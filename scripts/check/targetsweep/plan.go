package targetsweep

import (
	"fmt"
	"time"
)

// Unit is one entry directly under `debug/deps/`, `debug/.fingerprint/`, `debug/build/`,
// or `debug/examples/`: a file, or a whole directory.
type Unit struct {
	// Path is relative to the target dir, slash-separated.
	Path string
	// Hash is the unit hash its name ends in, when Hashed.
	Hash   string
	Hashed bool
	// Newest is the latest mtime of the entry and, for a directory, its direct children
	// (cargo rewrites a fingerprint's files in place, which doesn't touch the dir's own).
	Newest time.Time
	// Links is a file's hard-link count; 1 for a directory.
	Links uint64
	// Fingerprint marks an entry of `.fingerprint/`.
	Fingerprint bool
	// Size is the apparent size of everything in it.
	Size int64
}

// Incremental is one crate dir under `debug/incremental/`. Its name carries rustc's crate
// disambiguator, not cargo's unit hash, so no stream names it: its age decides.
type Incremental struct {
	Path string
	// Newest is the latest mtime of the dir and its sessions.
	Newest time.Time
	Size   int64
}

// Listing is a target dir as the sweep sees it.
type Listing struct {
	Units       []Unit
	Incremental []Incremental
	// OtherBytes is everything the sweep never deletes: `doc/`, the uplifted copies at the
	// top of `debug/`, the streams, and so on.
	OtherBytes int64
	// LayoutProblem says why the tree doesn't look like the cargo layout the unit rules
	// assume; non-empty means no unit goes.
	LayoutProblem string
}

// Policy is the time context of one sweep.
type Policy struct {
	Now time.Time
	// Seed is when the volume's history in this checkout began; SeedKnown is false when no
	// stamp says so.
	Seed      time.Time
	SeedKnown bool
}

// Plan is what a sweep deletes, and why.
type Plan struct {
	// Delete lists paths relative to the target dir. On Reset it holds nothing: the
	// executor empties the whole dir instead.
	Delete                           []string
	DeletedUnits, DeletedIncremental int
	BytesBefore, BytesAfter          int64
	// UnitsSkipped says why no build unit was considered; empty when the unit rules ran.
	UnitsSkipped string
	// Repruned: over budget after the sweep, so incremental dirs older than the seed went
	// too. Reset: still over, so everything goes.
	Repruned, Reset bool
}

// plan decides the sweep. Pure: the listing, the live set, and the clock in, the
// deletions out. The rules (and the measurement behind each) are in the DETAILS section
// the package doc names.
//
// A build unit stays when any of these holds:
//   - its name carries no hash (the sweep never guesses at a name it doesn't understand),
//   - a stream names its hash, or a hard-linked unit shares it (cargo's uplift of a bin
//     or an example, which the stream names only by its unhashed copy),
//   - it was written after the seed (the checkout's own work, recorded or not),
//   - it's a fingerprint dir younger than `FingerprintWindow`.
//
// No unit goes at all unless the live set is sound, the layout is the one these rules
// assume, and a seed stamp exists. An incremental dir stays when any session in it is
// newer than the seed or than `IncrementalWindow`.
func plan(l Listing, live Live, p Policy) Plan {
	var out Plan
	for _, u := range l.Units {
		out.BytesBefore += u.Size
	}
	for _, inc := range l.Incremental {
		out.BytesBefore += inc.Size
	}
	out.BytesBefore += l.OtherBytes
	after := out.BytesBefore

	out.UnitsSkipped = unitsSkipReason(l, live, p)
	if out.UnitsSkipped == "" {
		hashes := liveWithHardLinks(l.Units, live.Hashes)
		for _, u := range l.Units {
			if keepUnit(u, hashes, p) {
				continue
			}
			out.Delete = append(out.Delete, u.Path)
			out.DeletedUnits++
			after -= u.Size
		}
	}

	afterSeed := func(t time.Time) bool { return p.SeedKnown && t.After(p.Seed) }
	var kept []Incremental
	for _, inc := range l.Incremental {
		if afterSeed(inc.Newest) || p.Now.Sub(inc.Newest) < IncrementalWindow {
			kept = append(kept, inc)
			continue
		}
		out.Delete = append(out.Delete, inc.Path)
		out.DeletedIncremental++
		after -= inc.Size
	}

	if after > Budget {
		out.Repruned = true
		for _, inc := range kept {
			if afterSeed(inc.Newest) {
				continue
			}
			out.Delete = append(out.Delete, inc.Path)
			out.DeletedIncremental++
			after -= inc.Size
		}
	}
	if after > Budget {
		out.Reset = true
		out.Delete = nil
		after = 0
	}
	out.BytesAfter = after
	return out
}

func unitsSkipReason(l Listing, live Live, p Policy) string {
	switch {
	case live.Problem != "":
		return live.Problem
	case l.LayoutProblem != "":
		return l.LayoutProblem
	case !p.SeedKnown:
		return fmt.Sprintf("there's no seed stamp (%s) to tell this checkout's own builds apart", StampFile)
	}
	return ""
}

// liveWithHardLinks adds the hash of every hard-linked unit to the live set.
func liveWithHardLinks(units []Unit, live map[string]bool) map[string]bool {
	out := make(map[string]bool, len(live))
	for h := range live {
		out[h] = true
	}
	for _, u := range units {
		if u.Hashed && u.Links > 1 {
			out[u.Hash] = true
		}
	}
	return out
}

func keepUnit(u Unit, live map[string]bool, p Policy) bool {
	switch {
	case !u.Hashed, live[u.Hash], u.Newest.After(p.Seed):
		return true
	case u.Fingerprint && p.Now.Sub(u.Newest) < FingerprintWindow:
		return true
	}
	return false
}
