package checks

import (
	"fmt"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// The full (type-aware) ESLint config is split into TWO passes — one over
// `.svelte` files, one over everything else (TS + JS/config) — because linting
// `.svelte` and `.ts` together in a single `eslint .` invocation hits a
// typescript-eslint projectService cliff: interleaving svelte-eslint-parser
// virtual files with `.ts` files thrashes the TS program and runs ~25x slower
// (~616s combined vs ~15s + ~10s split). Both passes use the SAME config,
// rules, and tsconfig-built projectService, so coverage is identical — each
// resolves cross-file types fully (verified with planted cross-file
// `no-floating-promises` violations). Run as two parallel checks, this keeps
// type-aware linting in the default (non-slow) suite. See
// docs/notes/check-cpu-contention.md.
//
// These two lanes are the desktop's whole ESLint coverage: the full config is a
// superset of the `ESLINT_NO_TYPECHECK=1` one (it swaps `strict` for
// `strictTypeChecked` and adds rules and unused-directive reporting, never
// removes any). A red local run also auto-fixes what's safe, scoped to the lane's
// files (`typeAwareFixPasses`).

// RunDesktopESLintTypecheckSvelte runs the full type-aware ESLint config over
// the `.svelte` files only.
func RunDesktopESLintTypecheckSvelte(ctx *CheckContext) (CheckResult, error) {
	return runScopedESLintTypecheck(ctx, "Svelte", []string{"**/*.svelte"}, []string{"*.svelte"}, svelteLintScope)
}

// svelteLintScope and typescriptLintScope are the repo-relative globs each lane's fix pass can
// rewrite, mirroring its ESLint scope args. They partition `apps/desktop` the same way, so one
// lane's fixes never read as a change in the other's snapshot.
var (
	svelteLintScope     = []string{"apps/desktop/**/*.svelte"}
	typescriptLintScope = []string{"apps/desktop/**", "!apps/desktop/**/*.svelte"}
)

// RunDesktopESLintTypecheckTypescript runs the full type-aware ESLint config
// over everything EXCEPT `.svelte` (TS, plus the handful of JS/config files
// `eslint .` lints). Together with the Svelte pass this is an exact partition
// of `eslint .`'s file set, so nothing drops out of coverage.
func RunDesktopESLintTypecheckTypescript(ctx *CheckContext) (CheckResult, error) {
	return runScopedESLintTypecheck(ctx, "TypeScript", []string{".", "--ignore-pattern", "**/*.svelte"}, []string{"*.ts"}, typescriptLintScope)
}

// lintOutcome is one verdict pass's output and exit status.
type lintOutcome struct {
	output string
	err    error
}

// fixThenReverdict runs a red lane's fix pass and judges again only if that pass rewrote one of
// the lane's files, compared by content before and after (never by parsing ESLint's output).
// Type-aware findings never auto-fix, so on most red runs the fix pass changes nothing and a
// second verdict would lint identical content for the identical answer: the first one stands.
// A snapshot that can't be taken counts as a change, so the verdict runs again. It reports
// whether the verdict ran again.
func fixThenReverdict(first lintOutcome, fix func(), verdict func() lintOutcome, snapshot func() (map[string]string, error)) (lintOutcome, bool) {
	before, beforeErr := snapshot()
	fix()
	after, afterErr := snapshot()
	if beforeErr == nil && afterErr == nil && maps.Equal(before, after) {
		return first, false
	}
	return verdict(), true
}

// lintScopeSnapshot maps every file under scope whose working-tree content differs from the git
// index to its content hash (deleted files to ""). A fix pass only rewrites files in place, so
// any file it touches lands here or changes its hash. A fix to a gitignored file would go
// unseen, which errs safe: the first (red) verdict stands, and the next run judges again.
func lintScopeSnapshot(rootDir string, scope []string) (map[string]string, error) {
	statusOut, err := runGit(rootDir, "status", "--porcelain", "-z", "--untracked-files=all")
	if err != nil {
		return nil, err
	}
	data := &RepoFingerprintData{dirtyContentHashes: map[string]string{}, deleted: map[string]bool{}}
	if err := parseStatusAndHashDirty(rootDir, statusOut, data); err != nil {
		return nil, err
	}
	snap := map[string]string{}
	for path, h := range data.dirtyContentHashes {
		if matchesAny(path, scope) {
			snap[path] = h
		}
	}
	for path := range data.deleted {
		if matchesAny(path, scope) {
			snap[path] = ""
		}
	}
	return snap, nil
}

// runScopedESLintTypecheck runs `eslint <scopeArgs>` with the full config (no
// ESLINT_NO_TYPECHECK) as the verdict; on a red local run it runs the safe fix
// pass and, if that rewrote any of the lane's files, the verdict again
// (`fixThenReverdict`). `eslint-passes.go` says why type-aware rules
// never auto-fix. countExts feeds an approximate src file count for the
// success message.
func runScopedESLintTypecheck(ctx *CheckContext, label string, scopeArgs, countExts, snapshotScope []string) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "desktop")

	// Defense in depth: the type-aware config extends `$app/tsconfig`
	// (generated by `svelte-kit sync` into `node_modules/$app/tsconfig.json`). If it's missing, the projectService can't
	// build a TypeScript program, the type-aware rules silently pass, and the
	// `--fix` below would delete their now-"unused" eslint-disable directives
	// across the repo. Refuse to run rather than corrupt source. The
	// `desktop-svelte-kit-sync` check (a dependency) normally guarantees this exists.
	if _, statErr := os.Stat(filepath.Join(dir, "node_modules", "$app", "tsconfig.json")); os.IsNotExist(statErr) {
		return CheckResult{}, fmt.Errorf("%s: node_modules/$app/tsconfig.json missing — run `pnpm exec svelte-kit sync` first (the desktop-svelte-kit-sync check should have)", label)
	}

	// Approximate file count for the success message (src only; informational).
	findCmd := exec.Command("find", buildFindArgs("src", countExts)...)
	findCmd.Dir = dir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	verdictPasses := typeAwareESLintPasses(scopeArgs)
	verdict := func() lintOutcome {
		out, runErr := runESLintPasses(dir, verdictPasses)
		return lintOutcome{output: out, err: runErr}
	}
	outcome := verdict()
	fixed := false
	if fix := typeAwareFixPasses(scopeArgs, ctx.CI); outcome.err != nil && len(fix) > 0 {
		// The fix pass's own exit code says nothing new: the re-run verdict reports whatever
		// it couldn't fix, alongside the type-aware findings.
		runFix := func() { _, _ = runESLintPasses(dir, fix) }
		snapshot := func() (map[string]string, error) { return lintScopeSnapshot(ctx.RootDir, snapshotScope) }
		outcome, _ = fixThenReverdict(outcome, runFix, verdict, snapshot)
		fixed = outcome.err == nil
	}
	output, err := outcome.output, outcome.err
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("lint errors (%s), run `pnpm check eslint-typecheck-svelte eslint-typecheck-ts` locally to auto-fix what's safe\n%s", label, indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("lint errors (%s) the safe auto-fix couldn't resolve; type-aware findings never auto-fix (#622), so fix them by hand\n%s", label, indentOutput(output))
	}

	if fixed {
		return SuccessWithChanges(fmt.Sprintf("%s files auto-fixed, then passed", label)), nil
	}
	if fileCount > 0 {
		result := Success(fmt.Sprintf("%d %s %s passed", fileCount, label, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		return result, nil
	}
	return Success(fmt.Sprintf("%s files passed", label)), nil
}
