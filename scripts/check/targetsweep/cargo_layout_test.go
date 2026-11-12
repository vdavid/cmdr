package targetsweep

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"
)

// TestCargoLayoutMatchesTheSweepsAssumptions pins the sweep against a REAL cargo on the
// repo's pinned toolchain, building a tiny two-crate workspace with every unit kind the
// lanes produce: a lib, a bin and an example (both uplifted by hard link), a build script,
// an integration test, and docs. It asserts what the sweep's safety rests on:
//
//  1. every `.fingerprint` entry is a `<name>-<16 hex>` dir (the layout probe),
//  2. the lanes' streams, plus the hard-link rule, name every hashed unit of the current
//     graph, so a sweep with every time-based rule expired deletes nothing cargo needs:
//     the rebuild afterwards is fully fresh (docs aside, which the fingerprint window
//     covers, see below),
//  3. after a generation change (a version bump), that same sweep deletes the old
//     generation and the rebuild is still fully fresh.
//
// A toolchain bump that changes cargo's layout (`-Z build-dir-new-layout`, once it
// stabilizes) fails it here instead of turning a sweep into a cold build. Skipped when
// the pinned toolchain isn't installed (CI's light lane), since installing one costs a
// minute; the machines that hold a Linux target volume always have it.
func TestCargoLayoutMatchesTheSweepsAssumptions(t *testing.T) {
	channel := pinnedChannel(t)
	ws := tinyWorkspace(t, channel)
	target := filepath.Join(ws, "target")

	record(t, ws, LaneClippy, "build", "--all-targets")
	record(t, ws, LaneTests, "test", "--no-run")
	record(t, ws, LaneDoc, "doc", "--no-deps")

	l, err := list(target)
	if err != nil {
		t.Fatal(err)
	}
	if l.LayoutProblem != "" {
		t.Fatalf("cargo's layout no longer matches the sweep's assumptions: %s", l.LayoutProblem)
	}
	var linked int
	for _, u := range l.Units {
		if u.Links > 1 {
			linked++
		}
	}
	if linked < 2 {
		t.Errorf("expected cargo to uplift the bin and the example by hard link, found %d linked units", linked)
	}

	// (2) With every time-based rule expired, only the streams and the hard links decide.
	// The incremental dirs go too (expired), which costs rebuild speed, never freshness.
	p := sweepAsIfLongAfter(t, target)
	for _, rel := range p.Delete {
		if !strings.HasPrefix(rel, "debug/incremental/") && !isDocFingerprint(t, target, rel) {
			t.Errorf("the sweep would delete %s, a unit of the current graph", rel)
		}
	}
	executeForTest(t, target, p)
	expectFresh(t, ws, "build", "--all-targets")
	expectFresh(t, ws, "test", "--no-run")

	// (3) A new generation: every unit hash in the graph changes.
	manifest := filepath.Join(ws, "dep", "Cargo.toml")
	bumpVersion(t, manifest)
	record(t, ws, LaneClippy, "build", "--all-targets")
	record(t, ws, LaneTests, "test", "--no-run")
	record(t, ws, LaneDoc, "doc", "--no-deps")

	p = sweepAsIfLongAfter(t, target)
	if p.DeletedUnits < 10 {
		t.Errorf("expected the old generation's units to go, the sweep deleted only %d: %v", p.DeletedUnits, p.Delete)
	}
	executeForTest(t, target, p)
	expectFresh(t, ws, "build", "--all-targets")
	expectFresh(t, ws, "test", "--no-run")
}

// sweepAsIfLongAfter plans the sweep 31 days from now with the seed at now: the seed rule,
// the incremental window, and the fingerprint window have all expired.
func sweepAsIfLongAfter(t *testing.T, target string) Plan {
	t.Helper()
	files, err := readStreams(target)
	if err != nil {
		t.Fatal(err)
	}
	live, _ := assembleLive(files, Lanes)
	if live.Problem != "" {
		t.Fatalf("the recorded streams aren't sound: %s", live.Problem)
	}
	l, err := list(target)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Now()
	return plan(l, live, Policy{Now: now.Add(31 * 24 * time.Hour), Seed: now.Add(time.Minute), SeedKnown: true})
}

func executeForTest(t *testing.T, target string, p Plan) {
	t.Helper()
	if p.Reset {
		t.Fatal("a tiny workspace can't be over budget")
	}
	for _, rel := range p.Delete {
		if err := removeUnder(target, rel); err != nil {
			t.Fatal(err)
		}
	}
}

// isDocFingerprint says whether a planned deletion is a rustdoc unit's fingerprint: the
// one unit kind whose stream names no hash (its output is `doc/<crate>/`), and the reason
// `FingerprintWindow` exists.
func isDocFingerprint(t *testing.T, target, rel string) bool {
	t.Helper()
	if !strings.HasPrefix(rel, "debug/.fingerprint/") {
		return false
	}
	entries, err := os.ReadDir(filepath.Join(target, filepath.FromSlash(rel)))
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), "doc-") {
			return true
		}
	}
	return false
}

