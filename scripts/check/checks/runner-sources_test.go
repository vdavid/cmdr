package checks

import (
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"runtime"
	"strings"
	"testing"
)

// The runner's own source is split in two: the core every check carries
// (`GlobalInputs`) and the implementation files a check's `Run` reaches
// (`runner-sources.go`). The tests here are the reason that split is allowed to
// be narrow. An input set that's too wide costs cache speed; one that's too
// narrow reports a green describing code it never ran, so every one of these
// checks the narrow direction.

// runnerSourcesForTest analyzes the real tree once per test.
func runnerSourcesForTest(t *testing.T) (string, *RunnerSourceIndex) {
	t.Helper()
	root := repoRootForTest(t)
	idx := LoadRunnerSources(root)
	if idx.Err != nil {
		t.Fatalf("the runner-source analysis gave up on the real tree, so every check falls back to `scripts/check/**`: %v", idx.Err)
	}
	return root, idx
}

// checksDirForTest is the runner's check-implementation directory.
func checksDirForTest(t *testing.T) string {
	t.Helper()
	return filepath.Join(repoRootForTest(t), filepath.Join(runnerChecksDirParts...))
}

// TestRunnerSourcesStayInTheirLane is the point of the whole per-check runner
// attribution: a leaf check's implementation file belongs to that check and to
// nobody else. Editing `lock-poison.go` cannot change what `cargo nextest`
// reports, so it must not appear in the nextest lane's input set.
func TestRunnerSourcesStayInTheirLane(t *testing.T) {
	root := repoRootForTest(t)
	data, err := CollectRepoFingerprintData(root)
	if err != nil {
		t.Fatalf("CollectRepoFingerprintData: %v", err)
	}

	for _, tc := range []struct {
		owner   string // the check whose implementation file this is
		file    string
		strange string // a check that has no business fingerprinting it
	}{
		{owner: "desktop-rust-lock-poison", file: "scripts/check/checks/lock-poison.go", strange: "desktop-rust-tests"},
		{owner: "desktop-svelte-e2e-playwright", file: "scripts/check/checks/desktop-svelte-e2e-playwright.go", strange: "desktop-rust-clippy"},
		{owner: "website-build", file: "scripts/check/checks/website-build.go", strange: "desktop-svelte-tests"},
	} {
		owner := GetCheckByID(tc.owner)
		if owner == nil {
			t.Fatalf("no check %q", tc.owner)
		}
		if !matchesAny(tc.file, data.PatternsFor(owner)) {
			t.Errorf("check %q doesn't fingerprint %s, which implements it: an edit there would cache-skip the lane it changes",
				tc.owner, tc.file)
		}

		strange := GetCheckByID(tc.strange)
		if strange == nil {
			t.Fatalf("no check %q", tc.strange)
		}
		if matchesAny(tc.file, data.PatternsFor(strange)) {
			t.Errorf("check %q fingerprints %s, which belongs to %q: editing one check re-runs the other for nothing",
				tc.strange, tc.file, tc.owner)
		}
	}
}

// TestEveryCheckGetsItsOwnRunnerSources ties the AST's reading of the registry to
// the binary that actually runs. `runtime.FuncForPC` names the file each `Run` was
// compiled from, which is ground truth the analysis can't talk itself out of: if
// the registry parse ever drifts (a check registered somewhere else, a `Run` built
// by a helper), the check whose file goes missing here is the one that would
// cache-skip its own implementation.
func TestEveryCheckGetsItsOwnRunnerSources(t *testing.T) {
	_, idx := runnerSourcesForTest(t)

	for i := range AllChecks {
		def := &AllChecks[i]
		files, ok := idx.byCheck[def.ID]
		if !ok {
			t.Errorf("check %q has no runner sources; it falls back to the whole tree (correct, but it means the registry parse missed it)", def.ID)
			continue
		}
		own := runFileOf(t, def)
		if !contains(files, own) {
			t.Errorf("check %q doesn't reach %s, the file its `Run` is compiled from; it would cache-skip its own edits (analysis says: %v)",
				def.ID, own, files)
		}
	}
}

// runFileOf returns the repo-relative path of the file a check's `Run` function
// is compiled from, straight from the linked binary.
func runFileOf(t *testing.T, def *CheckDefinition) string {
	t.Helper()
	pc := reflect.ValueOf(def.Run).Pointer()
	fn := runtime.FuncForPC(pc)
	if fn == nil {
		t.Fatalf("check %q has no resolvable Run function", def.ID)
	}
	file, _ := fn.FileLine(pc)
	// Every check implementation lives in one directory, so the base name is
	// enough (and survives a `-trimpath` build, where the path is module-relative).
	return strings.Join(runnerChecksDirParts, "/") + "/" + filepath.Base(file)
}

