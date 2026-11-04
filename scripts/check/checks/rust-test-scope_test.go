package checks

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"
)

// ── The scope value ───────────────────────────────────────────────────────────

func TestRustTestScopeZeroValueRunsEverything(t *testing.T) {
	var scope RustTestScope
	if scope.Narrowed || scope.selectsNothing() || scope.nextestFilter() != "" || scope.String() != "all" {
		t.Fatalf("the zero scope must run everything, got %+v (%q)", scope, scope.String())
	}
}

func TestParseRustTestScope(t *testing.T) {
	members := []string{"cmdr", "cmdr-fs", "cmdr-webdav"}
	cases := []struct {
		raw  string
		want RustTestScope
	}{
		{"", RustTestScope{}},
		{"  ", RustTestScope{}},
		{"all", RustTestScope{}},
		{"none", RustTestScope{Narrowed: true}},
		{"cmdr-webdav cmdr", RustTestScope{Narrowed: true, Packages: []string{"cmdr", "cmdr-webdav"}}},
		{"cmdr-fs,cmdr-fs\ncmdr", RustTestScope{Narrowed: true, Packages: []string{"cmdr", "cmdr-fs"}}},
	}
	for _, c := range cases {
		got, err := parseRustTestScope(c.raw, members)
		if err != nil {
			t.Errorf("parse(%q): %v", c.raw, err)
			continue
		}
		if got.Narrowed != c.want.Narrowed || !slices.Equal(got.Packages, c.want.Packages) {
			t.Errorf("parse(%q) = %+v, want %+v", c.raw, got, c.want)
		}
	}
}

func TestParseRustTestScopeRefusesWhatItCantHonor(t *testing.T) {
	for _, raw := range []string{"cmdr cmdr-typo", "all cmdr", "none cmdr"} {
		if _, err := parseRustTestScope(raw, []string{"cmdr"}); err == nil {
			t.Errorf("parse(%q) accepted a value that would silently drop or mix crates", raw)
		}
	}
}

func TestScopedNextestFilterIntersectsTheLaneFilter(t *testing.T) {
	scope := RustTestScope{Narrowed: true, Packages: []string{"cmdr", "cmdr-fs"}}
	if got := scopedNextestFilter("", RustTestScope{}); got != "" {
		t.Errorf("unscoped, no lane filter: %q", got)
	}
	if got := scopedNextestFilter("test(a) + test(b)", RustTestScope{}); got != "test(a) + test(b)" {
		t.Errorf("unscoped must leave the lane filter alone: %q", got)
	}
	if got, want := scopedNextestFilter("", scope), "package(=cmdr) + package(=cmdr-fs)"; got != want {
		t.Errorf("scoped, no lane filter: %q, want %q", got, want)
	}
	if got, want := scopedNextestFilter("test(a) + test(b)", scope), "(test(a) + test(b)) & (package(=cmdr) + package(=cmdr-fs))"; got != want {
		t.Errorf("scoped lane filter: %q, want %q", got, want)
	}
}

func TestRustLaneNotRunSkipsOnlyAnEmptyScope(t *testing.T) {
	result, err := rustLaneNotRun(errRustTestScopeSelectsNothing)
	if err != nil || result.Code != ResultSkipped {
		t.Errorf("an empty scope must skip, got %+v, %v", result, err)
	}
	buildErr := errors.New("couldn't build the Rust tests")
	if _, err := rustLaneNotRun(buildErr); !errors.Is(err, buildErr) {
		t.Errorf("a build error must come back as the error it was, got %v", err)
	}
}

func TestScopedLaneToleratesSelectingNoTests(t *testing.T) {
	lane, _ := recordingLane(nextestRunOptions{})
	if slices.Contains(lane.runArgv("", ""), "--no-tests") {
		t.Error("an unscoped run must keep nextest's no-tests failure")
	}
	lane.scope = RustTestScope{Narrowed: true, Packages: []string{"cmdr"}}
	if got := valueAfter(lane.runArgv("", "package(=cmdr)"), "--no-tests"); got != "warn" {
		t.Errorf("a scoped run must pass --no-tests warn, got %q", got)
	}
}

