package checks

import "strings"

// Shared input-set building blocks for the input-fingerprint cache. Each check's
// Inputs field (see CheckDefinition) is the union of one of these sets plus the
// GlobalInputs every check carries implicitly. The sets are mined from ci.yml's
// dorny/paths-filter rules, which were curated for exactly this question ("which
// paths does this job's checks read?"), plus the extra dirs individual checks
// touch. Conservative by policy: when unsure, a path is included — a too-wide set
// only costs cache speed, a too-narrow one costs correctness.
//
// Build sets by concatenation at the call site (`inputs(rustCompileInputs, ...)`)
// so a path added to a base set propagates to every check that uses it. A set may
// be narrower than "everything nearby" only where a test proves the lane can't see
// what it left out: `TestRustInputsCoverEveryEmbeddedFile` for what a tree embeds,
// `TestScannerInputsMatchTheirJurisdiction` for which trees a scanner walks.
//
// A `!`-prefixed entry is an EXCLUSION: it takes matching paths back out of the
// set, whatever else matched, including out of the GlobalInputs the check carries
// implicitly. It's the one construct here that can make a set too NARROW, so each
// one names files nothing in the check's pipeline reads and carries the reasoning
// at its declaration site.

// agentDocExclusions blinds a CODE lane to the colocated agent docs living in its
// tree. House rule puts a `CLAUDE.md` + `DETAILS.md` pair beside every code area
// and has them updated on nearly every session, so without this veto a docs-only
// commit re-runs whole test suites against unchanged code. No lane that uses it
// reads one: no `include_str!` reaches a `.md` (`TestRustInputsCoverEveryEmbeddedFile`),
// nextest runs no doctests, no frontend module imports one
// (`TestNoFrontendSourceLoadsAgentDocs`), and every scanner parses source files.
//
// The veto spans the union with GlobalInputs and with the runner sources a check
// reaches, so the runner's own `CLAUDE.md` / `DETAILS.md` stay out of those lanes'
// fingerprints too. Same reasoning: the runner's docs don't change what a lane
// enforces. The doc-scanning lanes (`claude-md-length`, `docs-reachable`, …) take
// `wholeRepoInputs` and are unaffected, which is the whole point of scoping the
// veto to code lanes.
//
// Measured over the 1,439 commits of 2026-07-19..2026-08-12: the Rust lanes went
// from 62% of commits to 54%, the frontend lanes from 41.3% to 35.0% (a 15.3%
// relative cut across 21 checks that cost 59.6 h of the 24-day window).
var agentDocExclusions = []string{
	"!**/CLAUDE.md",
	"!**/DETAILS.md",
}

// GlobalInputs are the paths that affect EVERY check's fingerprint regardless of
// its own Inputs: the toolchain pin, and the runner core (everything that decides
// how a check is selected, executed, cached, and reported). Mirrors the
// ".mise.toml + ci.yml in every filter" rule in ci.yml's change-detection block.
//
// ❗ This is the runner core, NOT the whole runner. A check ALSO fingerprints the
// implementation files its own `Run` reaches, which `runner-sources.go` works out
// from the AST, so editing one check's file no longer re-runs the other 115.
// Everything here is a file no such analysis can attribute:
//
//   - `scripts/check/*.go` is package `main` (the executor, the cache plan, the
//     status line, the stats logs, the Docker orchestrator). A check can't
//     reference it, and it decides how every check runs.
//   - `registry.go` holds every check's config, `inputs.go` the shared input
//     blocks, `fingerprint.go` + `cache.go` + `runner-sources.go` the cache
//     itself, `common.go` the context and process handling every check runs
//     through, and `test-log.go` the per-test record every lane records into.
//     `fixture-stacks.go`, `smb_ports.go`, and `sftp_ports.go` are the fixture
//     vocabulary and the port env the orchestrator applies before any lane runs.
//   - `go.mod` / `go.sum` and `check.sh` build and start the runner itself.
//
// `TestGlobalInputsCoverWhatNoCheckCanReach` and
// `TestRunnerCoreCoversWhatTheExecutorReaches` prove nothing else in the runner
// tree is left unattributed, in both directions.
var GlobalInputs = []string{
	".mise.toml",
	"scripts/check.sh",
	"scripts/check/*.go",
	"scripts/check/go.mod",
	"scripts/check/go.sum",
	"scripts/check/stack-lease/**",
	"scripts/check/stacklease/**",
	"scripts/check/checks/cache.go",
	"scripts/check/checks/common.go",
	"scripts/check/checks/fingerprint.go",
	"scripts/check/checks/fixture-stacks.go",
	"scripts/check/checks/inputs.go",
	"scripts/check/checks/registry.go",
	"scripts/check/checks/runner-sources.go",
	"scripts/check/checks/sftp_ports.go",
	"scripts/check/checks/smb_ports.go",
	"scripts/check/checks/test-log.go",
}

