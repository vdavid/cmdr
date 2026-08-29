package checks

import (
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"
)

// repoRootForTest walks up from the test's working directory to the repo root,
// using the same landmark `findRootDir` does. Tests that assert something about
// the REAL tree (rather than a fixture) need it; a fixture can't tell us whether
// today's sources embed a file today's Inputs forget.
func repoRootForTest(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	for {
		if _, statErr := os.Stat(filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")); statErr == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatalf("couldn't find the repo root above %s", dir)
		}
		dir = parent
	}
}

// embeddedPathRE captures the literal path of an `include_str!`/`include_bytes!`.
// Non-literal forms (a `concat!`, a `env!("CARGO_MANIFEST_DIR")` join) don't match
// and aren't checked; nothing in the tree uses one today, and a regex that tried
// would report paths that don't exist.
var embeddedPathRE = regexp.MustCompile(`include_(?:str|bytes)!\s*\(\s*"([^"]+)"`)

// embeddedFile is one compile-time input a Rust source pulls in with
// `include_str!` / `include_bytes!`: which member's tree the embedding source
// lives in, the source itself, and the embedded path. Both paths are
// repo-relative slash paths, ready to match against an `Inputs` set.
type embeddedFile struct {
	source   string
	embedded string
}

// collectEmbeddedFiles walks every workspace member's sources and returns each
// literal `include_str!` / `include_bytes!` target inside the repo.
func collectEmbeddedFiles(t *testing.T, root string) []embeddedFile {
	t.Helper()
	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	var found []embeddedFile
	for _, m := range members {
		walkErr := filepath.WalkDir(m.SrcDir, func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() || !strings.HasSuffix(path, ".rs") {
				return nil
			}
			source, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			for _, match := range embeddedPathRE.FindAllStringSubmatch(string(source), -1) {
				embedded := filepath.Clean(filepath.Join(filepath.Dir(path), filepath.FromSlash(match[1])))
				rel, relErr := filepath.Rel(root, embedded)
				if relErr != nil || strings.HasPrefix(rel, "..") {
					continue // outside the repo; not something Inputs can express
				}
				found = append(found, embeddedFile{
					source:   mustRel(t, root, path),
					embedded: filepath.ToSlash(rel),
				})
			}
			return nil
		})
		if walkErr != nil && !os.IsNotExist(walkErr) {
			t.Fatalf("walking %s: %v", m.SrcDir, walkErr)
		}
	}
	return found
}

// TestRustInputsCoverEveryEmbeddedFile is the guardrail behind every narrowed
// Rust input set. A file pulled into the binary with `include_str!` is a
// compile-time input just as much as a `.rs` file: change it and the tests can
// change verdict. If such a file falls outside a lane's `Inputs` while the source
// that embeds it stays inside, that lane cache-skips the edit and reports a green
// that describes the previous content.
//
// This is not hypothetical. `whats_new` embeds the repo-root `CHANGELOG.md`, which
// the one shared Rust input set didn't list at all, so a changelog edit never
// re-ran the Rust tests.
//
// It walks the WHOLE REGISTRY rather than one set, and pairs each check against
// each embedding source individually. That's what makes it safe to narrow a lane
// to one member's tree: a lane that stops covering `crates/cmdr-index/**` also
// stops owing whatever that crate embeds, while a lane that still covers the app
// tree still owes `CHANGELOG.md`. A per-set test could only ever prove the global
// case, which is the case that stops being the interesting one the moment the sets
// differ.
func TestRustInputsCoverEveryEmbeddedFile(t *testing.T) {
	root := repoRootForTest(t)
	embeds := collectEmbeddedFiles(t, root)
	if len(embeds) == 0 {
		t.Fatal("found no `include_str!`/`include_bytes!` call at all; the scan is broken, not the tree")
	}

	for _, def := range AllChecks {
		patterns := inputs(def.Inputs, GlobalInputs)
		for _, e := range embeds {
			if !matchesAny(e.source, patterns) {
				continue // the lane can't see the embedding source, so a stale embed can't reach it either
			}
			if !matchesAny(e.embedded, patterns) {
				t.Errorf("check %q covers %s but not %s, which that source embeds: editing %s would be invisible to the lane",
					def.ID, e.source, e.embedded, e.embedded)
			}
		}
	}
}