// ── The graph ─────────────────────────────────────────────────────────────────

// A three-crate workspace: `app` depends on `lib`, `tool` dev-depends on `lib`,
// and `leaf` stands alone.
const fakeMetadata = `{
  "workspace_root": "/repo",
  "packages": [
    {"name": "app", "manifest_path": "/repo/apps/app/Cargo.toml",
     "dependencies": [{"name": "lib", "path": "/repo/crates/lib", "kind": null}, {"name": "serde"}]},
    {"name": "lib", "manifest_path": "/repo/crates/lib/Cargo.toml", "dependencies": []},
    {"name": "tool", "manifest_path": "/repo/crates/tool/Cargo.toml",
     "dependencies": [{"name": "lib", "path": "/repo/crates/lib", "kind": "dev"}]},
    {"name": "leaf", "manifest_path": "/repo/crates/leaf/Cargo.toml", "dependencies": []}
  ]
}`

func fakeGraph(t *testing.T) rustCrateGraph {
	t.Helper()
	graph, err := parseRustCrateGraph([]byte(fakeMetadata))
	if err != nil {
		t.Fatalf("parseRustCrateGraph: %v", err)
	}
	return graph
}

var fakeRules = []rustScopeRule{
	{Glob: "**/CLAUDE.md", Effect: scopeNothing},
	{Glob: "Cargo.lock", Effect: scopeAll},
	{Glob: "shared/table.json", Effect: scopeReaders, Readers: []string{"leaf"}},
	{Glob: "crates/lib/src/data.rs", Effect: scopeReaders, Readers: []string{"leaf"}},
	{Glob: "crates/**", Effect: scopeOwner},
	{Glob: "apps/app/**", Effect: scopeOwner},
	{Glob: "docs/**", Effect: scopeNothing},
}

func TestAffectedRustScope(t *testing.T) {
	graph := fakeGraph(t)
	cases := []struct {
		name    string
		changed []string
		all     bool
		want    []string
	}{
		{"a dependency change reaches its dependents, dev ones included", []string{"crates/lib/src/x.rs"}, false, []string{"app", "lib", "tool"}},
		{"a top crate affects itself alone", []string{"apps/app/src/main.rs"}, false, []string{"app"}},
		{"a leaf affects itself alone", []string{"crates/leaf/src/lib.rs"}, false, []string{"leaf"}},
		{"a read file seeds its reader", []string{"shared/table.json"}, false, []string{"leaf"}},
		{"a read file inside a member seeds both", []string{"crates/lib/src/data.rs"}, false, []string{"app", "leaf", "lib", "tool"}},
		{"docs affect nothing", []string{"docs/a.md", "crates/lib/CLAUDE.md"}, false, nil},
		{"no change affects nothing", nil, false, nil},
		{"a valve tests everything", []string{"crates/leaf/src/lib.rs", "Cargo.lock"}, true, nil},
		{"an unknown path tests everything", []string{"crates/leaf/src/lib.rs", "somewhere/new.txt"}, true, nil},
		{"a path in no member under an owner glob tests everything", []string{"crates/not-a-member/src/lib.rs"}, true, nil},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			scope, _ := affectedRustScope(graph, fakeRules, c.changed)
			if c.all {
				if scope.Narrowed {
					t.Fatalf("want all, got %v", scope.Packages)
				}
				return
			}
			if !scope.Narrowed || !slices.Equal(scope.Packages, c.want) {
				t.Fatalf("got %+v, want %v", scope, c.want)
			}
		})
	}
}

