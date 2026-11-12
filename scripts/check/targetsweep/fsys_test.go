package targetsweep

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

// fakeTarget lays out a small target dir: two generations of a third-party crate, an
// uplifted bin, an old and a fresh incremental dir, and the three lanes' streams naming
// the live generation.
func fakeTarget(t *testing.T, now time.Time) string {
	t.Helper()
	root := t.TempDir()
	old := now.Add(-40 * 24 * time.Hour)

	write := func(rel string, at time.Time) {
		t.Helper()
		full := filepath.Join(root, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
		touch(t, full, at)
	}
	for _, rel := range []string{
		"debug/deps/libserde-1111111111111111.rlib",
		"debug/deps/libserde-9999999999999999.rlib",
		"debug/deps/cmdr-2222222222222222",
		"debug/deps/libcmdr_lib.so",
		"debug/.fingerprint/serde-1111111111111111/lib-serde",
		"debug/.fingerprint/serde-9999999999999999/lib-serde",
		"debug/build/ring-3333333333333333/out/x",
		"debug/build/ring-8888888888888888/out/x",
		"debug/incremental/cmdr-old/s-old/dep-graph.bin",
		"debug/incremental/cmdr-new/s-new/dep-graph.bin",
		"doc/cmdr/index.html",
	} {
		write(rel, old)
	}
	touch(t, filepath.Join(root, "debug/incremental/cmdr-new/s-new"), now.Add(-time.Hour))
	for _, dir := range []string{"debug/.fingerprint/serde-1111111111111111", "debug/.fingerprint/serde-9999999999999999", "debug/build/ring-3333333333333333", "debug/build/ring-3333333333333333/out", "debug/build/ring-8888888888888888", "debug/build/ring-8888888888888888/out", "debug/incremental/cmdr-old", "debug/incremental/cmdr-old/s-old", "debug/incremental/cmdr-new"} {
		touch(t, filepath.Join(root, dir), old)
	}
	// Cargo's uplift: `debug/cmdr` is a hard link of `deps/cmdr-<h>`.
	if err := os.Link(filepath.Join(root, "debug/deps/cmdr-2222222222222222"), filepath.Join(root, "debug/cmdr")); err != nil {
		t.Fatal(err)
	}

	stream := artifactLine("/cargo-target/debug/deps/libserde-1111111111111111.rlib") + "\n" +
		`{"reason":"build-script-executed","out_dir":"/cargo-target/debug/build/ring-3333333333333333/out"}` + "\n" + finished + "\n"
	for _, lane := range Lanes {
		full := filepath.Join(root, LiveDir, CompleteStream(lane))
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(stream), 0o644); err != nil {
			t.Fatal(err)
		}
		touch(t, full, now.Add(-2*time.Hour))
	}
	return root
}

func touch(t *testing.T, path string, at time.Time) {
	t.Helper()
	if err := os.Chtimes(path, at, at); err != nil {
		t.Fatal(err)
	}
}

func exists(root, rel string) bool {
	_, err := os.Lstat(filepath.Join(root, filepath.FromSlash(rel)))
	return err == nil
}