// TestRustMemberTreesMatchTheWorkspace pins `rustMemberTrees` to the real cargo
// workspace, in both directions. The table is hand-written because `Inputs` is
// static registry data with no repo root in hand, and a hand-written mirror of a
// manifest is exactly the thing that rots: a new crate that never reaches the
// table would sit outside every narrowed Rust lane's view and cache-skip forever,
// which is silent.
func TestRustMemberTreesMatchTheWorkspace(t *testing.T) {
	root := repoRootForTest(t)
	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}

	tabled := make(map[string]rustMemberTree, len(rustMemberTrees))
	for _, m := range rustMemberTrees {
		tabled[m.Pkg] = m
	}
	real := make(map[string]bool, len(members))
	for _, m := range members {
		real[m.Name] = true
		entry, ok := tabled[m.Name]
		if !ok {
			t.Errorf("workspace member %q has no `rustMemberTrees` entry; every narrowed Rust lane would ignore its tree", m.Name)
			continue
		}
		if entry.Kind != m.Kind {
			t.Errorf("`rustMemberTrees` calls %q a %q; its manifest says %q", m.Name, entry.Kind, m.Kind)
		}
		if want := m.RelDir(root) + "/**"; entry.Glob != want {
			t.Errorf("`rustMemberTrees` globs %q as %q; the member lives at %q", m.Name, entry.Glob, want)
		}
	}
	for _, m := range rustMemberTrees {
		if !real[m.Pkg] {
			t.Errorf("`rustMemberTrees` names %q, which is no longer a workspace member", m.Pkg)
		}
	}
}

// TestScannerInputsMatchTheirJurisdiction ties each Rust source scanner's cache
// key to the trees it actually walks. `rustScannerJurisdictions` already declares
// which members a scanner governs, and `rustScanInputs` takes the same kinds — but
// nothing stops a registry entry from passing different ones. Too wide and the
// scanner re-runs over trees it can't see; too narrow and it cache-skips the tree
// that moved, which is the silent half.
func TestScannerInputsMatchTheirJurisdiction(t *testing.T) {
	byID := make(map[string]CheckDefinition, len(AllChecks))
	for _, def := range AllChecks {
		byID[def.ID] = def
	}

	for id, jurisdiction := range rustScannerJurisdictions {
		def, ok := byID[id]
		if !ok {
			t.Errorf("`rustScannerJurisdictions` names %q, which is not a registered check", id)
			continue
		}
		patterns := inputs(def.Inputs, GlobalInputs)
		for _, m := range rustMemberTrees {
			probe := strings.TrimSuffix(m.Glob, "**") + "src/lib.rs"
			covered := matchesAny(probe, patterns)
			want := jurisdictionGoverns(jurisdiction, m)
			if covered && !want {
				t.Errorf("check %q fingerprints %s, but its jurisdiction doesn't govern that member: it re-runs over a tree it can't see",
					id, probe)
			}
			if !covered && want {
				t.Errorf("check %q scans %s but doesn't fingerprint it: an edit there would cache-skip the lane that guards it",
					id, probe)
			}
		}
	}
}

// jurisdictionGoverns reports whether a jurisdiction reaches the given member.
func jurisdictionGoverns(j ScannerJurisdiction, m rustMemberTree) bool {
	if j.AppTreeOnly {
		return m.Pkg == "cmdr"
	}
	for _, k := range j.Kinds {
		if m.Kind == k {
			return true
		}
	}
	return false
}