func TestDependentClosureSurvivesACycle(t *testing.T) {
	graph := rustCrateGraph{
		dirs:       map[string]string{"a": "crates/a", "b": "crates/b"},
		dependents: map[string][]string{"a": {"b"}, "b": {"a"}},
	}
	if got := graph.closeOverDependents(map[string]bool{"a": true}); !slices.Equal(got, []string{"a", "b"}) {
		t.Fatalf("got %v", got)
	}
}

// TestRustTestScopeOnTheRealWorkspace asks real cargo for the real graph. The
// motivating case: a `cmdr-webdav` change must not run `cmdr-index`'s tests, and
// must run the app's.
func TestRustTestScopeOnTheRealWorkspace(t *testing.T) {
	if _, err := exec.LookPath("cargo"); err != nil {
		t.Skip("no cargo on PATH")
	}
	root := repoRootForTest(t)
	graph, err := loadRustCrateGraph(root)
	if err != nil {
		t.Fatalf("loadRustCrateGraph: %v", err)
	}
	scope, _ := affectedRustScope(graph, rustTestScopeRules, []string{"crates/cmdr-webdav/src/lib.rs"})
	if !scope.Narrowed {
		t.Fatal("a cmdr-webdav source change tested everything")
	}
	if !scope.includes("cmdr-webdav") || !scope.includes("cmdr") {
		t.Errorf("a cmdr-webdav change must test cmdr-webdav and the app, got %v", scope.Packages)
	}
	if scope.includes("cmdr-index") {
		t.Errorf("a cmdr-webdav change must not test cmdr-index, got %v", scope.Packages)
	}

	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	if len(graph.dirs) != len(members) {
		t.Errorf("cargo metadata lists %d members, the manifests %d", len(graph.dirs), len(members))
	}
	for _, m := range members {
		if graph.dirs[m.Name] != m.RelDir(root) {
			t.Errorf("cargo places %s at %q, the manifests at %q", m.Name, graph.dirs[m.Name], m.RelDir(root))
		}
	}
}

// ── The rule table against the tree ───────────────────────────────────────────

// realGraphDirsOnly is the real workspace's member dirs with no edges, enough to
// classify a path without asking cargo.
func realGraphDirsOnly(t *testing.T, root string) (rustCrateGraph, []WorkspaceMember) {
	t.Helper()
	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	graph := rustCrateGraph{dirs: map[string]string{}, dependents: map[string][]string{}}
	for _, m := range members {
		graph.dirs[m.Name] = m.RelDir(root)
	}
	return graph, members
}

// ruleProbe is a path the rule's glob matches, for asking the table about it.
func ruleProbe(glob string) string {
	probe := strings.ReplaceAll(glob, "**/", "probe-dir/")
	probe = strings.TrimSuffix(probe, "/**") + map[bool]string{true: "/probe-dir/probe.rs", false: ""}[strings.HasSuffix(glob, "/**")]
	return strings.ReplaceAll(probe, "*", "probe")
}

// TestEveryRustScopeRuleIsReachable fails on a rule an earlier, broader one
// shadows: dead data that reads as if it decides something.
func TestEveryRustScopeRuleIsReachable(t *testing.T) {
	for i, rule := range rustTestScopeRules {
		probe := ruleProbe(rule.Glob)
		if !matchGlob(rule.Glob, probe) {
			t.Fatalf("probe %q doesn't match its own rule %q; fix ruleProbe", probe, rule.Glob)
		}
		got := matchRustScopeRule(rustTestScopeRules, probe)
		if got == nil {
			t.Fatalf("rule %q matched nothing for its own probe %q", rule.Glob, probe)
		}
		if got != &rustTestScopeRules[i] {
			t.Errorf("rule %q is shadowed by the earlier %q", rule.Glob, got.Glob)
		}
	}
}

func TestRustScopeReadersAreWorkspaceMembers(t *testing.T) {
	graph, _ := realGraphDirsOnly(t, repoRootForTest(t))
	for _, rule := range rustTestScopeRules {
		if (rule.Effect == scopeReaders) != (len(rule.Readers) > 0) {
			t.Errorf("rule %q: readers belong on scopeReaders rules, and every one names at least one", rule.Glob)
		}
		for _, r := range rule.Readers {
			if _, ok := graph.dirs[r]; !ok {
				t.Errorf("rule %q names reader %q, which is no workspace member", rule.Glob, r)
			}
		}
	}
}

