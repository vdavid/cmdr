package main

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"time"

	"cmdr/scripts/check/checks"
)

// The stale-debug-object prune.
//
// On macOS the dev profile's debuginfo stays unpacked: a linked test binary
// carries no DWARF, only a debug map pointing at the `.rcgu.o` files beside it
// in `target/debug/deps`. Each recompile of a unit writes a full new GENERATION
// of those objects under a new session suffix, and neither cargo nor rustc ever
// removes the old one. A worktree reached 457k entries there in a few days
// (getcmdr/cmdr#357), and that alone stalled the Rust lanes: the first NSURL call
// in a test process (`CFBundleGetMainBundle`) `readdir`s the binary's own
// directory, and 16 test processes doing that at once missed their 5 s waits.
//
// So before any lane that builds in `target/`, the runner deletes every
// generation of a unit that a newer generation superseded at least
// `rcguSupersededGrace` ago. The live generation always survives, whatever its
// age, because only a newer one can make it stale.
//
// Why generations and not "older than N hours": a CGU that incremental
// compilation reused is a HARD LINK to the work product, so it keeps the mtime
// of the session that first wrote it. An age cutoff would delete objects the
// current binary still points at, losing file:line in its backtraces. A
// generation's freshest member is its true write time.

// rcguSupersededGrace is how long a superseded generation stays. Until then a
// binary built from it may still be running (a lane started before the rebuild,
// another run in this worktree), and it reads these objects to symbolize a
// backtrace.
const rcguSupersededGrace = 15 * time.Minute

// rcguProfileDeps are the dirs under the target dir the compiling lanes link
// test binaries in. Clippy's own dir holds no objects (it never codegens).
var rcguProfileDeps = []string{filepath.Join("debug", "deps")}

type rcguFile struct {
	name  string
	mtime time.Time
	size  int64
}

type rcguPruneResult struct {
	files int
	bytes int64
}

// rcguGeneration splits a `.rcgu.o` name into its unit (`<crate>-<hash>`, the
// binary or rlib it belongs to) and its generation key: everything but the
// per-CGU part. Two shapes exist:
//
//   - incremental: `<unit>.<cgu>.<session>.rcgu.o`, generation `<session>`
//   - non-incremental: `<unit>.<crate>.<hash>-cgu.<n>.rcgu.o`, generation
//     `<crate>.<hash>-cgu` (a rebuild overwrites the same names)
//
// Anything else reports !ok and is never touched.
func rcguGeneration(name string) (unit, gen string, ok bool) {
	stem, found := strings.CutSuffix(name, ".rcgu.o")
	if !found {
		return "", "", false
	}
	parts := strings.Split(stem, ".")
	switch {
	case len(parts) == 3:
		return parts[0], parts[2], true
	case len(parts) == 4 && strings.HasSuffix(parts[2], "-cgu"):
		return parts[0], parts[1] + "." + parts[2], true
	}
	return "", "", false
}

// staleRcguObjects returns the files whose generation was superseded more than
// the grace before `now`. Every file must parse with rcguGeneration.
func staleRcguObjects(files []rcguFile, now time.Time) []rcguFile {
	// unit -> generation -> freshest mtime
	freshest := map[string]map[string]time.Time{}
	for _, f := range files {
		unit, gen, _ := rcguGeneration(f.name)
		gens := freshest[unit]
		if gens == nil {
			gens = map[string]time.Time{}
			freshest[unit] = gens
		}
		if f.mtime.After(gens[gen]) {
			gens[gen] = f.mtime
		}
	}

	// Per unit, the newest generation that has settled past the grace. Anything
	// strictly older than it is superseded for good.
	cutoff := now.Add(-rcguSupersededGrace)
	settled := map[string]time.Time{}
	for unit, gens := range freshest {
		for _, t := range gens {
			if !t.After(cutoff) && t.After(settled[unit]) {
				settled[unit] = t
			}
		}
	}

	var stale []rcguFile
	for _, f := range files {
		unit, gen, _ := rcguGeneration(f.name)
		if freshest[unit][gen].Before(settled[unit]) {
			stale = append(stale, f)
		}
	}
	return stale
}

// pruneStaleRcguObjects deletes the stale objects in one deps dir. It stats only
// the units holding more than one generation, so a clean dir costs one readdir.
// A missing dir is a no-op; a file that vanishes mid-prune is skipped.
func pruneStaleRcguObjects(depsDir string, now time.Time) (rcguPruneResult, error) {
	entries, err := os.ReadDir(depsDir)
	if errors.Is(err, fs.ErrNotExist) {
		return rcguPruneResult{}, nil
	}
	if err != nil {
		return rcguPruneResult{}, err
	}

	byUnit := map[string][]fs.DirEntry{}
	gensPerUnit := map[string]map[string]bool{}
	for _, e := range entries {
		unit, gen, ok := rcguGeneration(e.Name())
		if !ok || !e.Type().IsRegular() {
			continue
		}
		byUnit[unit] = append(byUnit[unit], e)
		if gensPerUnit[unit] == nil {
			gensPerUnit[unit] = map[string]bool{}
		}
		gensPerUnit[unit][gen] = true
	}

	var candidates []rcguFile
	for unit, unitEntries := range byUnit {
		if len(gensPerUnit[unit]) < 2 {
			continue
		}
		for _, e := range unitEntries {
			info, err := e.Info()
			if err != nil {
				continue
			}
			candidates = append(candidates, rcguFile{name: e.Name(), mtime: info.ModTime(), size: info.Size()})
		}
	}

	var res rcguPruneResult
	for _, f := range staleRcguObjects(candidates, now) {
		if err := os.Remove(filepath.Join(depsDir, f.name)); err != nil {
			continue
		}
		res.files++
		res.bytes += f.size
	}
	return res, nil
}

// pruneStaleRcguIfNeeded runs the prune when a lane about to run compiles in the
// shared build dir, and says so in one line when it freed anything. Running it
// before the lanes start is what makes it safe: they hold the build dir from
// here on. A failure only costs the cleanup, so it's reported, never fatal.
func pruneStaleRcguIfNeeded(rootDir string, toRun []checks.CheckDefinition) {
	compiles := false
	for _, c := range toRun {
		if c.Exclusive == checks.ResourceCargoBuildDir {
			compiles = true
			break
		}
	}
	if !compiles {
		return
	}

	targetDir := os.Getenv("CARGO_TARGET_DIR")
	if targetDir == "" {
		targetDir = filepath.Join(rootDir, "target")
	}
	var total rcguPruneResult
	for _, rel := range rcguProfileDeps {
		res, err := pruneStaleRcguObjects(filepath.Join(targetDir, rel), time.Now())
		if err != nil {
			printNotice("Couldn't prune stale debug objects in %s: %v", rel, err)
			continue
		}
		total.files += res.files
		total.bytes += res.bytes
	}
	if total.files > 0 {
		fmt.Printf("%s🧹 Pruned %s stale debug %s (%.1f GB) from target/%s\n",
			colorDim, checks.FormatThousands(total.files), checks.Pluralize(total.files, "object", "objects"),
			float64(total.bytes)/1e9, colorReset)
	}
}
