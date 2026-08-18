package checks

import "path/filepath"

// frontendJscpdLane detects copy-paste across the desktop frontend.
//
// It's scoped to `apps/desktop/src` because that's where the frontend code is:
// 436,000 lines against roughly 12,000 each in `apps/website`, `apps/api-server`,
// and `apps/analytics-dashboard`. Those three are a deliberate blind spot — three
// more lanes to buy coverage of 3% of the frontend line count isn't a trade worth
// making until one of them grows.
var frontendJscpdLane = jscpdLane{
	checkID:       "desktop-svelte-jscpd",
	allowlistName: "jscpd-frontend",
	what:          "frontend",
	roots: func(rootDir string) ([]string, error) {
		return existingDirs([]string{filepath.Join(rootDir, "apps", "desktop", "src")}), nil
	},
	// Svelte IS covered. jscpd tokenizes a `.svelte` file into three sub-formats
	// (`typescript` for the script block, `css` for the style block, `markup` for
	// the template), so its clones are reported under those names and there is no
	// `svelte` row in the statistics — which reads exactly like "Svelte didn't
	// parse". ❌ Don't drop `svelte` from this list on that evidence; dropping it
	// makes jscpd skip every `.svelte` file outright, which is the real blind
	// spot. (Verified on jscpd 4.2.3, 2026-08-18: `getFormatByFile('a.svelte')`
	// returns `svelte`, and 46 of this lane's clones live in `.svelte` files.)
	formats:  "typescript,svelte",
	minLines: 5,
	// 75 tokens, higher-signal than the Rust lane's raw count suggests: measured
	// on this repo, 50 gives 101 clones with a 14-line median, over half of them
	// short CSS blocks that read as house style rather than copy-paste. At 75 the
	// median is 20 lines and every pair names two things that plainly do the same
	// job (`NewFolderDialog` ↔ `NewFileDialog`, `SettingCheckbox` ↔
	// `SettingSwitch`). The list is the product, so the floor is set where the
	// list stays worth reading.
	minTokens: 75,
	// Exclude test code (intentionally repetitive) and its support files. The
	// conventions in this tree: `*.test.ts`, `*.test-<something>.ts` (harnesses
	// and fixtures), `test-*.ts` helpers, and the `__mocks__` / `__fixtures__`
	// directories. `dialog-gallery/fixtures/` is deliberately NOT excluded — that
	// gallery ships.
	ignore: "**/*.test.ts,**/*.test-*.ts,**/test-*.ts,**/*.spec.ts,**/__mocks__/**,**/__fixtures__/**",
}

// RunJscpdFrontend reports copy-paste between frontend files: which two files say
// the same thing, at which lines. Warn-only, gated by
// `jscpd-frontend-allowlist.json`.
func RunJscpdFrontend(ctx *CheckContext) (CheckResult, error) {
	return runJscpdLane(ctx, frontendJscpdLane)
}