// TestEveryWorkspaceMemberIsOwned fails on a member whose tree no owner rule
// covers: its own edits would seed nothing and its tests would never run.
func TestEveryWorkspaceMemberIsOwned(t *testing.T) {
	root := repoRootForTest(t)
	graph, members := realGraphDirsOnly(t, root)
	for _, m := range members {
		probe := m.RelDir(root) + "/src/lib.rs"
		d := classifyRustScopePath(graph, rustTestScopeRules, probe)
		if d.All || !slices.Contains(d.Seeds, m.Name) {
			t.Errorf("an edit to %s doesn't seed %s (got all=%v seeds=%v): give its tree a scopeOwner rule", probe, m.Name, d.All, d.Seeds)
		}
	}
}

// rustFilterPaths returns the include patterns of one dorny/paths-filter block in
// ci.yml (`rust:`), dropping the `!` exclusions.
func rustFilterPaths(t *testing.T, root, name string) []string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(root, ".github", "workflows", "ci.yml"))
	if err != nil {
		t.Fatalf("reading ci.yml: %v", err)
	}
	var paths []string
	in, indent := false, 0
	for _, line := range strings.Split(string(raw), "\n") {
		trimmed := strings.TrimLeft(line, " ")
		lineIndent := len(line) - len(trimmed)
		if !in {
			if trimmed == name+":" {
				in, indent = true, lineIndent
			}
			continue
		}
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		if lineIndent <= indent {
			break
		}
		if m := filterPathRe.FindStringSubmatch(line); m != nil && !strings.HasPrefix(m[1], "!") {
			paths = append(paths, m[1])
		}
	}
	if len(paths) == 0 {
		t.Fatalf("found no `%s` filter in ci.yml; the parse is broken, not the workflow", name)
	}
	return paths
}

// TestEveryRustFilterPathIsClassified ties the table to ci.yml's `rust` filter,
// which decides whether the Rust lanes run at all: every path that can start a
// Rust lane has a deliberate answer here, not the "all" an unknown path gets.
func TestEveryRustFilterPathIsClassified(t *testing.T) {
	root := repoRootForTest(t)
	for _, pattern := range rustFilterPaths(t, root, "rust") {
		if matchRustScopeRule(rustTestScopeRules, ruleProbe(pattern)) == nil {
			t.Errorf("ci.yml's `rust` filter lists %q, which no `rustTestScopeRules` entry classifies", pattern)
		}
	}
}

// rustFilterGaps are the paths the scope says can change a Rust test's outcome,
// yet ci.yml's `rust` filter doesn't list, so a change to them ALONE doesn't run
// the Rust lanes at all. Each was a gap before the scope existed.
// `TestRustScopeInputsAreInTheRustFilter` fails on a new gap and on a closed one,
// so add the path to the filter and delete its entry here.
var rustFilterGaps = map[string]string{
	"vendor/**":                        "patched crates swapped into every build; a vendor-only patch bump skips the Rust lanes",
	".github/workflows/macos-lane.yml": "in the `finder-extension` filter only; a macOS-lane-only edit skips the Rust lanes",
	".github/actions/**":               "the pinned-toolchain action the Rust lanes use",
	"scripts/check/**":                 "the runner's own edits run the Go lanes (`scripts` filter), never the Rust lanes they change",
	"scripts/check.sh":                 "same as `scripts/check/**`",
	"CHANGELOG.md":                     "`whats_new` embeds it; a changelog-only edit skips the Rust lanes",
	"apps/desktop/src/lib/commands/command-ids.ts":              "`build.rs` and a drift test read it",
	"apps/desktop/src/lib/logging/bridge-redaction-cases.json":  "the bridge redaction test embeds it",
	"apps/desktop/src/lib/logging/log-markers.json":             "the bridge redaction test embeds it",
	"apps/desktop/src/lib/units/drive-figure-cases.json":        "the space poller's tests embed it",
	"apps/desktop/src/lib/units/free-percent-cases.json":        "the space poller's tests embed it",
	"apps/desktop/src/lib/intl/messages/**":                     "the native-string resolver's tests read it",
	"apps/desktop/src/lib/ask-cmdr/ask-cmdr-tool-label-keys.ts": "the agent view tests read it",
	"apps/desktop/test/fixtures/encodings/**":                   "the encoding-inspect tests read it",
	"apps/desktop/scripts/download-llama-server.go":             "the app's `build.rs` reruns on it",
}