// TestInputSetsExcludeOnlyAgentDocs keeps the exclusions honest: a shared set may
// take out the colocated agent docs and nothing else, so a future `!` entry can't
// quietly hide a source tree from the cache. An exclusion is the one construct
// that can make an input set too NARROW, which is the failure mode that ships a
// regression, so a new one needs its own reasoning and its own test.
func TestInputSetsExcludeOnlyAgentDocs(t *testing.T) {
	allowed := map[string]bool{}
	for _, pattern := range agentDocExclusions {
		allowed[pattern] = true
	}
	for name, set := range map[string][]string{
		"rustCompileInputs":   rustCompileInputs,
		"rustAppTreeInputs":   rustAppTreeInputs,
		"rustScanApp":         rustScanInputs(KindApp),
		"rustScanAppTool":     rustScanInputs(KindApp, KindTool),
		"rustScanEveryKind":   rustScanInputs(KindApp, KindTool, KindVendored),
		"rustWorkspaceConfig": rustWorkspaceConfigInputs,
		"rustEmbeddedInputs":  rustEmbeddedInputs,
		"rustFixtureServers":  rustFixtureServerInputs,
		"svelteInputs":        svelteInputs,
		"websiteInputs":       websiteInputs,
		"apiServerInputs":     apiServerInputs,
		"dashboardInputs":     dashboardInputs,
		"goScriptsInputs":     goScriptsInputs,
		"goSourceInputs":      goSourceInputs,
		"goTestsInputs":       goTestsInputs,
		"workflowsInputs":     workflowsInputs,
		"wholeRepoInputs":     wholeRepoInputs,
		"desktopApp":          desktopAppInputs(),
		"GlobalInputs":        GlobalInputs,
	} {
		for _, pattern := range set {
			if strings.HasPrefix(pattern, "!") && !allowed[pattern] {
				t.Errorf("`%s` excludes %q; only the agent docs may be excluded", name, pattern)
			}
		}
	}
}

// frontendDocImportRE catches an ESM/CJS import or a Vite glob whose specifier is
// a Markdown file, including the `?raw` / `?url` query forms Vite uses to inline
// one. mdReadRE catches the Node side, where the path is usually assembled
// (`readFileSync(join(dir, 'CLAUDE.md'))`) rather than a bare literal.
var (
	frontendDocImportRE = regexp.MustCompile("\\b(?:from|import|require|glob)\\s*\\(?\\s*['\"`][^'\"`]*\\.md(?:\\?[a-z]+)?['\"`]")
	mdReadRE            = regexp.MustCompile(`\breadFile(?:Sync)?\s*\([^)]*\.md['"` + "`" + `]`)
)

// TestNoFrontendSourceLoadsAgentDocs is the guardrail behind `svelteInputs`'
// share of `agentDocExclusions`, and the frontend's answer to
// `TestRustInputsCoverEveryEmbeddedFile`. Vite turns `import doc from './X.md?raw'`
// into a build input, so a module that did that would make a doc edit change what
// the lanes verify — while the exclusion tells the cache it can't. Prose
// references to a `CLAUDE.md` in comments and ESLint messages are everywhere and
// are not loads, which is why this matches on the load construct, not the name.
func TestNoFrontendSourceLoadsAgentDocs(t *testing.T) {
	assertMarkdownLoadScanWorks(t)

	root := repoRootForTest(t)
	scanned := 0
	for _, rel := range frontendSourceRoots {
		walkErr := filepath.WalkDir(filepath.Join(root, rel), func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() {
				return skipDirIf(entry.Name() == "node_modules")
			}
			if !isFrontendSourceFile(path) {
				return nil
			}
			source, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			scanned++
			if loadsMarkdown(source) {
				t.Errorf("%s loads a Markdown file, but `svelteInputs` excludes the agent docs: drop the load, or narrow the exclusion", mustRel(t, root, path))
			}
			return nil
		})
		if walkErr != nil && !os.IsNotExist(walkErr) {
			t.Fatalf("walking %s: %v", rel, walkErr)
		}
	}
	if scanned == 0 {
		t.Fatal("found no frontend source file at all; the scan is broken, not the tree")
	}
}

