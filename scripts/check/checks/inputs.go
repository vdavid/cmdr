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

// rustInputs mirrors ci.yml's `rust` filter: everything the desktop Rust checks
// compile or read. The smb-servers dir is in here because the SMB integration
// tests run against those container configs.
var rustInputs = []string{
	"apps/desktop/src-tauri/**",
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
	// The agent docs colocated with the Rust trees (206 files, edited on nearly
	// every session by house rule). Nothing in a Rust lane reads them: no
	// `include_str!` reaches one (same test), nextest doesn't run doctests, and
	// the scanners parse `.rs`. Leaving them in made a docs-only pass re-run the
	// whole ~5,400-test suite. Measured over 24 days of commits: 62% of commits
	// touched this set before, 54% after.
	//
	// The veto spans the union with GlobalInputs, so this also drops
	// `scripts/check/**`'s own `CLAUDE.md` / `DETAILS.md` from every Rust lane's
	// fingerprint. That's the same reasoning: the runner's docs don't change what
	// a Rust lane enforces.
	"!**/CLAUDE.md",
	"!**/DETAILS.md",
}

// svelteInputs mirrors ci.yml's `svelte` filter: the desktop frontend plus the
// configs and shared test/plugin dirs ESLint, Vitest, and svelte-check read.
var svelteInputs = []string{
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
}

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