// A workspace member as the input sets see it: the package name cargo knows it
// by, the kind that decides which scanners govern it, and the glob covering its
// tree. `Inputs` is static registry data, read before any check has a repo root
// in hand, so the table is written out rather than derived from the manifests.
// `TestRustMemberTreesMatchTheWorkspace` fails the moment it disagrees with the
// real workspace, in either direction, so a new crate can't quietly land outside
// every Rust lane's view.
type rustMemberTree struct {
	Pkg  string
	Kind MemberKind
	Glob string
}

var rustMemberTrees = []rustMemberTree{
	{Pkg: "cmdr", Kind: KindApp, Glob: "apps/desktop/src-tauri/**"},
	{Pkg: "cmdr-archive", Kind: KindApp, Glob: "crates/cmdr-archive/**"},
	{Pkg: "cmdr-fs", Kind: KindApp, Glob: "crates/cmdr-fs/**"},
	{Pkg: "cmdr-fsevent-stream", Kind: KindVendored, Glob: "crates/fsevent-stream/**"},
	{Pkg: "cmdr-index", Kind: KindApp, Glob: "crates/cmdr-index/**"},
	{Pkg: "cmdr-sftp", Kind: KindApp, Glob: "crates/cmdr-sftp/**"},
	{Pkg: "cmdr-smb", Kind: KindApp, Glob: "crates/cmdr-smb/**"},
	{Pkg: "index-query", Kind: KindTool, Glob: "crates/index-query/**"},
	{Pkg: "operation-log-dump", Kind: KindTool, Glob: "crates/operation-log-dump/**"},
}

// rustMemberGlobs returns the tree globs of every member of the given kinds, in
// table order. A scanner passes the SAME kinds it declares in
// `rustScannerJurisdictions`, so the set it fingerprints and the set it walks
// come from one decision; `TestScannerInputsMatchTheirJurisdiction` proves they
// still agree.
func rustMemberGlobs(kinds ...MemberKind) []string {
	var out []string
	for _, m := range rustMemberTrees {
		for _, k := range kinds {
			if m.Kind == k {
				out = append(out, m.Glob)
				break
			}
		}
	}
	return out
}

// rustWorkspaceConfigInputs is what every cargo command and every member-walking
// scanner resolves against: the root manifest decides who the members ARE, and
// the lockfile and toolchain pin decide what they compile to.
var rustWorkspaceConfigInputs = []string{
	"Cargo.toml",
	"Cargo.lock",
	"rust-toolchain.toml",
	// Per-test caps and test-groups: change one and the test lanes enforce
	// something different, so a cached pass from before the edit isn't one.
	".config/nextest.toml",
}

// rustEmbeddedInputs are the non-`.rs` files a member's sources pull into the
// binary with `include_str!`. `whats_new` embeds the repo-root changelog and a
// test parses the real thing, so it's a compile-time source like any `.rs` file.
//
// A lane carries this whenever its own set covers the tree that does the
// embedding, which is what `TestRustInputsCoverEveryEmbeddedFile` walks the whole
// registry to prove — per check and per member, so narrowing one lane to one
// crate cannot reopen the hole this closed. It was open once: the one shared Rust
// set didn't list `CHANGELOG.md` at all, so every Rust lane cache-skipped changelog
// edits and reported a green describing the previous content.
var rustEmbeddedInputs = []string{
	"CHANGELOG.md",
}

