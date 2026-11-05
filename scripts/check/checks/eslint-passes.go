package checks

import (
	"os"
	"os/exec"
)

// Every ESLint lane plans its invocations here so one invariant holds everywhere: `--fix` runs
// only with the type-aware rules OFF and never touches eslint-disable directives. A type-aware
// rule judges code against the TypeScript program, so while the tree doesn't typecheck (another
// agent mid-edit in a shared worktree, a half-regenerated `bindings.ts`) it misjudges correct
// code and `--fix` rewrites it. Type-aware findings are reported, check-only, and fixed by hand.
// `TestESLintPassesNeverFixWithTypeAwareRules` pins it; DETAILS § "ESLint never auto-fixes with
// type information".

// eslintPass is one `pnpm exec eslint <args>` invocation an ESLint lane makes.
type eslintPass struct {
	args []string
	// noTypecheck sets ESLINT_NO_TYPECHECK=1, which every app's `eslint.config.js` reads to turn
	// off the type-aware rules (and unused-directive reporting) and skip the projectService.
	noTypecheck bool
}

// safeFixArgs auto-fixes every fix type except `directive`, so a fix pass can never delete an
// eslint-disable comment, even one a misconfigured rule set makes look unused.
var safeFixArgs = []string{"--fix", "--fix-type", "problem,suggestion,layout"}

// appESLintPasses plans `runESLintCheck`'s lane, which owns an app's whole config: locally, a
// fix pass without type information, then the full config check-only.
func appESLintPasses(ci bool) []eslintPass {
	check := eslintPass{args: []string{"."}}
	if ci {
		return []eslintPass{check}
	}
	return []eslintPass{{args: append([]string{"."}, safeFixArgs...), noTypecheck: true}, check}
}

// typeAwareESLintPasses plans the verdict of one of the desktop's two type-aware lanes over
// scopeArgs: the full config, check-only in every mode. The two scopes partition `eslint .`, so
// together they cover everything the config minus its type-aware rules would.
func typeAwareESLintPasses(scopeArgs []string) []eslintPass {
	return []eslintPass{{args: append([]string{}, scopeArgs...)}}
}

// typeAwareFixPasses plans what a desktop type-aware lane runs when its verdict is red on a local
// run: a fix pass without type information over the lane's own scope (so the Svelte and
// TypeScript lanes, running in parallel, never fix the same file), after which the lane re-runs
// its verdict if the fix pass changed any of its files (`fixThenReverdict`). Running it only on red keeps a green run at one ESLint invocation. CI never fixes.
func typeAwareFixPasses(scopeArgs []string, ci bool) []eslintPass {
	if ci {
		return nil
	}
	return []eslintPass{{args: append(append([]string{}, scopeArgs...), safeFixArgs...), noTypecheck: true}}
}

// runESLintPasses runs every pass in order in dir. The final pass is the verdict: a fix pass exits
// non-zero on any unfixable finding, which the final full-config pass reports again alongside the
// type-aware ones. An earlier failure surfaces only when the final pass is green (a fix-mode-only
// breakage, like a config that crashes without type information).
func runESLintPasses(dir string, passes []eslintPass) (string, error) {
	var firstOutput, output string
	var firstErr, err error
	for _, p := range passes {
		cmd := exec.Command("pnpm", append([]string{"exec", "eslint"}, p.args...)...)
		cmd.Dir = dir
		if p.noTypecheck {
			cmd.Env = append(os.Environ(), "ESLINT_NO_TYPECHECK=1")
		}
		output, err = RunCommand(cmd, true)
		if err != nil && firstErr == nil {
			firstOutput, firstErr = output, err
		}
	}
	if err == nil && firstErr != nil {
		return firstOutput, firstErr
	}
	return output, err
}