// TestGlobalInputsCoverWhatNoCheckCanReach closes the gap the per-check analysis
// leaves: a runner file no check's `Run` reaches still gets edited, and if nothing
// fingerprints it, every lane it changes reports a stale green. Every check
// implementation file is therefore either attributed to a check or part of the
// core set every check carries.
func TestGlobalInputsCoverWhatNoCheckCanReach(t *testing.T) {
	checksDir := checksDirForTest(t)
	_, idx := runnerSourcesForTest(t)

	attributed := map[string]bool{}
	for _, files := range idx.byCheck {
		for _, f := range files {
			attributed[f] = true
		}
	}

	entries, err := os.ReadDir(checksDir)
	if err != nil {
		t.Fatalf("reading %s: %v", checksDir, err)
	}
	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || !strings.HasSuffix(name, ".go") || strings.HasSuffix(name, "_test.go") {
			continue
		}
		path := strings.Join(runnerChecksDirParts, "/") + "/" + name
		if attributed[path] || matchesAny(path, GlobalInputs) {
			continue
		}
		t.Errorf("nothing fingerprints %s: no check's `Run` reaches it and `GlobalInputs` doesn't list it, "+
			"so an edit to it is invisible to the cache. Attribute it (call it from the check that needs it) or add it to `GlobalInputs`.", path)
	}
}

// TestRunnerCoreCoversWhatTheExecutorReaches is the other half of the same
// question, for the half the analysis structurally cannot see: package `main`
// imports `checks`, never the other way round, so nothing a check's `Run` reaches
// can tell us that the EXECUTOR uses a symbol. A helper the runner itself calls
// changes how every lane runs, so it has to live in a core file.
func TestRunnerCoreCoversWhatTheExecutorReaches(t *testing.T) {
	checksDir := checksDirForTest(t)

	// Symbols package `main` uses outside any check run. Each one is reached from
	// a CLI path that renders something and exits, so an edit to its file can't
	// change a check's verdict.
	offTheRunPath := map[string]string{
		"NightlyToolchain": "printed by `--print-nightly` for CI's toolchain install; no check run touches it",
		"BuildDocGraph":    "the `--docs-graph` renderer, which draws a diagram instead of running checks",
		"DocGraph":         "the type that renderer hands around; same path, same reasoning",
	}

	declaringFile := map[string]string{}
	parsed, err := parseRunnerFiles(checksDir)
	if err != nil {
		t.Fatalf("parseRunnerFiles: %v", err)
	}
	for name, file := range parsed {
		for _, decl := range file.Decls {
			switch decl := decl.(type) {
			case *ast.FuncDecl:
				if decl.Recv == nil {
					declaringFile[decl.Name.Name] = name
				}
			case *ast.GenDecl:
				for _, spec := range decl.Specs {
					switch spec := spec.(type) {
					case *ast.ValueSpec:
						for _, ident := range spec.Names {
							declaringFile[ident.Name] = name
						}
					case *ast.TypeSpec:
						declaringFile[spec.Name.Name] = name
					}
				}
			}
		}
	}

	for _, symbol := range executorUsesFromChecks(t, filepath.Dir(checksDir)) {
		file, ok := declaringFile[symbol]
		if !ok {
			continue // a method or a name that isn't declared at package level
		}
		path := strings.Join(runnerChecksDirParts, "/") + "/" + file
		if matchesAny(path, GlobalInputs) {
			continue
		}
		if _, allowed := offTheRunPath[symbol]; allowed {
			continue
		}
		t.Errorf("the runner itself uses `checks.%s` from %s, which isn't in `GlobalInputs`: an edit there changes how lanes run "+
			"but only re-runs the checks that happen to reach that file. Move the symbol to a core file, or add %s to `GlobalInputs`.",
			symbol, path, path)
	}
}

// executorUsesFromChecks returns every `checks.X` name package `main` references.
func executorUsesFromChecks(t *testing.T, runnerDir string) []string {
	t.Helper()
	entries, err := os.ReadDir(runnerDir)
	if err != nil {
		t.Fatalf("reading %s: %v", runnerDir, err)
	}
	fset := token.NewFileSet()
	seen := map[string]bool{}
	var out []string
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".go") {
			continue
		}
		file, err := parser.ParseFile(fset, filepath.Join(runnerDir, entry.Name()), nil, 0)
		if err != nil {
			t.Fatalf("parsing %s: %v", entry.Name(), err)
		}
		ast.Inspect(file, func(n ast.Node) bool {
			selector, ok := n.(*ast.SelectorExpr)
			if !ok {
				return true
			}
			if ident, ok := selector.X.(*ast.Ident); ok && ident.Name == "checks" && !seen[selector.Sel.Name] {
				seen[selector.Sel.Name] = true
				out = append(out, selector.Sel.Name)
			}
			return true
		})
	}
	if len(out) == 0 {
		t.Fatalf("found no `checks.` reference in %s at all; the scan is broken, not the tree", runnerDir)
	}
	return out
}

