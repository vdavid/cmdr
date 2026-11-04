package main

import (
	"reflect"
	"strings"
	"testing"
)

func TestLoadConfigFindsLanesAndTheirDefinitions(t *testing.T) {
	c := mustConfig(t, newMemTree(fixtureFiles()))
	if c.workflow != ".github/workflows/ci.yml" || c.changesJob != "changes" || c.quant != quantSomeWithExcludes {
		t.Fatalf("config = %s / %s / %s", c.workflow, c.changesJob, c.quant)
	}
	ci := ".github/workflows/ci.yml"
	want := []lane{
		// A prerequisite job builds part of what the lane tests, so its action is the lane's input.
		{id: "desktop-e2e", gate: []string{"svelte"}, filters: []string{"svelte"}, defFiles: []string{ci},
			defDirs: []string{".github/actions/base-image"}},
		{id: "desktop-rust", gate: []string{"rust"}, filters: []string{"rust"}, defFiles: []string{ci},
			defDirs: []string{".github/actions/pinned-rust-toolchain", ".github/actions/shared-helper"}},
		// The sub-gate's filter (`with:`) counts as an input, and the reusable workflow and the
		// actions it reaches are its definition. Its gate stays the `if:` alone: a Finder-only
		// change starts no macOS run, and that skip is an ordinary path-filter one.
		{id: "desktop-rust-macos", gate: []string{"rust"}, filters: []string{"finder-extension", "rust"},
			defFiles: []string{ci, ".github/workflows/macos-lane.yml"},
			defDirs:  []string{".github/actions/mac-only", ".github/actions/pinned-rust-toolchain", ".github/actions/shared-helper"}},
	}
	// `warm-rust-cache` reads a filter but doesn't opt in, and `e2e-base-image` only follows
	// `desktop-e2e`'s verdict, so neither is a lane.
	if !reflect.DeepEqual(c.lanes, want) {
		t.Errorf("lanes:\n got  %+v\n want %+v", c.lanes, want)
	}
}

func TestLaneInputsAreExactlyWhatTheLaneReads(t *testing.T) {
	got := inputPaths(t, fixtureFiles(), "desktop-rust")
	want := strings.Join([]string{
		".github/actions/pinned-rust-toolchain/action.yml",
		".github/actions/shared-helper/action.yml",
		".github/actions/shared-helper/script.sh",
		".github/workflows/ci.yml",
		".mise.toml",
		"Cargo.lock",
		"apps/desktop/src-tauri/src/agent/prompts/system.md",
		"apps/desktop/src-tauri/src/main.rs",
		"crates/cmdr-fs/src/lib.rs",
		"rust-toolchain.toml",
		"scripts/check/main.go",
	}, "\n")
	if got != want {
		t.Errorf("desktop-rust inputs:\n%s\nwant:\n%s", got, want)
	}
}

func TestLaneHashMovesExactlyWithItsInputs(t *testing.T) {
	base := hashOf(t, fixtureFiles(), "desktop-rust")
	edit := func(name string, change func(map[string]string), wantSame bool) {
		t.Helper()
		files := fixtureFiles()
		change(files)
		if same := hashOf(t, files, "desktop-rust") == base; same != wantSame {
			t.Errorf("%s: hash unchanged = %v, want %v", name, same, wantSame)
		}
	}
	// What the lane never reads leaves its hash alone: the motivating docs fix.
	edit("agent docs", func(f map[string]string) { f["apps/desktop/src-tauri/CLAUDE.md"] += "more\n" }, true)
	edit("check-runner agent docs", func(f map[string]string) { f["scripts/check/CLAUDE.md"] += "more\n" }, true)
	edit("other docs", func(f map[string]string) { f["docs/tooling/ci.md"] += "more\n" }, true)
	edit("frontend", func(f map[string]string) { f["apps/desktop/src/App.svelte"] += "<p/>\n" }, true)
	edit("another workflow", func(f map[string]string) { f[".github/workflows/other.yml"] += "# x\n" }, true)
	edit("an action it doesn't use", func(f map[string]string) { f[".github/actions/unused/action.yml"] += "# x\n" }, true)
	// Everything it reads moves it.
	edit("source", func(f map[string]string) { f["apps/desktop/src-tauri/src/main.rs"] += "// x\n" }, false)
	edit("a prompt (non-agent .md)", func(f map[string]string) { f["apps/desktop/src-tauri/src/agent/prompts/system.md"] += "x\n" }, false)
	edit("new file", func(f map[string]string) { f["crates/cmdr-fs/src/new.rs"] = "" }, false)
	edit("deleted file", func(f map[string]string) { delete(f, "crates/cmdr-fs/src/lib.rs") }, false)
	edit("renamed file", func(f map[string]string) {
		f["crates/cmdr-fs/src/lib2.rs"] = f["crates/cmdr-fs/src/lib.rs"]
		delete(f, "crates/cmdr-fs/src/lib.rs")
	}, false)
	edit("its workflow", func(f map[string]string) { f[".github/workflows/ci.yml"] += "# x\n" }, false)
	edit("its action", func(f map[string]string) { f[".github/actions/pinned-rust-toolchain/action.yml"] += "# x\n" }, false)
	edit("a nested action's script", func(f map[string]string) { f[".github/actions/shared-helper/script.sh"] += "x\n" }, false)
	edit("toolchain pin", func(f map[string]string) { f[".mise.toml"] += "node = \"24\"\n" }, false)
	edit("rust pin", func(f map[string]string) { f["rust-toolchain.toml"] += "# x\n" }, false)
	edit("check runner", func(f map[string]string) { f["scripts/check/main.go"] += "// x\n" }, false)

	tr := newMemTree(fixtureFiles())
	tr.modes["apps/desktop/src-tauri/src/main.rs"] = "100755"
	c := mustConfig(t, tr)
	if laneHash("desktop-rust", laneInputs(tr, c, laneByID(t, c, "desktop-rust"))) == base {
		t.Error("an executable-bit flip didn't move the hash")
	}
}