// TestRustScopeInputsAreInTheRustFilter is the other direction: a path the scope
// tests something for has to be able to start the lanes, or a change to it alone
// tests nothing. Known gaps live in `rustFilterGaps`, exactly.
func TestRustScopeInputsAreInTheRustFilter(t *testing.T) {
	root := repoRootForTest(t)
	filter := rustFilterPaths(t, root, "rust")
	for _, rule := range rustTestScopeRules {
		if rule.Effect == scopeNothing {
			continue
		}
		inFilter := matchesAny(ruleProbe(rule.Glob), filter)
		_, known := rustFilterGaps[rule.Glob]
		switch {
		case !inFilter && !known:
			t.Errorf("%q can change a Rust test's outcome, but ci.yml's `rust` filter doesn't list it: a change to it alone runs no Rust lane. Add it to the filter.", rule.Glob)
		case inFilter && known:
			t.Errorf("%q is in ci.yml's `rust` filter now; drop its `rustFilterGaps` entry", rule.Glob)
		}
	}
	for glob := range rustFilterGaps {
		if !slices.ContainsFunc(rustTestScopeRules, func(r rustScopeRule) bool { return r.Glob == glob }) {
			t.Errorf("`rustFilterGaps` lists %q, which no rule has", glob)
		}
	}
}

// ── The out-of-crate reads ────────────────────────────────────────────────────

// rustStringLiteralRE finds the plain string literals in a Rust source. Raw
// strings and multi-line literals go unseen, which `rustScopeScanFloor` guards.
var rustStringLiteralRE = regexp.MustCompile(`"((?:[^"\\\n]|\\.)*)"`)

// outOfCrateRead is one path a member's source names outside its own tree.
type outOfCrateRead struct {
	reader  string // the member whose source names it
	source  string // repo-relative source file
	literal string // the literal as written
	path    string // repo-relative path it resolves to
	isDir   bool
}

// rustScopeNotReads are literals the scan resolves to a real path outside the
// crate that the code doesn't read, keyed by "source|literal".
var rustScopeNotReads = map[string]string{}

// collectOutOfCrateReads scans every member's `.rs` files for literals that name
// a real path outside that member: one with a `..` segment, resolved against the
// source's dir (`include_str!`) and the member's dir (`CARGO_MANIFEST_DIR`, a
// build script's cwd, a test's cwd), or a repo-root-relative file path.
func collectOutOfCrateReads(t *testing.T, root string, members []WorkspaceMember) ([]outOfCrateRead, int) {
	t.Helper()
	var reads []outOfCrateRead
	scanned := 0
	for _, m := range members {
		memberRel := m.RelDir(root)
		err := filepath.WalkDir(m.Dir, func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() {
				if name := entry.Name(); name == "target" || name == "node_modules" || (strings.HasPrefix(name, ".") && path != m.Dir) {
					return filepath.SkipDir
				}
				return nil
			}
			if !strings.HasSuffix(path, ".rs") {
				return nil
			}
			scanned++
			found, readErr := outOfCrateReadsIn(t, root, m, memberRel, path)
			reads = append(reads, found...)
			return readErr
		})
		if err != nil {
			t.Fatalf("walking %s: %v", m.Dir, err)
		}
	}
	return reads, scanned
}

