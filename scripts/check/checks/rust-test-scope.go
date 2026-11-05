package checks

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// The Rust test scope: which workspace members' tests a change can affect, so a
// test lane runs those and skips the rest. Two halves, both here:
//
//   - COMPUTE (`ComputeRustTestScope`, behind `--print-rust-test-scope`): classify
//     every changed path against `rustTestScopeRules`, seed the members that own or
//     read it, and close the set over reverse dependencies from `cargo metadata`
//     (normal, build, AND dev edges: a crate whose tests use a changed crate is
//     affected too). Anything it can't place answers "all".
//   - CONSUME (`rustTestScopeFromEnv`, read by `runNextestLane`): the
//     `CMDR_RUST_TEST_SCOPE` value becomes a nextest `package(=…)` filterset ANDed
//     onto the lane's own filter. Unset means everything, which is the default.
//
// ❗ The build stays workspace-wide (`HostCargoLaneArgs`); only the RUN narrows. A
// `-p` build resolves features differently and thrashes the shared `target/`
// (`DETAILS.md` § "Why the cargo lanes stay on `--workspace`").
//
// The failure mode this file exists to rule out is a green run that never ran a
// test the change could break. So every doubt resolves to "all": an unknown path,
// a git or cargo error, an empty env value. `rust-test-scope_test.go` pins the rule
// table to the tree (every out-of-crate read the Rust sources make is classified,
// every `rust` CI filter path is classified, and every rule is reachable).

// rustTestScopeAll and rustTestScopeNone are the two non-list values of the scope.
const (
	rustTestScopeAll  = "all"
	rustTestScopeNone = "none"
)

// RustTestScope is the set of workspace members whose tests a change can affect.
//
// The zero value runs everything, so a lane nobody scoped behaves as it always did.
type RustTestScope struct {
	// Narrowed is false for "every test runs", today's behavior and the safe
	// answer. True means only Packages run.
	Narrowed bool
	// Packages are the affected members' package names, sorted. Empty with
	// Narrowed means no member is affected, and the lane skips.
	Packages []string
}

// String renders the scope the way `CMDR_RUST_TEST_SCOPE` takes it.
func (s RustTestScope) String() string {
	if !s.Narrowed {
		return rustTestScopeAll
	}
	if len(s.Packages) == 0 {
		return rustTestScopeNone
	}
	return strings.Join(s.Packages, " ")
}

// selectsNothing reports whether the scope affects no member at all.
func (s RustTestScope) selectsNothing() bool {
	return s.Narrowed && len(s.Packages) == 0
}

// includes reports whether the scope runs the given package's tests.
func (s RustTestScope) includes(pkg string) bool {
	if !s.Narrowed {
		return true
	}
	for _, p := range s.Packages {
		if p == pkg {
			return true
		}
	}
	return false
}

// nextestFilter is the filterset for the scope, or "" when it runs everything.
// `=` makes each match exact, so `cmdr` can't also select `cmdr-fs`.
func (s RustTestScope) nextestFilter() string {
	if !s.Narrowed {
		return ""
	}
	clauses := make([]string, 0, len(s.Packages))
	for _, p := range s.Packages {
		clauses = append(clauses, "package(="+p+")")
	}
	return strings.Join(clauses, " + ")
}

// note is the suffix a lane's success message carries, so a CI log says a green
// run was a scoped one.
func (s RustTestScope) note() string {
	if !s.Narrowed {
		return ""
	}
	return fmt.Sprintf(" (scoped to %d %s: %s)", len(s.Packages),
		Pluralize(len(s.Packages), "crate", "crates"), strings.Join(s.Packages, ", "))
}

// scopedNextestFilter ANDs the scope onto a lane's own filter. Both sides are
// parenthesised: a lane filter is a `+` union, and `&` binds tighter.
func scopedNextestFilter(laneFilter string, scope RustTestScope) string {
	scopeFilter := scope.nextestFilter()
	switch {
	case scopeFilter == "":
		return laneFilter
	case laneFilter == "":
		return scopeFilter
	default:
		return "(" + laneFilter + ") & (" + scopeFilter + ")"
	}
}

