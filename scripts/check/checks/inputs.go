package checks

// Shared input-set building blocks for the input-fingerprint cache. Each check's
// Inputs field (see CheckDefinition) is the union of one of these sets plus the
// GlobalInputs every check carries implicitly. The sets are mined from ci.yml's
// dorny/paths-filter rules, which were curated for exactly this question ("which
// paths does this job's checks read?"), plus the extra dirs individual checks
// touch. Conservative by policy: when unsure, a path is included — a too-wide set
// only costs cache speed, a too-narrow one costs correctness.
//
// Build sets by concatenation at the call site (`inputs(rustInputs, ...)`) so a
// path added to a base set propagates to every check that uses it.
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

// rustInputs mirrors ci.yml's `rust` filter: everything the desktop Rust checks
// compile or read. Both fixture-server dirs are in here because the integration
// lane runs against those container configs, and a change to one is a change to
// what the lane tests.
var rustInputs = inputs([]string{
	"apps/desktop/src-tauri/**",
	"apps/desktop/test/sftp-servers/**",
	"apps/desktop/test/smb-servers/**",
	"crates/**",
	"tools/**",
	"Cargo.toml",
	"Cargo.lock",
	"rust-toolchain.toml",
	// The workspace-root format/lint/policy config. Editing any of these changes
	// what every Rust lane enforces, so a cached pass from before the edit is a
	// pass against different rules.
	"rustfmt.toml",
	"clippy.toml",
	"deny.toml",
	"pnpm-lock.yaml", // bindings-fresh and some Rust tooling resolve node deps
	// `whats_new` pulls the changelog into the binary with `include_str!`, and a
	// test parses the real thing, so it's a compile-time source like any `.rs`.
	// `TestRustInputsCoverEveryEmbeddedFile` finds the next file that becomes one.
	"CHANGELOG.md",
}, agentDocExclusions)

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
	return inputs([]string{"apps/desktop/**"}, []string{
		"crates/**",
		"tools/**",
		"Cargo.toml",
		"Cargo.lock",
		"rust-toolchain.toml",
		"pnpm-lock.yaml",
	})
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