// outOfCrateReadsIn is one source file's share of `collectOutOfCrateReads`.
func outOfCrateReadsIn(t *testing.T, root string, m WorkspaceMember, memberRel, path string) ([]outOfCrateRead, error) {
	t.Helper()
	source, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	sourceRel := mustRel(t, root, path)
	var reads []outOfCrateRead
	for _, match := range rustStringLiteralRE.FindAllStringSubmatch(string(source), -1) {
		literal := match[1]
		for _, prefix := range []string{"cargo:rerun-if-changed=", "cargo::rerun-if-changed="} {
			literal = strings.TrimPrefix(literal, prefix)
		}
		if literal == "" || strings.ContainsAny(literal, "{}\\ ") {
			continue
		}
		if _, ok := rustScopeNotReads[sourceRel+"|"+literal]; ok {
			continue
		}
		for _, resolved := range resolveLiteral(root, filepath.Dir(path), m.Dir, literal) {
			rel := mustRel(t, root, resolved)
			if rel == memberRel || strings.HasPrefix(rel, memberRel+"/") {
				continue // inside its own tree
			}
			reads = append(reads, outOfCrateRead{reader: m.Name, source: sourceRel, literal: literal, path: rel, isDir: isDir(resolved)})
		}
	}
	return reads, nil
}

// resolveLiteral returns the existing in-repo paths a literal can name.
func resolveLiteral(root, sourceDir, memberDir, literal string) []string {
	var candidates []string
	segments := strings.Split(strings.TrimLeft(literal, "/"), "/")
	if onlyHops(segments) {
		// A bare `..` / `../..` names a directory to stand in, never a file: a
		// path-traversal case, a `current_dir("..")`. What gets read from there is
		// a further literal, which the scan sees on its own.
		return nil
	}
	if slices.Contains(segments, "..") {
		rel := filepath.FromSlash(strings.TrimLeft(literal, "/"))
		candidates = append(candidates, filepath.Join(sourceDir, rel), filepath.Join(memberDir, rel))
	} else if len(segments) >= 2 && segments[0] != "." && !strings.HasPrefix(literal, "/") {
		// Repo-root-relative, files only: a bare `docs/` is an archive entry
		// name in a dozen tests, never a read of the repo's docs.
		if p := filepath.Join(root, filepath.FromSlash(literal)); isFile(p) {
			candidates = append(candidates, p)
		}
	}
	var out []string
	for _, c := range candidates {
		c = filepath.Clean(c)
		rel, err := filepath.Rel(root, c)
		if err != nil || strings.HasPrefix(rel, "..") || slices.Contains(out, c) {
			continue
		}
		if _, err := os.Stat(c); err == nil {
			out = append(out, c)
		}
	}
	return out
}

func onlyHops(segments []string) bool {
	for _, s := range segments {
		if s != ".." && s != "." && s != "" {
			return false
		}
	}
	return true
}

func isFile(path string) bool {
	info, err := os.Stat(path)
	if err != nil || info == nil {
		return false
	}
	return !info.IsDir()
}

func isDir(path string) bool {
	info, err := os.Stat(path)
	if err != nil || info == nil {
		return false
	}
	return info.IsDir()
}

// manifestDirHopRE finds `CARGO_MANIFEST_DIR` followed, within a few lines, by a
// hop the literal scan can't follow: `.parent()`, `.ancestors()`, or a `PathBuf`
// `.pop();` (a work-stack `while let Some(x) = stack.pop()` has no `;` there).
var manifestDirHopRE = regexp.MustCompile(`CARGO_MANIFEST_DIR(?:[^\n]*\n){0,6}?[^\n]*(?:\.(?:parent|ancestors)\(\)|\.pop\(\);)`)