func writeSeedStamp(t *testing.T, root string, at time.Time) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(root, StampFile), []byte(strconv.FormatInt(at.Unix(), 10)+"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestRunPromoteSweepsTheDeadGenerationOnly(t *testing.T) {
	now := time.Now()
	root := fakeTarget(t, now)
	writeSeedStamp(t, root, now.Add(-5*time.Hour))
	// A partial older than its lane's complete stream is superseded.
	stale := filepath.Join(root, LiveDir, "tests.partial-1-1.jsonl")
	if err := os.WriteFile(stale, []byte(artifactLine("/t/debug/deps/libserde-9999999999999999.rlib")+"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	touch(t, stale, now.Add(-3*time.Hour))

	r, err := Run(root, ModePromote, now)
	if err != nil {
		t.Fatal(err)
	}
	if r.Plan.UnitsSkipped != "" || r.Failures != 0 {
		t.Fatalf("UnitsSkipped=%q Failures=%d", r.Plan.UnitsSkipped, r.Failures)
	}
	for _, gone := range []string{
		"debug/deps/libserde-9999999999999999.rlib",
		"debug/.fingerprint/serde-9999999999999999",
		"debug/build/ring-8888888888888888",
		"debug/incremental/cmdr-old",
		LiveDir + "/tests.partial-1-1.jsonl",
	} {
		if exists(root, gone) {
			t.Errorf("%s should be gone", gone)
		}
	}
	for _, kept := range []string{
		"debug/deps/libserde-1111111111111111.rlib",
		"debug/.fingerprint/serde-1111111111111111",
		"debug/build/ring-3333333333333333",
		"debug/deps/cmdr-2222222222222222", // hard-linked
		"debug/cmdr",
		"debug/deps/libcmdr_lib.so", // no hash
		"debug/incremental/cmdr-new",
		"doc/cmdr/index.html",
		StampFile,
	} {
		if !exists(root, kept) {
			t.Errorf("%s should be kept", kept)
		}
	}
	if s := r.Summary(); !strings.HasPrefix(s, "swept the Linux cache from ") || !strings.Contains(s, "(3 build units, 1 incremental dir)") {
		t.Errorf("summary = %q", s)
	}
}

// The seed mode stamps the copy, and judges it by the live set alone: nothing in a fresh
// seed is the new worktree's own yet.
func TestRunSeedStampsTheCopy(t *testing.T) {
	now := time.Now().Truncate(time.Second)
	root := fakeTarget(t, now)
	if _, err := Run(root, ModeSeed, now); err != nil {
		t.Fatal(err)
	}
	if got, ok := readStamp(root); !ok || !got.Equal(now) {
		t.Errorf("stamp = %v, %v; want %v", got, ok, now)
	}
	if exists(root, "debug/deps/libserde-9999999999999999.rlib") {
		t.Error("the dead generation should be gone")
	}
}

// No stamp at promotion: the unit rules can't tell the checkout's own work apart, so they
// don't run, and the summary says so.
func TestRunPromoteWithoutAStampLeavesUnitsAlone(t *testing.T) {
	now := time.Now()
	root := fakeTarget(t, now)
	r, err := Run(root, ModePromote, now)
	if err != nil {
		t.Fatal(err)
	}
	if !exists(root, "debug/deps/libserde-9999999999999999.rlib") {
		t.Error("with no stamp, no unit may go")
	}
	if exists(root, "debug/incremental/cmdr-old") {
		t.Error("the incremental rule needs no stamp and should still run")
	}
	if !strings.Contains(r.Summary(), "kept every build unit, since there's no seed stamp") {
		t.Errorf("summary = %q", r.Summary())
	}
}

func TestRunPromoteWithAMissingLaneLeavesUnitsAlone(t *testing.T) {
	now := time.Now()
	root := fakeTarget(t, now)
	writeSeedStamp(t, root, now.Add(-5*time.Hour))
	if err := os.Remove(filepath.Join(root, LiveDir, CompleteStream(LaneTests))); err != nil {
		t.Fatal(err)
	}
	r, err := Run(root, ModePromote, now)
	if err != nil {
		t.Fatal(err)
	}
	if !exists(root, "debug/deps/libserde-9999999999999999.rlib") {
		t.Error("with a lane's stream missing, no unit may go")
	}
	if !strings.Contains(r.Summary(), "the tests lane has no finished build on record") {
		t.Errorf("summary = %q", r.Summary())
	}
}

// A tree that isn't cargo's layout (no `.fingerprint`, or one whose entries don't carry a
// hash) gets no unit sweep at all.
func TestListFlagsAnUnknownLayout(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "debug/deps"), 0o755); err != nil {
		t.Fatal(err)
	}
	l, err := list(root)
	if err != nil {
		t.Fatal(err)
	}
	if l.LayoutProblem == "" {
		t.Error("no .fingerprint must read as an unknown layout")
	}

	if err := os.MkdirAll(filepath.Join(root, "debug/.fingerprint/serde/1111111111111111"), 0o755); err != nil {
		t.Fatal(err)
	}
	if l, err = list(root); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(l.LayoutProblem, "serde") {
		t.Errorf("a fingerprint entry without a hash must read as a changed layout, got %q", l.LayoutProblem)
	}
}

// The executor refuses any path outside the swept dirs, whatever the plan says.
func TestRemoveUnderRefusesPathsOutsideTheSweptDirs(t *testing.T) {
	root := t.TempDir()
	for _, rel := range []string{"doc/cmdr", "debug/cmdr", "debug/deps", "debug/deps/../../x", "debug/other/x", "../x", ".cmdr-live/x"} {
		if err := removeUnder(root, rel); err == nil {
			t.Errorf("removeUnder(%q) should refuse", rel)
		}
	}
}
