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
	"pnpm-lock.yaml", // bindings-fresh and some Rust tooling resolve node deps
}

// svelteInputs mirrors ci.yml's `svelte` filter: the desktop frontend plus the
// configs and shared test/plugin dirs ESLint, Vitest, and svelte-check read.
var svelteInputs = []string{
	"apps/desktop/src/**",
	"apps/desktop/static/**",
	"apps/desktop/test/**",
	"apps/desktop/eslint-plugins/**",
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
// claude-md-reminder). `**` matches every path, so these re-run on any change.
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
