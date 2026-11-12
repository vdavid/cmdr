package targetsweep

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// The `debug/` subdirs whose entries are build units, and the incremental dir.
var unitDirs = []string{"deps", ".fingerprint", "build", "examples"}

const incrementalDir = "incremental"

// Mode says which handoff a sweep runs in, which decides where its seed boundary comes from.
type Mode string

const (
	// ModeSeed sweeps a new worktree's copy: nothing in it is the worktree's yet, so the
	// seed is now, and the sweep stamps it.
	ModeSeed Mode = "seed"
	// ModePromote sweeps a merged worktree's copy on its way to the main clone: the
	// worktree's stamp is the seed.
	ModePromote Mode = "promote"
)

// Report is one sweep's outcome.
type Report struct {
	Plan     Plan
	Took     time.Duration
	Failures int
}

// Run sweeps the target dir at root: reads the streams and the stamp, lists the tree,
// plans, and deletes. Every deletion is safe for cargo on its own (a missing output or
// fingerprint only makes cargo rebuild that unit), so a sweep cut short leaves a usable
// tree; failed deletions are counted, not fatal.
func Run(root string, mode Mode, now time.Time) (Report, error) {
	start := time.Now()
	files, err := readStreams(root)
	if err != nil {
		return Report{}, err
	}
	live, superseded := assembleLive(files, Lanes)

	policy := Policy{Now: now}
	switch mode {
	case ModeSeed:
		policy.Seed, policy.SeedKnown = now, true
	case ModePromote:
		policy.Seed, policy.SeedKnown = readStamp(root)
	default:
		return Report{}, fmt.Errorf("unknown sweep mode %q", mode)
	}

	listing, err := list(root)
	if err != nil {
		return Report{}, err
	}
	p := plan(listing, live, policy)

	r := Report{Plan: p}
	if p.Reset {
		r.Failures += emptyDir(root)
	} else {
		for _, rel := range p.Delete {
			if err := removeUnder(root, rel); err != nil {
				r.Failures++
			}
		}
		for _, name := range superseded {
			if err := os.Remove(filepath.Join(root, LiveDir, name)); err != nil {
				r.Failures++
			}
		}
	}
	if mode == ModeSeed {
		if err := writeStamp(root, now); err != nil {
			return r, err
		}
	}
	r.Took = time.Since(start)
	return r, nil
}

// removeUnder deletes one planned path, refusing anything outside `debug/`'s swept dirs:
// the plan only ever names those, and this keeps a bug from reaching further.
func removeUnder(root, rel string) error {
	clean := path.Clean(rel)
	parts := strings.Split(clean, "/")
	if len(parts) != 3 || parts[0] != "debug" || parts[2] == ".." || !isSweptDir(parts[1]) {
		return fmt.Errorf("refusing to delete %q: not an entry of a swept dir", rel)
	}
	return os.RemoveAll(filepath.Join(root, filepath.FromSlash(clean)))
}

func isSweptDir(name string) bool {
	if name == incrementalDir {
		return true
	}
	for _, d := range unitDirs {
		if name == d {
			return true
		}
	}
	return false
}

// emptyDir removes every entry of root (the reset), returning how many it couldn't.
func emptyDir(root string) int {
	entries, err := os.ReadDir(root)
	if err != nil {
		return 1
	}
	failures := 0
	for _, e := range entries {
		if err := os.RemoveAll(filepath.Join(root, e.Name())); err != nil {
			failures++
		}
	}
	return failures
}

func readStreams(root string) ([]StreamFile, error) {
	dir := filepath.Join(root, LiveDir)
	entries, err := os.ReadDir(dir)
	if errors.Is(err, fs.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var files []StreamFile
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".jsonl") {
			continue
		}
		info, err := e.Info()
		if err != nil {
			return nil, err
		}
		content, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			return nil, err
		}
		files = append(files, StreamFile{Name: e.Name(), ModTime: info.ModTime(), Content: content})
	}
	return files, nil
}

// readStamp reads the seed stamp: a Unix second. Missing or unreadable reads as unknown.
func readStamp(root string) (time.Time, bool) {
	data, err := os.ReadFile(filepath.Join(root, StampFile))
	if err != nil {
		return time.Time{}, false
	}
	secs, err := strconv.ParseInt(strings.TrimSpace(string(data)), 10, 64)
	if err != nil || secs <= 0 {
		return time.Time{}, false
	}
	return time.Unix(secs, 0), true
}

// writeStamp writes the seed stamp atomically.
func writeStamp(root string, at time.Time) error {
	tmp := filepath.Join(root, StampFile+".tmp")
	if err := os.WriteFile(tmp, []byte(strconv.FormatInt(at.Unix(), 10)+"\n"), 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, filepath.Join(root, StampFile))
}