func TestReusableLaneHashFollowsItsOwnFile(t *testing.T) {
	base := hashOf(t, fixtureFiles(), "desktop-rust-macos")
	files := fixtureFiles()
	files[".github/workflows/macos-lane.yml"] += "# x\n"
	if hashOf(t, files, "desktop-rust-macos") == base {
		t.Error("editing macos-lane.yml didn't move the macOS lane's hash")
	}
	files = fixtureFiles()
	files["apps/desktop/finder-extension/Ext.swift"] += "// x\n"
	if hashOf(t, files, "desktop-rust-macos") == base {
		t.Error("editing the Finder extension didn't move the macOS lane's hash")
	}
	if hashOf(t, files, "desktop-rust") != hashOf(t, fixtureFiles(), "desktop-rust") {
		t.Error("the Finder extension moved the Linux lane, which doesn't read it")
	}
	// Same inputs, different lane: different hash, so one lane's marker never covers another.
	if hashOf(t, fixtureFiles(), "desktop-rust") == base {
		t.Error("two lanes hash alike")
	}
}

func TestLoadConfigRefusesBrokenWiring(t *testing.T) {
	cases := map[string]func(map[string]string){
		"a job saving another lane's marker": func(f map[string]string) {
			f[".github/workflows/ci.yml"] = strings.Replace(f[".github/workflows/ci.yml"],
				"lanes)['desktop-rust-macos'].key", "lanes)['desktop-rust'].key", 1)
		},
		"a verdict read of a job that isn't a lane": func(f map[string]string) {
			f[".github/workflows/ci.yml"] = strings.Replace(f[".github/workflows/ci.yml"],
				"lanes)['desktop-e2e'].verified", "lanes)['desktop-e2e-typo'].verified", 1)
		},
		"an unknown plan field": func(f map[string]string) {
			f[".github/workflows/ci.yml"] = strings.Replace(f[".github/workflows/ci.yml"],
				"lanes)['desktop-e2e'].verified", "lanes)['desktop-e2e'].verfied", 1)
		},
		"a lane whose if: reads no filter": func(f map[string]string) {
			f[".github/workflows/ci.yml"] = strings.Replace(f[".github/workflows/ci.yml"],
				"needs.changes.outputs.rust == 'true' && !fromJSON(needs.changes.outputs.lanes)['desktop-rust']",
				"!fromJSON(needs.changes.outputs.lanes)['desktop-rust']", 1)
		},
		"a lane needing a missing job": func(f map[string]string) {
			f[".github/workflows/ci.yml"] = strings.Replace(f[".github/workflows/ci.yml"],
				"needs: [changes, e2e-base-image]", "needs: [changes, e2e-base-imag]", 1)
		},
		"a missing local action": func(f map[string]string) {
			delete(f, ".github/actions/mac-only/action.yml")
		},
		"an unsupported glob": func(f map[string]string) {
			f[".github/workflows/ci.yml"] = strings.Replace(f[".github/workflows/ci.yml"], "'crates/**'", "'crates/{a,b}/**'", 1)
		},
		"a second dorny step in a changes job": func(f map[string]string) {
			f[".github/workflows/other.yml"] = "jobs:\n  changes:\n    steps:\n      - uses: dorny/paths-filter@x\n        with:\n          filters: |\n            a:\n              - 'a/**'\n"
		},
	}
	for name, change := range cases {
		files := fixtureFiles()
		change(files)
		if _, err := loadConfig(newMemTree(files)); err == nil {
			t.Errorf("%s: loadConfig accepted it", name)
		}
	}
}