// errRustTestScopeSelectsNothing is what `runNextestLane` returns, with a nil lane,
// when the scope affects no member: there's nothing to build or run.
var errRustTestScopeSelectsNothing = errors.New("no workspace crate is affected by this change")

// rustLaneNotRun is the verdict for a lane `runNextestLane` didn't hand back: a
// skip when the scope selected nothing, else the build error as it came.
func rustLaneNotRun(err error) (CheckResult, error) {
	if errors.Is(err, errRustTestScopeSelectsNothing) {
		return Skipped(fmt.Sprintf("%s (%s=%s)", errRustTestScopeSelectsNothing, RustTestScopeEnv, rustTestScopeNone)), nil
	}
	return CheckResult{}, err
}

// rustTestScopeFromEnv reads `CMDR_RUST_TEST_SCOPE`. Unset, empty, or "all" is
// everything. A name that isn't a workspace member fails the lane rather than
// dropping out quietly: a typo'd crate would otherwise be a crate nobody tests.
func rustTestScopeFromEnv(rootDir string) (RustTestScope, error) {
	raw := os.Getenv(RustTestScopeEnv)
	members, err := WorkspaceMembers(rootDir)
	if err != nil {
		return RustTestScope{}, err
	}
	names := make([]string, 0, len(members))
	for _, m := range members {
		names = append(names, m.Name)
	}
	return parseRustTestScope(raw, names)
}

// parseRustTestScope parses a scope value against the workspace's package names.
func parseRustTestScope(raw string, members []string) (RustTestScope, error) {
	fields := strings.FieldsFunc(raw, func(r rune) bool { return r == ' ' || r == ',' || r == '\n' || r == '\t' })
	if len(fields) == 0 || (len(fields) == 1 && fields[0] == rustTestScopeAll) {
		return RustTestScope{}, nil
	}
	if len(fields) == 1 && fields[0] == rustTestScopeNone {
		return RustTestScope{Narrowed: true}, nil
	}
	pkgs, err := rustTestScopePackages(raw, fields, members)
	if err != nil {
		return RustTestScope{}, err
	}
	return RustTestScope{Narrowed: true, Packages: pkgs}, nil
}

// rustTestScopePackages validates a scope's package list: sorted, deduplicated,
// and every name a workspace member.
func rustTestScopePackages(raw string, fields, members []string) ([]string, error) {
	known := make(map[string]bool, len(members))
	for _, m := range members {
		known[m] = true
	}
	seen := make(map[string]bool, len(fields))
	var unknown []string
	for _, f := range fields {
		if f == rustTestScopeAll || f == rustTestScopeNone {
			return nil, fmt.Errorf("%s=%q mixes %q with other values; it's %q, %q, or a list of crates",
				RustTestScopeEnv, raw, f, rustTestScopeAll, rustTestScopeNone)
		}
		if !known[f] {
			unknown = append(unknown, f)
			continue
		}
		seen[f] = true
	}
	if len(unknown) > 0 {
		return nil, fmt.Errorf("%s names %s, which %s no workspace member; refusing to run a scope that would silently drop %s",
			RustTestScopeEnv, strings.Join(unknown, ", "), Pluralize(len(unknown), "is", "are"), Pluralize(len(unknown), "it", "them"))
	}
	pkgs := make([]string, 0, len(seen))
	for p := range seen {
		pkgs = append(pkgs, p)
	}
	sort.Strings(pkgs)
	return pkgs, nil
}

// ── Compute ───────────────────────────────────────────────────────────────────

// rustScopeEffect is what a changed path does to the scope.
type rustScopeEffect int

const (
	// scopeAll: the path can change any test's outcome (toolchain, lockfile,
	// manifests, the runner, the fixture servers).
	scopeAll rustScopeEffect = iota
	// scopeOwner: the path belongs to a member's tree, which seeds that member.
	// A path under the glob that's in NO member (a crate not yet in the
	// workspace) answers "all".
	scopeOwner
	// scopeReaders: the named members read this path (an `include_str!`, a
	// fixture, a build script input), so they're seeded, plus the owning member
	// if the path sits inside one.
	scopeReaders
	// scopeNothing: no Rust test or build reads the path.
	scopeNothing
)