// rustScanInputs is what a Rust source scanner of the given jurisdiction reads:
// the trees it walks, the root manifest that decides which trees those are, and
// whatever those trees embed.
func rustScanInputs(kinds ...MemberKind) []string {
	return inputs(rustMemberGlobs(kinds...), rustWorkspaceConfigInputs, rustEmbeddedInputs, agentDocExclusions)
}

// rustAppTreeInputs is for the scanners whose jurisdiction is `AppTreeOnly`: they
// walk the app crate's `src/` and nothing else, so a crate edit can't change their
// verdict however the workspace is arranged.
var rustAppTreeInputs = inputs(
	[]string{"apps/desktop/src-tauri/**"},
	rustEmbeddedInputs,
	agentDocExclusions,
)

// macOSAvailabilityInputs is the scan set plus the file holding the floor being
// enforced: raise `minimumSystemVersion` and the same sources get a different
// verdict, so a cached pass from before that edit isn't one.
var macOSAvailabilityInputs = inputs(
	rustScanInputs(KindApp, KindTool, KindVendored),
	[]string{"apps/desktop/src-tauri/tauri.conf.json"},
	runnerDataInputs("macos-availability-selectors.json"),
)

// rustCompileInputs is what a lane that runs cargo over the whole workspace
// reads: every member's tree plus the workspace configs.
//
// ❗ It is deliberately NOT narrowed per crate. The app crate depends on all five
// library crates, so `--workspace` genuinely has to rebuild after a crate edit,
// and cargo's own incrementality already limits that rebuild to the affected
// units. Splitting the cargo lanes per package MEASURES SLOWER, and a `-p` lane
// resolves features differently from the workspace build it shares `target/`
// with. `scripts/check/DETAILS.md` § "Why the cargo lanes stay on `--workspace`"
// has the numbers.
var rustCompileInputs = inputs(
	rustMemberGlobs(KindApp, KindTool, KindVendored),
	rustWorkspaceConfigInputs,
	rustEmbeddedInputs,
	agentDocExclusions,
)

// rustFixtureServerInputs are the Docker fixture configs the integration lane runs
// against. Only that lane reads them: a change to one changes what it tests, and
// changes nothing any other Rust lane compiles or scans.
var rustFixtureServerInputs = []string{
	"apps/desktop/test/sftp-servers/**",
	"apps/desktop/test/smb-servers/**",
}

// frontendSourceRoots are the directories inside `svelteInputs` that hold code
// (as opposed to config or static assets). `TestNoFrontendSourceLoadsAgentDocs`
// walks exactly these, which is what makes `agentDocExclusions` safe for the
// frontend lanes, and `goTestsInputs` covers them because that test does.
var frontendSourceRoots = []string{
	"apps/desktop/src",
	"apps/desktop/test",
	"apps/desktop/scripts",
	"apps/desktop/eslint-plugins",
	"eslint-plugins", // the two custom rules shared with the dashboard
}

// svelteInputs mirrors ci.yml's `svelte` filter: the desktop frontend plus the
// configs and shared test/plugin dirs ESLint, Vitest, and svelte-check read.
var svelteInputs = inputs(treeGlobs(frontendSourceRoots...), []string{
	"apps/desktop/static/**",
	"apps/desktop/package.json",
	"apps/desktop/svelte.config.js",
	"apps/desktop/vite.config.js",
	"apps/desktop/vitest.config.ts",
	"apps/desktop/eslint.config.js",
	"apps/desktop/tsconfig.json",
	"apps/desktop/knip.json",
	"apps/desktop/.stylelintrc.mjs",
	"pnpm-lock.yaml",
}, agentDocExclusions)