// assertMarkdownLoadScanWorks proves the detector both ways before the tree scan
// trusts its silence: a scan that can't see a load would pass on any tree, and one
// that reads prose as a load would fail on every tree.
func assertMarkdownLoadScanWorks(t *testing.T) {
	t.Helper()
	for _, positive := range []string{
		"import doc from './CLAUDE.md?raw'",
		"const md = await import('../DETAILS.md')",
		"const md = require('./CLAUDE.md')",
		"import.meta.glob('./**/DETAILS.md', { eager: true })",
		"readFileSync(join(dir, 'CLAUDE.md'), 'utf8')",
	} {
		if !loadsMarkdown([]byte(positive)) {
			t.Errorf("the Markdown-load scan misses %q", positive)
		}
	}
	for _, negative := range []string{
		"// See `lib/ui/CLAUDE.md` § \"Focus trapping\".",
		"'narrow it with `isCommandId`. See `lib/commands/CLAUDE.md`.',",
		"import.meta.glob('./messages/*/*.json', { eager: true })",
	} {
		if loadsMarkdown([]byte(negative)) {
			t.Errorf("the Markdown-load scan reads %q as a load; a prose reference is not one", negative)
		}
	}
}

// loadsMarkdown reports whether the source pulls a Markdown file in as a build or
// runtime input.
func loadsMarkdown(source []byte) bool {
	return frontendDocImportRE.Match(source) || mdReadRE.Match(source)
}

// isFrontendSourceFile reports whether the path is a module the frontend toolchain
// compiles or lints.
func isFrontendSourceFile(path string) bool {
	switch filepath.Ext(path) {
	case ".ts", ".js", ".mjs", ".cjs", ".svelte":
		return true
	}
	return false
}

// skipDirIf turns a "should I descend?" question into WalkDir's answer.
func skipDirIf(skip bool) error {
	if skip {
		return filepath.SkipDir
	}
	return nil
}

func mustRel(t *testing.T, root, path string) string {
	t.Helper()
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return path
	}
	return filepath.ToSlash(rel)
}

// TestSvelteInputsSkipAgentDocs is the contract behind `svelteInputs`' exclusions:
// the frontend lanes must not re-run because a colocated `CLAUDE.md` changed.
// Those docs sit inside `apps/desktop/src/**` and `apps/desktop/test/**` and get
// edited on nearly every session by house rule, so without the veto a docs-only
// commit reruns the whole ~8,600-test Vitest suite plus every ESLint and typecheck
// pass.
func TestSvelteInputsSkipAgentDocs(t *testing.T) {
	patterns := inputs(svelteInputs, GlobalInputs)
	for _, doc := range []string{
		"apps/desktop/src/lib/ui/CLAUDE.md",
		"apps/desktop/src/lib/ui/DETAILS.md",
		"apps/desktop/test/e2e-playwright/CLAUDE.md",
		"scripts/check/CLAUDE.md",
	} {
		if matchesAny(doc, patterns) {
			t.Errorf("`svelteInputs` still covers %s; a docs-only edit re-runs every frontend lane", doc)
		}
	}
	// The veto must not reach past the docs: the sources themselves stay in.
	for _, src := range []string{
		"apps/desktop/src/lib/ui/Button.svelte",
		"apps/desktop/test/e2e-playwright/queue.spec.ts",
		"scripts/check/main.go",
	} {
		if !matchesAny(src, patterns) {
			t.Errorf("`svelteInputs` no longer covers %s; the frontend lanes would cache-skip a real change", src)
		}
	}
}