// allowlistFileRE matches the on-disk allowlists and baselines that live beside
// the checks. They're data, not code, so no source analysis can attribute them.
var allowlistFileRE = regexp.MustCompile(`[A-Za-z0-9._-]+-(?:allowlist|baseline)\.json`)

// TestAllowlistFilesAreFingerprintedByTheirCheck pins the data half of the
// runner tree. A warn-only scanner reads its allowlist on every run, so a
// hand-edited entry has to re-run the check that enforces it. Both directions:
// the check that names the file must fingerprint it, and no allowlist may sit
// there with nothing watching it.
func TestAllowlistFilesAreFingerprintedByTheirCheck(t *testing.T) {
	root := repoRootForTest(t)
	data, err := CollectRepoFingerprintData(root)
	if err != nil {
		t.Fatalf("CollectRepoFingerprintData: %v", err)
	}

	covered := map[string]bool{}
	for i := range AllChecks {
		def := &AllChecks[i]
		for _, path := range allowlistsNamedBy(t, root, runFileOf(t, def)) {
			covered[path] = true
			if !matchesAny(path, data.PatternsFor(def)) {
				t.Errorf("check %q names %s but doesn't fingerprint it: a hand-edited entry would cache-skip the check that enforces it. Add it to the check's `Inputs`.",
					def.ID, path)
			}
		}
	}

	for _, path := range allowlistFilesOnDisk(t, checksDirForTest(t)) {
		if covered[path] {
			continue
		}
		watched := false
		for i := range AllChecks {
			if matchesAny(path, data.PatternsFor(&AllChecks[i])) {
				watched = true
				break
			}
		}
		if !watched {
			t.Errorf("no check fingerprints %s: editing it changes what a lane enforces and nothing would re-run. Add it to the owning check's `Inputs`.", path)
		}
	}
}

// allowlistsNamedBy returns the allowlist paths a check's own implementation file
// mentions and that exist on disk. A name assembled at runtime doesn't match, and
// is covered by the second half of the test instead.
func allowlistsNamedBy(t *testing.T, root, checkFile string) []string {
	t.Helper()
	source, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(checkFile)))
	if err != nil {
		t.Fatalf("reading %s: %v", checkFile, err)
	}
	seen := map[string]bool{}
	var out []string
	for _, name := range allowlistFileRE.FindAllString(string(source), -1) {
		path := strings.Join(runnerChecksDirParts, "/") + "/" + name
		if seen[path] {
			continue
		}
		if _, err := os.Stat(filepath.Join(root, filepath.FromSlash(path))); err != nil {
			continue
		}
		seen[path] = true
		out = append(out, path)
	}
	return out
}

// allowlistFilesOnDisk lists the allowlists and baselines living beside the checks.
func allowlistFilesOnDisk(t *testing.T, checksDir string) []string {
	t.Helper()
	entries, err := os.ReadDir(checksDir)
	if err != nil {
		t.Fatalf("reading %s: %v", checksDir, err)
	}
	var out []string
	for _, entry := range entries {
		if !entry.IsDir() && allowlistFileRE.MatchString(entry.Name()) {
			out = append(out, strings.Join(runnerChecksDirParts, "/")+"/"+entry.Name())
		}
	}
	return out
}