// desktopAppInputs covers the whole desktop app (frontend + Rust workspace),
// used by the E2E checks that build the entire binary. Mirrors ci.yml's
// `desktop` filter.
func desktopAppInputs() []string {
	return inputs(
		[]string{"apps/desktop/**"},
		rustMemberGlobs(KindApp, KindTool, KindVendored),
		rustEmbeddedInputs,
		[]string{
			"Cargo.toml",
			"Cargo.lock",
			"rust-toolchain.toml",
			"pnpm-lock.yaml",
		},
	)
}

// websiteInputs mirrors ci.yml's `website` filter.
var websiteInputs = []string{
	"apps/website/**",
	".dockerignore",
	"CHANGELOG.md",
	"pnpm-lock.yaml",
}

// apiServerInputs mirrors ci.yml's `api-server` filter.
var apiServerInputs = []string{
	"apps/api-server/**",
	"pnpm-lock.yaml",
}

// dashboardInputs mirrors ci.yml's `dashboard` filter, plus the shared custom ESLint rules the
// dashboard's config imports from the repo root.
var dashboardInputs = []string{
	"apps/analytics-dashboard/**",
	"eslint-plugins/**",
	"pnpm-lock.yaml",
}

// goScriptsInputs covers the Go directories the scripts-go-* checks scan, whole.
// `scripts/check/**` is already a GlobalInput, but the wider `scripts/**` (the
// sibling tools, the shell and Node helpers) and `apps/desktop/scripts/**` are
// not. Derived from `GetGoDirectories()`, the same list the checks walk, so a
// third Go tree can't land inside the walk and outside the fingerprint.
//
// Only `misspell` takes it: misspell spell-checks every text file it walks, not
// just the Go ones.
var goScriptsInputs = treeGlobs(GetGoDirectories()...)

// goSourceInputs is what a lane that COMPILES the Go trees reads: the Go sources
// and the module files that decide how they resolve. Nothing else in those trees
// reaches the compiler — no `//go:embed`, no assembly or cgo sources, which is
// what `TestGoCompileLanesReadOnlyGoSources` checks on the real tree.
//
// The narrowing is the point. `scripts/**` also holds the JSON allowlists the
// warn-only checks shrink-wrap on nearly every local run, the colocated agent
// docs, and a pile of `.sh` / `.ts` / `.py` helpers. Over the 5,584 commits of
// 2026-02-21..2026-08-24, 357 touched a Go tree without touching one `.go` file
// there, and each re-ran ~70 s of Go linting that couldn't change verdict.
var goSourceInputs = goTreeGlobs("/**/*.go", "/**/go.mod", "/**/go.sum")

// goTreeGlobs suffixes every Go directory with each of the given patterns.
func goTreeGlobs(suffixes ...string) []string {
	var out []string
	for _, dir := range GetGoDirectories() {
		for _, suffix := range suffixes {
			out = append(out, dir+suffix)
		}
	}
	return out
}

// The SFTP fixture stack declares its machine-wide keys dir and its host ports
// in Go, and each of these files repeats them for the runner-less path
// (`start.sh` and the compose file's `${…:-default}`) or reads them back
// (`fixture_key_path`, `fixture_port`). `TestSftpFixturePathsAgree` and
// `TestSftpFixturePortsMatchComposeDefaults` are what keep the copies equal.
const (
	sftpComposeRel = "apps/desktop/test/sftp-servers/docker-compose.yml"
	sftpStartRel   = "apps/desktop/test/sftp-servers/start.sh"
	sftpTestingRel = "crates/cmdr-sftp/src/volume/testing.rs"
)