// rustScopeRule classifies the changed paths a glob matches. First match wins.
type rustScopeRule struct {
	Glob    string
	Effect  rustScopeEffect
	Readers []string // scopeReaders only: the package names that read it
	Why     string
}

// rustTestScopeRules is the whole classification, in order, first match wins. A
// path no rule matches answers "all".
//
// `rust-test-scope_test.go` keeps it honest: every out-of-crate path a Rust source
// names (`include_str!`, a `CARGO_MANIFEST_DIR` join, a build script's
// `rerun-if-changed`) must classify to a rule that tests its reader, every path
// in ci.yml's `rust` filter must classify, every rule must be reachable past the
// ones above it, and every reader must be a member.
var rustTestScopeRules = []rustScopeRule{
	// Agent docs first: no Rust test or build reads one (the scanner test proves
	// it), and the `rust` filter and `agentDocExclusions` skip them too.
	{Glob: "**/CLAUDE.md", Effect: scopeNothing, Why: "agent docs; no Rust test or build reads one"},
	{Glob: "**/DETAILS.md", Effect: scopeNothing, Why: "agent docs; no Rust test or build reads one"},

	// The safety valves. Each can change what any member compiles to or how any
	// test runs.
	{Glob: "Cargo.toml", Effect: scopeAll, Why: "the workspace manifest: members, patches, profiles, lints"},
	{Glob: "crates/*/Cargo.toml", Effect: scopeAll, Why: "a member manifest; a feature or dependency change re-unifies features for the whole workspace build"},
	{Glob: "apps/desktop/src-tauri/Cargo.toml", Effect: scopeAll, Why: "the app's manifest; a feature or dependency change re-unifies features for the whole workspace build"},
	{Glob: "Cargo.lock", Effect: scopeAll, Why: "the resolved dependency versions every member builds against"},
	{Glob: "rust-toolchain.toml", Effect: scopeAll, Why: "the compiler every member builds with"},
	{Glob: ".mise.toml", Effect: scopeAll, Why: "tool versions (cargo-nextest, Go, Node) the lanes run with"},
	{Glob: ".config/nextest.toml", Effect: scopeAll, Why: "nextest's per-test caps, retries, and groups"},
	{Glob: "vendor/**", Effect: scopeAll, Why: "patched third-party crates, swapped in for the whole graph via `[patch.crates-io]`"},
	{Glob: "apps/desktop/test/nfs-servers/**", Effect: scopeAll, Why: "a fixture server the integration lanes run against"},
	{Glob: "apps/desktop/test/s3-servers/**", Effect: scopeAll, Why: "a fixture server the integration lanes run against"},
	{Glob: "apps/desktop/test/sftp-servers/**", Effect: scopeAll, Why: "a fixture server the integration lanes run against"},
	{Glob: "apps/desktop/test/smb-servers/**", Effect: scopeAll, Why: "a fixture server the integration lanes run against"},
	{Glob: "apps/desktop/test/webdav-servers/**", Effect: scopeAll, Why: "a fixture server the integration lanes run against"},
	{Glob: ".github/workflows/ci.yml", Effect: scopeAll, Why: "defines how the Rust lanes run"},
	{Glob: ".github/workflows/rust-lane.yml", Effect: scopeAll, Why: "defines how the Linux Rust lane runs"},
	{Glob: ".github/workflows/rust-fixtures-lane.yml", Effect: scopeAll, Why: "defines how the fixtures Rust lane runs"},
	{Glob: ".github/workflows/macos-lane.yml", Effect: scopeAll, Why: "defines how the macOS Rust lane runs"},
	// The composite actions the Rust lane files use; another action (say `light-cache`) has no
	// rule, so it falls through to the unknown-path `all`, which only ever widens.
	{Glob: ".github/actions/free-disk-if-small/**", Effect: scopeAll, Why: "a composite action the Rust lanes use"},
	{Glob: ".github/actions/pinned-rust-toolchain/**", Effect: scopeAll, Why: "a composite action the Rust lanes use (the compiler they build with)"},
	{Glob: ".github/actions/tauri-linux-deps/**", Effect: scopeAll, Why: "a composite action the Rust lanes use (the system libraries they link)"},
	{Glob: ".github/actions/wasm-toolchain/**", Effect: scopeAll, Why: "a composite action the Rust lanes use"},
	{Glob: ".github/actions/rust-test-scope/**", Effect: scopeAll, Why: "computes this very scope in the Rust lanes"},
	{Glob: ".github/actions/lane-marker/**", Effect: scopeNothing, Why: "saves the lane's marker after its last check; no test reads it"},
	{Glob: "scripts/check/**", Effect: scopeAll, Why: "the check runner that builds and runs the tests, this scope logic included"},
	{Glob: "scripts/check.sh", Effect: scopeAll, Why: "the check runner's entry point"},

	// Paths inside one member's tree that ANOTHER member's tests read. The owner
	// is seeded too.
	{Glob: "apps/desktop/src-tauri/src/**", Effect: scopeReaders, Readers: []string{"cmdr-operations"},
		Why: "`cmdr-operations`' compression-estimate test measures the app's source tree (`compress_estimate.rs`)"},
	{Glob: "crates/cmdr-menu/src/**", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the app embeds the menu bar's definition and ids (`menu/menu_items_test.rs`, `menu/macos_appkit.rs`, `menu/context_menu_icons_test.rs`), and the native-strings test walks the whole crate's sources for message keys (`intl/native_strings.rs`)"},

	// Paths outside every member that a member's tests or build read.
	{Glob: "CHANGELOG.md", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "`whats_new` embeds it and a test parses it"},
	{Glob: "apps/desktop/src/lib/commands/command-ids.ts", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the app's `build.rs` generates the breadcrumb vocabulary from it, and a drift test embeds it"},
	{Glob: "apps/desktop/src/lib/ipc/bindings.ts", Effect: scopeReaders, Readers: []string{"cmdr", "cmdr-listing-wasm"},
		Why: "the app's IPC tests compare against the generated bindings (`ipc.rs`), and the browser listing engine's event drift guard embeds them (`cmdr-listing-wasm/src/events_test.rs`)"},
	{Glob: "apps/desktop/src/lib/logging/bridge-redaction-cases.json", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the frontend-bridge redaction test embeds the shared case table (`redact/fe_bridge_tests.rs`)"},
	{Glob: "apps/desktop/src/lib/logging/log-markers.json", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the frontend-bridge redaction test embeds the marker vocabulary (`redact/fe_bridge_tests.rs`)"},
	{Glob: "apps/desktop/src/lib/units/drive-figure-cases.json", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the space poller's tests embed the shared drive-figure table (`space_poller/readout.rs`)"},
	{Glob: "apps/desktop/src/lib/units/free-percent-cases.json", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the space poller's band tests embed the shared free-percent cases (`space_poller/readout.rs`)"},
	{Glob: "apps/desktop/src/lib/intl/messages/**", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the native-string resolver's tests read the message catalogs (`intl/resolve.rs`)"},
	{Glob: "apps/desktop/src/lib/ask-cmdr/ask-cmdr-tool-label-keys.ts", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the agent view tests read the tool-label key list (`tools/tests/agent_view_tests.rs`)"},
	{Glob: "apps/desktop/test/fixtures/encodings/**", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the encoding-inspect tests read these fixtures (`tools/read/inspect/tests.rs`)"},
	{Glob: "apps/desktop/scripts/download-llama-server.go", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the app's `build.rs` reruns on it"},
	{Glob: "apps/website/public/mdm/**", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the managed-policy drift guards embed the public MDM files (`managed_policy/`)"},
	{Glob: "apps/website/src/lib/trust-mdm.ts", Effect: scopeReaders, Readers: []string{"cmdr"},
		Why: "the managed-policy drift guards embed the `/trust` key list (`managed_policy/`)"},

	// The members' own trees. A test checks every member sits under one of these.
	{Glob: "crates/**", Effect: scopeOwner, Why: "a member crate's own tree"},
	{Glob: "apps/desktop/src-tauri/**", Effect: scopeOwner, Why: "the app crate's own tree"},

	// Trees no Rust test or build reads. Kept to the ones that are certain and
	// commonly touched; anything left out answers "all", which only costs time.
	{Glob: "apps/desktop/src/**", Effect: scopeNothing, Why: "the Svelte frontend; the files the Rust side reads are classified above"},
	{Glob: "apps/desktop/static/**", Effect: scopeNothing, Why: "frontend static assets"},
	{Glob: "apps/desktop/test/e2e-playwright/**", Effect: scopeNothing, Why: "the Playwright E2E suite, run by its own lane"},
	{Glob: "apps/desktop/test/e2e-shared/**", Effect: scopeNothing, Why: "E2E helpers and fixtures, run by the E2E lanes"},
	{Glob: "apps/desktop/eslint-plugins/**", Effect: scopeNothing, Why: "frontend lint rules"},
	{Glob: "apps/desktop/finder-extension/**", Effect: scopeNothing, Why: "the Swift Finder extension, compiled by `desktop-finder-extension-compile`, not by cargo"},
	{Glob: "apps/website/**", Effect: scopeNothing, Why: "the marketing site; the files the Rust side reads are classified above"},
	{Glob: "apps/api-server/**", Effect: scopeNothing, Why: "the Cloudflare Worker"},
	{Glob: "apps/analytics-dashboard/**", Effect: scopeNothing, Why: "the metrics dashboard"},
	{Glob: "scripts/build-listing-wasm.sh", Effect: scopeNothing, Why: "built and smoke-run by `desktop-rust-listing-wasm`, not by nextest"},
	{Glob: "scripts/build-listing-wasm/**", Effect: scopeNothing, Why: "built and smoke-run by `desktop-rust-listing-wasm`, not by nextest"},
	{Glob: "docs/**", Effect: scopeNothing, Why: "docs"},
	{Glob: "brand/**", Effect: scopeNothing, Why: "brand assets"},
	{Glob: "eslint-plugins/**", Effect: scopeNothing, Why: "shared frontend lint rules"},
}

// rustCrateGraph is the slice of `cargo metadata --no-deps` the scope needs.
type rustCrateGraph struct {
	// dirs maps each member's package name to its repo-relative dir, slash form.
	dirs map[string]string
	// dependents maps a member to the members that depend on it directly, over
	// every edge kind (normal, build, dev) and every target.
	dependents map[string][]string
}

// cargoMetadataNoDeps is the slice of `cargo metadata --no-deps` JSON read here.
type cargoMetadataNoDeps struct {
	WorkspaceRoot string `json:"workspace_root"`
	Packages      []struct {
		Name         string `json:"name"`
		ManifestPath string `json:"manifest_path"`
		Dependencies []struct {
			Name string `json:"name"`
			// Path is set for path dependencies only, which is every edge between
			// two members (a `workspace = true` dep resolves to its path here).
			Path string `json:"path"`
		} `json:"dependencies"`
	} `json:"packages"`
}

// loadRustCrateGraph asks cargo for the workspace graph. `--no-deps` lists the
// members with their declared dependencies and needs no resolution, so it's fast
// (~40 ms) and works offline.
func loadRustCrateGraph(rootDir string) (rustCrateGraph, error) {
	cmd := exec.Command("cargo", "metadata", "--no-deps", "--format-version", "1", "--offline")
	cmd.Dir = rootDir
	var stdout, stderr bytes.Buffer
	if err := RunCommandTo(cmd, &stdout, &stderr); err != nil {
		return rustCrateGraph{}, fmt.Errorf("cargo metadata: %w\n%s", err, indentOutput(stderr.String()))
	}
	return parseRustCrateGraph(stdout.Bytes())
}

// parseRustCrateGraph builds the graph from `cargo metadata --no-deps` JSON.
func parseRustCrateGraph(raw []byte) (rustCrateGraph, error) {
	var meta cargoMetadataNoDeps
	if err := json.Unmarshal(raw, &meta); err != nil {
		return rustCrateGraph{}, fmt.Errorf("couldn't parse cargo metadata: %w", err)
	}
	graph := rustCrateGraph{dirs: map[string]string{}, dependents: map[string][]string{}}
	byDir := map[string]string{}
	for _, p := range meta.Packages {
		rel, err := filepath.Rel(meta.WorkspaceRoot, filepath.Dir(p.ManifestPath))
		if err != nil || strings.HasPrefix(rel, "..") {
			return rustCrateGraph{}, fmt.Errorf("member %s sits outside the workspace root %s", p.Name, meta.WorkspaceRoot)
		}
		graph.dirs[p.Name] = filepath.ToSlash(rel)
		byDir[filepath.Clean(filepath.Dir(p.ManifestPath))] = p.Name
	}
	for _, p := range meta.Packages {
		seen := map[string]bool{}
		for _, d := range p.Dependencies {
			if d.Path == "" {
				continue
			}
			dep, ok := byDir[filepath.Clean(d.Path)]
			if !ok || dep == p.Name || seen[dep] {
				continue
			}
			seen[dep] = true
			graph.dependents[dep] = append(graph.dependents[dep], p.Name)
		}
	}
	return graph, nil
}

// owner returns the member whose dir holds path (longest dir wins), or "".
func (g rustCrateGraph) owner(path string) string {
	best, bestLen := "", -1
	for name, dir := range g.dirs {
		if (dir == "." || strings.HasPrefix(path, dir+"/")) && len(dir) > bestLen {
			best, bestLen = name, len(dir)
		}
	}
	return best
}

// closeOverDependents returns the seeds plus every member that depends on one of
// them, transitively, sorted.
func (g rustCrateGraph) closeOverDependents(seeds map[string]bool) []string {
	affected := map[string]bool{}
	queue := make([]string, 0, len(seeds))
	for s := range seeds {
		queue = append(queue, s)
	}
	for len(queue) > 0 {
		next := queue[0]
		queue = queue[1:]
		if affected[next] {
			continue
		}
		affected[next] = true
		queue = append(queue, g.dependents[next]...)
	}
	out := make([]string, 0, len(affected))
	for name := range affected {
		out = append(out, name)
	}
	sort.Strings(out)
	return out
}

// matchRustScopeRule returns the first rule matching path, or nil.
func matchRustScopeRule(rules []rustScopeRule, path string) *rustScopeRule {
	for i := range rules {
		if matchGlob(rules[i].Glob, path) {
			return &rules[i]
		}
	}
	return nil
}

// rustScopeDecision is why one changed path moved the scope; printed to stderr so
// a CI log says why a run was full.
type rustScopeDecision struct {
	Path   string
	Seeds  []string
	All    bool
	Reason string
}

// affectedRustScope classifies each changed path and closes the seeds over
// reverse dependencies. Any path that answers "all" makes the whole scope "all";
// the decisions say which.
func affectedRustScope(graph rustCrateGraph, rules []rustScopeRule, changed []string) (RustTestScope, []rustScopeDecision) {
	seeds := map[string]bool{}
	var decisions []rustScopeDecision
	all := false
	for _, path := range changed {
		path = filepath.ToSlash(strings.TrimPrefix(path, "./"))
		if path == "" {
			continue
		}
		d := classifyRustScopePath(graph, rules, path)
		if d.All {
			all = true
		}
		for _, s := range d.Seeds {
			seeds[s] = true
		}
		if d.All || len(d.Seeds) > 0 {
			decisions = append(decisions, d)
		}
	}
	if all {
		return RustTestScope{}, decisions
	}
	return RustTestScope{Narrowed: true, Packages: graph.closeOverDependents(seeds)}, decisions
}

// classifyRustScopePath is one path's verdict.
func classifyRustScopePath(graph rustCrateGraph, rules []rustScopeRule, path string) rustScopeDecision {
	d := rustScopeDecision{Path: path}
	rule := matchRustScopeRule(rules, path)
	if rule == nil {
		d.All, d.Reason = true, "no rule classifies it, so it could be anything"
		return d
	}
	d.Reason = rule.Why
	switch rule.Effect {
	case scopeAll:
		d.All = true
	case scopeOwner:
		owner := graph.owner(path)
		if owner == "" {
			d.All, d.Reason = true, "under "+rule.Glob+" but in no workspace member"
			return d
		}
		d.Seeds = []string{owner}
	case scopeReaders:
		d.Seeds = append(d.Seeds, rule.Readers...)
		if owner := graph.owner(path); owner != "" {
			d.Seeds = append(d.Seeds, owner)
		}
	case scopeNothing:
	}
	return d
}

// ComputeRustTestScope answers `--print-rust-test-scope`. `base` is the git ref
// the change is measured against: the scope covers every path that differs
// between that tree and the working tree (committed, staged, unstaged, and
// untracked). In CI that's the tree being tested against the PR's base commit; a
// base that's behind only widens the scope. `base` "-" reads the changed paths
// from stdin instead, one per line. The notes say why, for stderr.
func ComputeRustTestScope(rootDir, base string, stdin io.Reader) (RustTestScope, []string, error) {
	var changed []string
	var err error
	if base == "-" {
		changed, err = readPathList(stdin)
	} else {
		changed, err = changedPathsSince(rootDir, base)
	}
	if err != nil {
		return RustTestScope{}, nil, err
	}
	graph, err := loadRustCrateGraph(rootDir)
	if err != nil {
		return RustTestScope{}, nil, err
	}
	scope, decisions := affectedRustScope(graph, rustTestScopeRules, changed)
	return scope, renderRustScopeNotes(scope, decisions, len(changed)), nil
}

// renderRustScopeNotes says what drove the scope: the paths that forced "all",
// or each seed's paths.
func renderRustScopeNotes(scope RustTestScope, decisions []rustScopeDecision, changedCount int) []string {
	notes := []string{fmt.Sprintf("%d changed %s", changedCount, Pluralize(changedCount, "path", "paths"))}
	for _, d := range decisions {
		switch {
		case !scope.Narrowed && d.All:
			notes = append(notes, fmt.Sprintf("all: %s (%s)", d.Path, d.Reason))
		case scope.Narrowed:
			notes = append(notes, fmt.Sprintf("%s: %s", strings.Join(d.Seeds, ", "), d.Path))
		}
	}
	return notes
}

// changedPathsSince lists every path that differs between base and the working
// tree, plus untracked files. `--no-renames` reports a move as both its old and
// its new path, so the crate a file left is affected too.
func changedPathsSince(rootDir, base string) ([]string, error) {
	diff, err := gitStdout(rootDir, "diff", "--name-only", "--no-renames", "-z", base, "--")
	if err != nil {
		return nil, fmt.Errorf("git diff against %s: %w", base, err)
	}
	untracked, err := gitStdout(rootDir, "ls-files", "--others", "--exclude-standard", "-z")
	if err != nil {
		return nil, err
	}
	var paths []string
	for _, chunk := range []string{diff, untracked} {
		for p := range strings.SplitSeq(chunk, "\x00") {
			if p != "" {
				paths = append(paths, p)
			}
		}
	}
	return paths, nil
}

// gitStdout runs git and returns its stdout alone: `runGit` folds stderr in, and
// a warning line there would read as a changed path.
func gitStdout(rootDir string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = rootDir
	var stdout, stderr bytes.Buffer
	if err := RunCommandTo(cmd, &stdout, &stderr); err != nil {
		return "", fmt.Errorf("git %s: %w\n%s", strings.Join(args, " "), err, indentOutput(stderr.String()))
	}
	return stdout.String(), nil
}

// readPathList reads one path per line, skipping blanks.
func readPathList(r io.Reader) ([]string, error) {
	var paths []string
	scanner := bufio.NewScanner(r)
	for scanner.Scan() {
		if line := strings.TrimSpace(scanner.Text()); line != "" {
			paths = append(paths, line)
		}
	}
	return paths, scanner.Err()
}