// The real ci.yml must parse, with every pattern inside our supported subset, so a filter edit
// that dorny would read differently fails this test instead of hashing the wrong set.
func realConfig(t *testing.T) (*gitTree, *config) {
	t.Helper()
	tr, err := loadGitTree("../..", "HEAD")
	if err != nil {
		t.Skipf("no git tree: %v", err)
	}
	return tr, mustConfig(t, tr)
}

func TestRealWorkflowsParse(t *testing.T) {
	tr, c := realConfig(t)
	if c.workflow != ".github/workflows/ci.yml" || c.changesJob != "changes" || c.quant != quantSomeWithExcludes {
		t.Fatalf("config = %s / %s / %s", c.workflow, c.changesJob, c.quant)
	}
	rust := c.filters["rust"]
	for p, want := range map[string]bool{
		"Cargo.lock":                           true,
		"apps/desktop/src/lib/ipc/bindings.ts": true,
		"apps/desktop/src-tauri/CLAUDE.md":     false,
		"apps/desktop/src/App.svelte":          false,
		"docs/tooling/ci.md":                   false,
	} {
		if got := rust.matches(p, c.quant); got != want {
			t.Errorf("real rust filter on %s = %v, want %v", p, got, want)
		}
	}
	for _, l := range c.lanes {
		if len(laneInputs(tr, c, l)) == 0 {
			t.Errorf("lane %s has no inputs", l.id)
		}
	}
}

// Opts three real jobs in the way the wiring will, and checks their real input sets.
func TestRealLaneInputs(t *testing.T) {
	tr, c := realConfig(t)
	ci, err := tr.read(c.workflow)
	if err != nil {
		t.Fatal(err)
	}
	wired := string(ci)
	for _, id := range []string{"desktop-rust", "desktop-rust-macos", "desktop-e2e-linux"} {
		wired = optIn(t, wired, id)
	}
	ov := overlayTree{tree: tr, path: c.workflow, content: wired}
	wc := mustConfig(t, ov)
	has := func(laneID, p string) bool {
		for _, e := range laneInputs(ov, wc, laneByID(t, wc, laneID)) {
			if e.path == p {
				return true
			}
		}
		return false
	}
	for _, tc := range []struct {
		lane, path string
		want       bool
	}{
		{"desktop-rust", "Cargo.lock", true},
		{"desktop-rust", ".github/actions/pinned-rust-toolchain/action.yml", true},
		{"desktop-rust", "apps/desktop/src-tauri/CLAUDE.md", false},
		{"desktop-rust", "docs/tooling/ci.md", false},
		{"desktop-rust", "scripts/check/main.go", true},
		{"desktop-rust", ".github/workflows/macos-lane.yml", false},
		{"desktop-rust-macos", ".github/workflows/macos-lane.yml", true},
		{"desktop-rust-macos", "apps/desktop/finder-extension/CLAUDE.md", false},
		{"desktop-e2e-linux", "apps/desktop/package.json", true},
		{"desktop-e2e-linux", "apps/website/package.json", false},
	} {
		if got := has(tc.lane, tc.path); got != tc.want {
			t.Errorf("real %s reads %s = %v, want %v", tc.lane, tc.path, got, tc.want)
		}
	}
}

// optIn appends the plan check to a real job's `if:`, the shape the wiring adds.
func optIn(t *testing.T, ci, jobID string) string {
	t.Helper()
	start := strings.Index(ci, "\n  "+jobID+":\n")
	if start < 0 {
		t.Fatalf("no job %s in ci.yml", jobID)
	}
	ifAt := strings.Index(ci[start:], "\n    if: ")
	if ifAt < 0 {
		t.Fatalf("job %s has no one-line if:", jobID)
	}
	lineStart := start + ifAt + 1
	lineEnd := lineStart + strings.Index(ci[lineStart:], "\n")
	cond := strings.TrimPrefix(ci[lineStart:lineEnd], "    if: ")
	// The `.key` read stands in for the marker save the wiring adds as the job's last step.
	plan := "fromJSON(needs.changes.outputs.lanes)['" + jobID + "']"
	line := "    if: (" + cond + ") && !" + plan + ".verified && " + plan + ".key != ''"
	return ci[:lineStart] + line + ci[lineEnd:]
}

// overlayTree is a real tree with one file's content replaced.
type overlayTree struct {
	tree
	path, content string
}

func (o overlayTree) entries() []entry {
	var out []entry
	for _, e := range o.tree.entries() {
		if e.path == o.path {
			e.object = "overlay"
		}
		out = append(out, e)
	}
	return out
}

func (o overlayTree) read(p string) ([]byte, error) {
	if p == o.path {
		return []byte(o.content), nil
	}
	return o.tree.read(p)
}