// goTestsInputs is what `scripts-go-tests` reads, which is far more than the Go
// trees. Fifteen tests in this package assert something about the REAL repo
// rather than a fixture, and each can change verdict on a file no Go linter ever
// opens. ❗ Without them the guard is a cache hit on the very change it exists to
// catch: adding a crate, adding an `include_str!`, or importing a `.md` from a
// Svelte module would all have gone green from cache.
//
//   - the cargo manifests and every member's sources (`TestRustMemberTreesMatchTheWorkspace`,
//     `TestModuleCyclesPackagesAreTheLibraryMembers`, and `TestRustInputsCoverEveryEmbeddedFile`,
//     which scans every `src/` for `include_str!` — the guard that caught the
//     `CHANGELOG.md` hole, and it was blind to the tree it scans),
//   - the frontend source roots (`TestNoFrontendSourceLoadsAgentDocs`, which is
//     what makes `agentDocExclusions` safe),
//   - `apps/desktop/package.json` (`TestBindingsRegenAsksCargoTheSameQuestionAsTheOtherLanes`),
//   - the SFTP fixture trio the fixture-path tests compare.
//
// ❗ No `agentDocExclusions` here. An exclusion vetoes across the whole union, so
// borrowing one from `rustCompileInputs` would take `scripts/check/CLAUDE.md`
// back out of a lane that walks the scripts tree.
//
// `TestGoTestsInputsCoverTheRealTreeItsTestsRead` keeps the list honest: it finds
// every test that reaches the real tree and fails on one that doesn't declare
// what it reads, or declares a path this set doesn't cover.
var goTestsInputs = inputs(
	goScriptsInputs,
	rustMemberGlobs(KindApp, KindTool, KindVendored),
	rustWorkspaceConfigInputs,
	rustEmbeddedInputs,
	treeGlobs(frontendSourceRoots...),
	[]string{"apps/desktop/package.json", sftpComposeRel, sftpStartRel, sftpTestingRel},
)

// workflowsInputs covers the GitHub workflow files the workflow-scanning checks
// read.
var workflowsInputs = []string{
	".github/workflows/**",
}

// runnerDataInputs names data files that live beside the check implementations:
// the JSON allowlists and baselines a warn-only scanner reads on every run.
// `runner-sources.go` attributes the runner's SOURCE to the checks that reach it,
// but a data file is read through a path built at runtime, so the check that owns
// one names it here. `TestAllowlistFilesAreFingerprintedByTheirCheck` fails on an
// allowlist whose check doesn't fingerprint it (a hand-edited entry would
// cache-skip the check that enforces it) and on one nothing watches at all.
func runnerDataInputs(names ...string) []string {
	out := make([]string, 0, len(names))
	for _, name := range names {
		out = append(out, strings.Join(runnerChecksDirParts, "/")+"/"+name)
	}
	return out
}

// siblingToolInputs names a helper program that lives BESIDE the runner
// (`scripts/check-css-unused`, …) and that a check shells out to. Its rules are
// outside the runner module, so neither `GlobalInputs` nor the per-check runner
// sources reach it, and the check that runs it has to say so.
// `TestSiblingToolDirsAreFingerprintedByTheirCheck` fails on a check that runs one
// without fingerprinting it, which is how these three were found cache-skipping
// their own rule engines.
func siblingToolInputs(dirs ...string) []string {
	out := make([]string, 0, len(dirs))
	for _, dir := range dirs {
		out = append(out, "scripts/"+dir)
	}
	return treeGlobs(out...)
}

// treeGlobs turns directory paths into the `dir/**` globs an input set is
// written in.
func treeGlobs(dirs ...string) []string {
	out := make([]string, 0, len(dirs))
	for _, dir := range dirs {
		out = append(out, dir+"/**")
	}
	return out
}

// wholeRepoInputs is for checks that walk the entire tree (file-length,
// claude-md-reminder, claude-md-length). `**` matches every path, so these
// re-run on any change.
// That's correct: their domain is the whole repo. They're warn-only and cheap,
// so always-running costs little.
var wholeRepoInputs = []string{"**"}

// inputs concatenates input-set slices into one fresh slice (so callers can't
// mutate a shared base set).
func inputs(sets ...[]string) []string {
	var out []string
	for _, s := range sets {
		out = append(out, s...)
	}
	return out
}