// TestRunnerSourceAnalysisFailsClosed is the safety net under every narrow answer
// above. The analysis runs on the working tree at plan time, so it meets
// half-written files and unfamiliar shapes; when it does, every check has to fall
// back to the whole runner tree rather than to a narrow guess.
func TestRunnerSourceAnalysisFailsClosed(t *testing.T) {
	t.Run("a tree with no runner in it", func(t *testing.T) {
		idx := LoadRunnerSources(t.TempDir())
		if idx.Err == nil {
			t.Fatal("analyzing a tree with no runner succeeded; it must report why it gave up")
		}
		if got := idx.For("desktop-rust-clippy"); !reflect.DeepEqual(got, runnerTreeFallback) {
			t.Errorf("a failed analysis handed out %v; every check must fall back to %v", got, runnerTreeFallback)
		}
	})

	t.Run("a check whose file doesn't parse", func(t *testing.T) {
		dir := runnerFixture(t, map[string]string{
			"registry.go": "package checks\n\nvar AllChecks = []CheckDefinition{{ID: \"a\", Run: RunA}}\n",
			"a.go":        "package checks\n\nfunc RunA() { this is not Go }\n",
		})
		if idx := LoadRunnerSources(dir); idx.Err == nil {
			t.Error("a source that doesn't parse produced a confident answer")
		}
	})

	t.Run("an init() that registers instead of assigning", func(t *testing.T) {
		dir := runnerFixture(t, map[string]string{
			"registry.go": "package checks\n\nvar AllChecks = []CheckDefinition{{ID: \"a\", Run: RunA}}\n",
			"a.go":        "package checks\n\nfunc RunA() {}\n",
			"side.go":     "package checks\n\nfunc init() { register(RunA) }\n\nfunc register(any) {}\n",
		})
		idx := LoadRunnerSources(dir)
		if idx.Err == nil {
			t.Fatal("an init() with a side effect nothing names produced a confident answer; that effect is invisible to file-level analysis")
		}
		if !strings.Contains(idx.Err.Error(), "init()") {
			t.Errorf("the failure says %q, which doesn't point at the init()", idx.Err)
		}
	})

	t.Run("an init() that assigns is attributed to what it assigns", func(t *testing.T) {
		dir := runnerFixture(t, map[string]string{
			"registry.go": "package checks\n\nvar AllChecks = []CheckDefinition{{ID: \"a\", Run: RunA}}\n",
			"a.go":        "package checks\n\nfunc RunA() { _ = table }\n",
			"side.go":     "package checks\n\nvar table []string\n\nfunc init() { table = []string{\"x\"} }\n",
		})
		idx := LoadRunnerSources(dir)
		if idx.Err != nil {
			t.Fatalf("analysis gave up: %v", idx.Err)
		}
		if !contains(idx.For("a"), "scripts/check/checks/side.go") {
			t.Errorf("check `a` reads a variable an init() in side.go fills, but doesn't fingerprint side.go: %v", idx.For("a"))
		}
	})
}

// runnerFixture writes a miniature checks package at the path the analysis
// expects inside a fresh temp root, and returns that root.
func runnerFixture(t *testing.T, files map[string]string) string {
	t.Helper()
	root := t.TempDir()
	dir := filepath.Join(root, filepath.Join(runnerChecksDirParts...))
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	for name, body := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return root
}

// TestRunnerSourcesReachSharedHelpers proves the closure isn't just "the file the
// Run lives in": a check that leans on a shared helper file has to re-run when
// that helper changes, however many files away it is.
func TestRunnerSourcesReachSharedHelpers(t *testing.T) {
	_, idx := runnerSourcesForTest(t)
	for _, tc := range []struct{ id, helper string }{
		{"desktop-rust-clippy", "scripts/check/checks/cargo-workspace.go"},          // takes its selection from `HostCargoLaneArgs`
		{"desktop-rust-jscpd", "scripts/check/checks/jscpd.go"},                     // the lane body both copy-paste checks share
		{"file-length", "scripts/check/checks/allowlist.go"},                        // the allowlist shrink-wrap
		{"desktop-rust-lock-poison", "scripts/check/checks/directives.go"},          // the opt-out comment tracker
		{"desktop-rust-integration-tests", "scripts/check/checks/sftp_ports.go"},    // the fixture ports it runs against
		{"claude-md-length", "scripts/check/checks/docs_graph.go"},                  // the doc graph it walks
		{"desktop-svelte-e2e-playwright", "scripts/check/checks/e2e-build.go"},      // the binary it builds
		{"desktop-rust-tests", "scripts/check/checks/rust-test-diagnostics.go"},     // how a red lane is re-run and reported
		{"desktop-rust-module-cycles", "scripts/check/checks/invariant-density.go"}, // the shared subsystem table
		// Reached only through a METHOD on a type it names, which is the rule that
		// stands in for type information: drop it and this line goes red.
		{"invariant-density", "scripts/check/checks/docs-dead-links.go"},
	} {
		if !contains(idx.For(tc.id), tc.helper) {
			t.Errorf("check %q doesn't reach %s, which it calls into: an edit there would cache-skip the lane (analysis says: %v)",
				tc.id, tc.helper, idx.For(tc.id))
		}
	}
}

// contains reports whether the sorted slice holds want.
func contains(files []string, want string) bool {
	for _, f := range files {
		if f == want {
			return true
		}
	}
	return false
}
