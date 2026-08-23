package checks

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
// The veto spans the union with GlobalInputs, so it also drops
// `scripts/check/**`'s own `CLAUDE.md` / `DETAILS.md` from those lanes'
// fingerprints. Same reasoning: the runner's docs don't change what a lane
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

// svelteInputs mirrors ci.yml's `svelte` filter: the desktop frontend plus the
// configs and shared test/plugin dirs ESLint, Vitest, and svelte-check read.
var svelteInputs = inputs([]string{
	"apps/desktop/src/**",
	"apps/desktop/static/**",
	"apps/desktop/test/**",
	"apps/desktop/eslint-plugins/**",
	"eslint-plugins/**", // the two custom rules shared with the dashboard
	"apps/desktop/scripts/**",
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

// goScriptsInputs covers the Go directories the scripts-go-* checks scan
// (GetGoDirectories: scripts/ and apps/desktop/scripts/). scripts/check/** is
// already a GlobalInput, but scripts/** (the wider set: check-css-unused, etc.)
// and apps/desktop/scripts/** are not, so they're listed explicitly.
var goScriptsInputs = []string{
	"scripts/**",
	"apps/desktop/scripts/**",
}

// workflowsInputs covers the GitHub workflow files the workflow-scanning checks
// read.
var workflowsInputs = []string{
	".github/workflows/**",
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