// record runs a cargo command with the lanes' message format and saves its stream the way
// a lane does: as the lane's complete stream when the build finished.
func record(t *testing.T, ws, lane string, args ...string) {
	t.Helper()
	out := cargo(t, ws, append(args, "--message-format=json-render-diagnostics")...)
	dir := filepath.Join(ws, "target", LiveDir)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, CompleteStream(lane)), out, 0o644); err != nil {
		t.Fatal(err)
	}
}

// expectFresh reruns a build and fails on any unit cargo had to rebuild.
func expectFresh(t *testing.T, ws string, args ...string) {
	t.Helper()
	out := cargo(t, ws, append(args, "--message-format=json")...)
	for _, line := range bytes.Split(out, []byte("\n")) {
		var msg struct {
			Reason string `json:"reason"`
			Fresh  bool   `json:"fresh"`
			Target struct {
				Name string `json:"name"`
			} `json:"target"`
		}
		if json.Unmarshal(line, &msg) != nil || msg.Reason != "compiler-artifact" {
			continue
		}
		if !msg.Fresh {
			t.Errorf("cargo %s rebuilt %s after the sweep", strings.Join(args, " "), msg.Target.Name)
		}
	}
}

func cargo(t *testing.T, ws string, args ...string) []byte {
	t.Helper()
	cmd := exec.Command("cargo", append(args, "--offline", "--quiet")...)
	cmd.Dir = ws
	cmd.Env = append(os.Environ(), "CARGO_TARGET_DIR="+filepath.Join(ws, "target"), "CARGO_INCREMENTAL=1")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		t.Fatalf("cargo %s: %v\n%s", strings.Join(args, " "), err, stderr.String())
	}
	return out
}

var channelRE = regexp.MustCompile(`(?m)^\s*channel\s*=\s*"([^"]+)"`)

// pinnedChannel reads the repo's toolchain channel, skipping when rustup doesn't have it.
func pinnedChannel(t *testing.T) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "..", "..", "rust-toolchain.toml"))
	if err != nil {
		t.Fatal(err)
	}
	m := channelRE.FindSubmatch(data)
	if m == nil {
		t.Fatal("rust-toolchain.toml has no channel")
	}
	channel := string(m[1])
	out, err := exec.Command("rustup", "toolchain", "list").Output()
	if err != nil {
		t.Skipf("no rustup here (%v); the layout pin needs the pinned toolchain", err)
	}
	for _, line := range strings.Split(string(out), "\n") {
		if strings.HasPrefix(line, channel+"-") || line == channel {
			return channel
		}
	}
	t.Skipf("the pinned toolchain %s isn't installed here; the layout pin needs it", channel)
	return ""
}

// tinyWorkspace writes a two-crate workspace: `app` (lib, bin, example, build script,
// integration test) on a path dependency `dep`.
func tinyWorkspace(t *testing.T, channel string) string {
	t.Helper()
	ws := t.TempDir()
	files := map[string]string{
		"rust-toolchain.toml":  "[toolchain]\nchannel = \"" + channel + "\"\n",
		"Cargo.toml":           "[workspace]\nmembers = [\"app\", \"dep\"]\nresolver = \"3\"\n",
		"dep/Cargo.toml":       "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
		"dep/src/lib.rs":       "/// Doubles.\npub fn double(x: u32) -> u32 { x * 2 }\n",
		"app/Cargo.toml":       "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
		"app/build.rs":         "fn main() { println!(\"cargo::rerun-if-changed=build.rs\"); }\n",
		"app/src/lib.rs":       "/// Quadruples.\npub fn quad(x: u32) -> u32 { dep::double(dep::double(x)) }\n",
		"app/src/main.rs":      "fn main() { println!(\"{}\", app::quad(1)); }\n",
		"app/examples/demo.rs": "fn main() { println!(\"{}\", app::quad(2)); }\n",
		"app/tests/it.rs":      "#[test]\nfn quads() { assert_eq!(app::quad(1), 4); }\n",
	}
	for rel, content := range files {
		full := filepath.Join(ws, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return ws
}

func bumpVersion(t *testing.T, manifest string) {
	t.Helper()
	data, err := os.ReadFile(manifest)
	if err != nil {
		t.Fatal(err)
	}
	bumped := strings.Replace(string(data), `version = "0.1.0"`, `version = "0.2.0"`, 1)
	if bumped == string(data) {
		t.Fatal("found no version to bump")
	}
	if err := os.WriteFile(manifest, []byte(bumped), 0o644); err != nil {
		t.Fatal(err)
	}
}