// rustScopeHopFiles are the sources where `TestNoUnseenManifestDirHops` finds a
// hop, each with why its read needs no rule. A new one fails the test until it
// lands here or its read lands in `rustTestScopeRules`.
var rustScopeHopFiles = map[string]string{
	"apps/desktop/src-tauri/src/operations_host/backend_suites/copy_bench.rs": "climbs out of the repo to the sibling smb2 checkout's `.env`",
	"crates/cmdr-git/src/bench.rs":                                            "locates `target/` for its generated fixture; reads nothing tracked",
	"crates/cmdr-index/src/importance/evals/tests.rs":                         "loads the gitignored importance corpus, which no diff carries",
	"crates/cmdr-operations/src/golden_trace_tests.rs":                        "the `.parent()` creates the dir of an in-crate golden file",
}

// TestNoUnseenManifestDirHops covers the scan's blind spot: a path built by
// climbing from `CARGO_MANIFEST_DIR` with `.parent()` and friends never shows up
// as a literal, so a read through one could escape every rule.
func TestNoUnseenManifestDirHops(t *testing.T) {
	root := repoRootForTest(t)
	_, members := realGraphDirsOnly(t, root)
	found := map[string]bool{}
	for _, m := range members {
		err := filepath.WalkDir(m.Dir, func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() {
				if name := entry.Name(); name == "target" || name == "node_modules" {
					return filepath.SkipDir
				}
				return nil
			}
			if !strings.HasSuffix(path, ".rs") {
				return nil
			}
			source, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			if manifestDirHopRE.Match(source) {
				found[mustRel(t, root, path)] = true
			}
			return nil
		})
		if err != nil {
			t.Fatalf("walking %s: %v", m.Dir, err)
		}
	}
	for file := range found {
		if _, ok := rustScopeHopFiles[file]; !ok {
			t.Errorf("%s climbs out of CARGO_MANIFEST_DIR, which the out-of-crate scan can't follow: classify what it reads in `rustTestScopeRules`, then list it in `rustScopeHopFiles` with why", file)
		}
	}
	for file := range rustScopeHopFiles {
		if !found[file] {
			t.Errorf("`rustScopeHopFiles` lists %s, which no longer climbs out of CARGO_MANIFEST_DIR; drop it", file)
		}
	}
}

// rustScopeScanFloor is a sanity floor on what the scan finds, so a regex that
// quietly stops matching fails here instead of passing on an empty list.
const rustScopeScanFloor = 15

// TestEveryOutOfCrateReadIsClassified is the guard behind the rule table: every
// path outside its own tree that a member's source names must classify to a
// rule that tests THAT member (or everything). A new `include_str!` of a
// frontend file, a fixture read across the tree, or a build script input lands
// here until the table says who reads it.
func TestEveryOutOfCrateReadIsClassified(t *testing.T) {
	root := repoRootForTest(t)
	graph, members := realGraphDirsOnly(t, root)
	reads, scanned := collectOutOfCrateReads(t, root, members)
	if scanned == 0 || len(reads) < rustScopeScanFloor {
		t.Fatalf("scanned %d Rust files and found %d out-of-crate reads (floor %d); the scan is broken, not the tree",
			scanned, len(reads), rustScopeScanFloor)
	}
	for _, r := range reads {
		probe := r.path
		if r.isDir {
			probe += "/probe-dir/probe.txt"
		}
		d := classifyRustScopePath(graph, rustTestScopeRules, probe)
		if d.All {
			if matchRustScopeRule(rustTestScopeRules, probe) == nil {
				t.Errorf("%s reads %s (%q), which no rule classifies (it tests everything today): add a scopeReaders rule naming %s",
					r.source, r.path, r.literal, r.reader)
			}
			continue
		}
		if !slices.Contains(d.Seeds, r.reader) {
			t.Errorf("%s reads %s (%q), but a change there seeds %v, not %s: add %s to the readers of its rule",
				r.source, r.path, r.literal, d.Seeds, r.reader, r.reader)
		}
	}
}