// realTreeReadingTests declares, for every test in this package that reads the
// REAL repo rather than a fixture, the paths it reads that live outside the
// `scripts-go-tests` lane's own Go trees, or that pin the lane's set to a tree it
// walks. A directory entry stands for everything under it.
//
// The declaration is what makes `goTestsInputs` explainable: the lane looks
// absurdly wide for a Go lint, and this is the list of guards that make it right.
// A guard that runs only when its own source changes isn't a guard: it goes green
// from cache on the very edit it exists to catch.
var realTreeReadingTests = map[string][]string{
	"TestAllowlistFilesAreFingerprintedByTheirCheck":           {"scripts/check/checks/registry.go", "scripts/check/checks/file-length-allowlist.json"},
	"TestBindingsRegenAsksCargoTheSameQuestionAsTheOtherLanes": {"apps/desktop/package.json", "Cargo.toml", "apps/desktop/src-tauri/Cargo.toml"},
	"TestEveryCheckGetsItsOwnRunnerSources":                    {"scripts/check/checks/registry.go"},
	"TestGlobalInputsCoverWhatNoCheckCanReach":                 {"scripts/check/checks"},
	"TestGoCompileLanesReadOnlyGoSources":                      {"scripts", "apps/desktop/scripts"},
	"TestGoTestsInputsCoverTheRealTreeItsTestsRead":            {"scripts/check/checks"},
	// `.mise.toml` is the single source for the Go toolchain version; these three
	// assert that whatever needs a Go version reads it from there.
	"TestLinuxContainerProvisionsTheMisePinnedGo":              {".mise.toml"},
	"TestMiseGoVersionReadsThePin":                             {".mise.toml"},
	"TestProvisionScriptStopsBeforeRunningTests":               {".mise.toml"},
	"TestModuleCyclesAllowlistMatchesPinnedVersion":            {"scripts/check/checks/module-cycles-allowlist.json"},
	"TestModuleCyclesPackagesAreTheLibraryMembers":             {"Cargo.toml", "crates/cmdr-fs/Cargo.toml"},
	"TestNoFrontendSourceLoadsAgentDocs":                       {"apps/desktop/src", "apps/desktop/test", "apps/desktop/scripts", "apps/desktop/eslint-plugins", "eslint-plugins"},
	"TestRunnerCoreCoversWhatTheExecutorReaches":               {"scripts/check", "scripts/check/checks"},
	"TestRunnerSourcesReachSharedHelpers":                      {"scripts/check/checks/common.go"},
	"TestRunnerSourcesStayInTheirLane":                         {"scripts/check/checks/lock-poison.go"},
	"TestRustInputsCoverEveryEmbeddedFile":                     {"Cargo.toml", "apps/desktop/src-tauri/src", "crates/cmdr-fs/src", "crates/fsevent-stream/src", "CHANGELOG.md"},
	"TestRustMemberTreesMatchTheWorkspace":                     {"Cargo.toml", "crates/cmdr-sftp/Cargo.toml", "apps/desktop/src-tauri/Cargo.toml"},
	"TestSftpFixturePathsAgree":                                {sftpComposeRel, sftpStartRel, sftpTestingRel, sftpEntrypointRel},
	"TestTheFixtureEntrypointRegeneratesAKeyItCanNoLongerBack": {sftpEntrypointRel},
	"TestSftpFixturePortsBindToLoopback":                       {sftpComposeRel},
	"TestSftpFixturePortsMatchComposeDefaults":                 {sftpComposeRel, sftpTestingRel},
	"TestSiblingToolDirsAreFingerprintedByTheirCheck":          {"scripts/check-a11y-contrast", "scripts/check-css-unused", "scripts/check-btn-restyle"},
}