// list reads the target dir into a Listing. It never follows a symlink.
func list(root string) (Listing, error) {
	var l Listing
	debug := filepath.Join(root, "debug")
	swept := map[string]bool{}

	if _, err := os.Lstat(filepath.Join(debug, ".fingerprint")); err != nil {
		l.LayoutProblem = "there's no debug/.fingerprint, so this isn't the cargo layout the sweep knows"
	}
	for _, d := range unitDirs {
		found, err := listUnits(&l, debug, d)
		if err != nil {
			return Listing{}, err
		}
		swept[d] = found
	}
	found, err := listIncremental(&l, debug)
	if err != nil {
		return Listing{}, err
	}
	swept[incrementalDir] = found

	other, err := otherBytes(root, swept)
	if err != nil {
		return Listing{}, err
	}
	l.OtherBytes += other
	return l, nil
}

// listUnits adds the entries of one unit dir, and flags a fingerprint entry that isn't a
// `<name>-<16 hex>` dir as a changed layout. It reports whether the dir exists.
func listUnits(l *Listing, debug, name string) (bool, error) {
	dir := filepath.Join(debug, name)
	entries, err := os.ReadDir(dir)
	if errors.Is(err, fs.ErrNotExist) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	for _, e := range entries {
		u, err := readUnit(dir, e)
		if err != nil {
			return false, err
		}
		u.Path = "debug/" + name + "/" + e.Name()
		u.Fingerprint = name == ".fingerprint"
		if u.Fingerprint && (!u.Hashed || !e.IsDir()) && l.LayoutProblem == "" {
			l.LayoutProblem = fmt.Sprintf("the fingerprint entry %s isn't a <name>-<16 hex> dir, so cargo's layout changed", e.Name())
		}
		l.Units = append(l.Units, u)
	}
	return true, nil
}

// listIncremental adds each crate dir of `incremental/`; a stray file there counts as
// other bytes. It reports whether the dir exists.
func listIncremental(l *Listing, debug string) (bool, error) {
	dir := filepath.Join(debug, incrementalDir)
	entries, err := os.ReadDir(dir)
	if errors.Is(err, fs.ErrNotExist) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	for _, e := range entries {
		full := filepath.Join(dir, e.Name())
		size, err := treeStats(full)
		if err != nil {
			return false, err
		}
		if !e.IsDir() {
			l.OtherBytes += size
			continue
		}
		newest, err := newestOf(full)
		if err != nil {
			return false, err
		}
		l.Incremental = append(l.Incremental, Incremental{Path: "debug/" + incrementalDir + "/" + e.Name(), Newest: newest, Size: size})
	}
	return true, nil
}

func readUnit(dir string, e fs.DirEntry) (Unit, error) {
	full := filepath.Join(dir, e.Name())
	h, ok := unitHash(e.Name())
	u := Unit{Hash: h, Hashed: ok, Links: 1}
	if e.IsDir() {
		newest, err := newestOf(full)
		if err != nil {
			return Unit{}, err
		}
		size, err := treeStats(full)
		if err != nil {
			return Unit{}, err
		}
		u.Newest, u.Size = newest, size
		return u, nil
	}
	info, err := os.Lstat(full)
	if err != nil {
		return Unit{}, err
	}
	u.Newest, u.Size, u.Links = info.ModTime(), info.Size(), linkCount(info)
	return u, nil
}

// newestOf is the latest mtime of a dir and its direct children.
func newestOf(dir string) (time.Time, error) {
	info, err := os.Lstat(dir)
	if err != nil {
		return time.Time{}, err
	}
	newest := info.ModTime()
	entries, err := os.ReadDir(dir)
	if err != nil {
		return time.Time{}, err
	}
	for _, e := range entries {
		ci, err := e.Info()
		if err != nil {
			return time.Time{}, err
		}
		if ci.ModTime().After(newest) {
			newest = ci.ModTime()
		}
	}
	return newest, nil
}

// treeStats sums the apparent size of everything under p.
func treeStats(p string) (int64, error) {
	var size int64
	err := filepath.WalkDir(p, func(_ string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		size += info.Size()
		return nil
	})
	return size, err
}

// otherBytes sums everything outside the swept dirs.
func otherBytes(root string, swept map[string]bool) (int64, error) {
	var total int64
	entries, err := os.ReadDir(root)
	if err != nil {
		return 0, err
	}
	for _, e := range entries {
		full := filepath.Join(root, e.Name())
		if e.Name() != "debug" || !e.IsDir() {
			size, err := treeStats(full)
			if err != nil {
				return 0, err
			}
			total += size
			continue
		}
		debugEntries, err := os.ReadDir(full)
		if err != nil {
			return 0, err
		}
		for _, de := range debugEntries {
			if de.IsDir() && swept[de.Name()] {
				continue
			}
			size, err := treeStats(filepath.Join(full, de.Name()))
			if err != nil {
				return 0, err
			}
			total += size
		}
	}
	return total, nil
}