// TestGoTestsInputsCoverTheRealTreeItsTestsRead is the guard behind
// `goTestsInputs`, and it goes red in both directions: a new test that reads the
// real tree without declaring what it reads, a declaration for a test that no
// longer exists, and a declared path the `scripts-go-tests` lane doesn't
// fingerprint.
//
// The third case is the one that bites. It found the lane cache-skipping the
// cargo manifests, `apps/desktop/package.json`, and every crate and frontend
// tree its own guards walk: adding a crate, adding an `include_str!`, or
// importing a `.md` from a Svelte module all reported a cached green from a run
// that predated them.
func TestGoTestsInputsCoverTheRealTreeItsTestsRead(t *testing.T) {
	root := repoRootForTest(t)
	reaching := testsReachingRealTree(t, filepath.Join(root, filepath.Join(runnerChecksDirParts...)))
	if len(reaching) == 0 {
		t.Fatal("found no test reaching `repoRootForTest`; the scan is broken, not the tree")
	}

	def := GetCheckByID("scripts-go-tests")
	if def == nil {
		t.Fatal("no `scripts-go-tests` check in the registry")
	}
	data, err := CollectRepoFingerprintData(root)
	if err != nil {
		t.Fatalf("CollectRepoFingerprintData: %v", err)
	}
	patterns := data.PatternsFor(def)

	for _, name := range reaching {
		declared, ok := realTreeReadingTests[name]
		if !ok {
			t.Errorf("%s reads the real repo but isn't in `realTreeReadingTests`: say what it reads, so `goTestsInputs` can cover it", name)
			continue
		}
		if len(declared) == 0 {
			t.Errorf("%s declares no path; a real-tree test reads something", name)
		}
	}
	reachingSet := make(map[string]bool, len(reaching))
	for _, name := range reaching {
		reachingSet[name] = true
	}

	for name, paths := range realTreeReadingTests {
		if !reachingSet[name] {
			t.Errorf("`realTreeReadingTests` lists %s, which no longer reads the real repo; drop the entry", name)
		}
		for _, path := range paths {
			info, statErr := os.Stat(filepath.Join(root, filepath.FromSlash(path)))
			if statErr != nil {
				t.Errorf("%s declares %s, which doesn't exist (renamed?)", name, path)
				continue
			}
			probe := path
			if info.IsDir() {
				probe = path + "/probe.txt"
			}
			if !matchesAny(probe, patterns) {
				t.Errorf("`scripts-go-tests` doesn't fingerprint %s, which %s reads: editing it would leave the guard on a cached green. Widen `goTestsInputs`.",
					path, name)
			}
		}
	}
}

// packageFuncRefs maps every plain function in the checks package (tests
// included) to the package-level names its body mentions.
func packageFuncRefs(t *testing.T, checksDir string) map[string]map[string]bool {
	t.Helper()
	entries, err := os.ReadDir(checksDir)
	if err != nil {
		t.Fatalf("reading %s: %v", checksDir, err)
	}
	fset := token.NewFileSet()
	refs := map[string]map[string]bool{}
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".go") {
			continue
		}
		file, parseErr := parser.ParseFile(fset, filepath.Join(checksDir, entry.Name()), nil, 0)
		if parseErr != nil {
			t.Fatalf("parsing %s: %v", entry.Name(), parseErr)
		}
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Recv != nil || fn.Body == nil {
				continue
			}
			named := refs[fn.Name.Name]
			if named == nil {
				named = map[string]bool{}
				refs[fn.Name.Name] = named
			}
			collectIdents(fn.Body, named)
		}
	}
	return refs
}

// collectIdents records the bare identifiers a node mentions. A selector
// contributes only its left-hand side, the same call `runner-sources.go` makes:
// resolving `x.Foo` by method name alone makes everything reach everything.
func collectIdents(node ast.Node, into map[string]bool) {
	ast.Inspect(node, func(n ast.Node) bool {
		switch n := n.(type) {
		case *ast.SelectorExpr:
			collectIdents(n.X, into)
			return false
		case *ast.Ident:
			into[n.Name] = true
		}
		return true
	})
}

// testsReachingRealTree returns the `Test*` functions in the checks package that
// reach `repoRootForTest`, directly or through a helper. Same rule as
// `runner-sources.go`: a declaration reaches the package-level names it mentions
// as a bare identifier.
func testsReachingRealTree(t *testing.T, checksDir string) []string {
	t.Helper()
	refs := packageFuncRefs(t, checksDir)

	var out []string
	for name := range refs {
		if !strings.HasPrefix(name, "Test") {
			continue
		}
		seen := map[string]bool{}
		queue := []string{name}
		for len(queue) > 0 {
			current := queue[0]
			queue = queue[1:]
			if seen[current] {
				continue
			}
			seen[current] = true
			for ref := range refs[current] {
				queue = append(queue, ref)
			}
		}
		if seen["repoRootForTest"] {
			out = append(out, name)
		}
	}
	sort.Strings(out)
	return out
}

// goEmbedDirectiveRE catches a real `go:embed` directive: it sits alone on its
// own line, ahead of the var it fills. Prose mentioning one mid-comment (this
// file does, twice) is not one.
var goEmbedDirectiveRE = regexp.MustCompile(`(?m)^[ \t]*//go:embed[ \t]`)

// TestGoCompileLanesReadOnlyGoSources is the guard behind `goSourceInputs`: the
// eight Go lanes that COMPILE the scripts trees fingerprint the `.go` files and
// the module files, and nothing else in those trees. That's only true while
// nothing there is a compile input the glob can't see, so this walks the real
// tree for the two shapes that would break it: a `//go:embed` (which turns any
// file into a compile-time input) and a non-Go source the toolchain builds.
//
// It also pins the narrowing itself: a JSON allowlist edit must NOT re-run these
// lanes, which is the ~70 s of linting the narrowing bought back on the 357
// commits in six months that touched a Go tree without touching a `.go` file.
func TestGoCompileLanesReadOnlyGoSources(t *testing.T) {
	root := repoRootForTest(t)
	// Go builds these alongside `.go` files; none exist in the scripts trees today,
	// and one landing there would sit outside `goSourceInputs`.
	compiledButNotGo := map[string]bool{".s": true, ".c": true, ".h": true, ".cc": true, ".cpp": true, ".m": true, ".syso": true}
	scanned := 0
	for _, goDir := range GetGoDirectories() {
		walkErr := filepath.WalkDir(filepath.Join(root, goDir), func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() {
				return skipDirIf(entry.Name() == "node_modules")
			}
			rel := mustRel(t, root, path)
			name := entry.Name()
			switch {
			case strings.HasSuffix(name, ".go"), name == "go.mod", name == "go.sum":
				if !matchesAny(rel, goSourceInputs) {
					t.Errorf("`goSourceInputs` doesn't cover %s, so the Go lanes would cache-skip it", rel)
				}
			case compiledButNotGo[filepath.Ext(name)]:
				t.Errorf("%s is a source the Go toolchain compiles, but `goSourceInputs` only names `.go` and the module files", rel)
			}
			if !strings.HasSuffix(name, ".go") {
				return nil
			}
			source, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			scanned++
			if goEmbedDirectiveRE.Match(source) {
				t.Errorf("%s embeds a file with `//go:embed`, which makes that file a compile input `goSourceInputs` can't see; add it to the set", rel)
			}
			return nil
		})
		if walkErr != nil {
			t.Fatalf("walking %s: %v", goDir, walkErr)
		}
	}
	if scanned == 0 {
		t.Fatal("scanned no Go source at all; the walk is broken, not the tree")
	}
	assertGoCompileLaneSeesOnlySources(t, root)
}

// assertGoCompileLaneSeesOnlySources checks the narrowing where it counts: on a
// real lane's real pattern set, which is `Inputs` plus `GlobalInputs` plus the
// runner sources it reaches. The narrowing has to bite, and it has to stop where
// the Go sources do.
func assertGoCompileLaneSeesOnlySources(t *testing.T, root string) {
	t.Helper()
	gofmt := GetCheckByID("scripts-go-gofmt")
	if gofmt == nil {
		t.Fatal("no `scripts-go-gofmt` check in the registry")
	}
	data, err := CollectRepoFingerprintData(root)
	if err != nil {
		t.Fatalf("CollectRepoFingerprintData: %v", err)
	}
	patterns := data.PatternsFor(gofmt)
	for _, quiet := range []string{
		"scripts/check/checks/file-length-allowlist.json",
		"scripts/check/CLAUDE.md",
		"apps/desktop/scripts/screenshots/take.ts",
	} {
		if matchesAny(quiet, patterns) {
			t.Errorf("the Go compile lanes still fingerprint %s, which no Go tool reads", quiet)
		}
	}
	for _, loud := range []string{
		"scripts/check/checks/lock-poison.go",
		"scripts/check-a11y-contrast/main.go",
		"scripts/check/go.mod",
	} {
		if !matchesAny(loud, patterns) {
			t.Errorf("the Go compile lanes no longer fingerprint %s; a real change there would cache-skip", loud)
		}
	}
}
