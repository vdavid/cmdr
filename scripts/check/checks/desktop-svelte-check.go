package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunSvelteCheck runs svelte-check for type and a11y validation, on TypeScript 7
// (`--tsgo`, the side-by-side `@typescript/native` package).
//
// Uses `typecheck:no-sync`, not the `typecheck` humans run standalone: the
// `desktop-svelte-kit-sync` dependency already generated `.svelte-kit/`, and
// re-syncing here would rewrite the generated tsconfig while the parallel
// eslint-typecheck passes are reading it.
func RunSvelteCheck(ctx *CheckContext) (CheckResult, error) {
	return runSvelteCheckLane(filepath.Join(ctx.RootDir, "apps", "desktop"), true)
}

// RunSvelteCheckTS6 is the TypeScript 6 backstop for RunSvelteCheck: svelte-check's
// default in-memory mode, so a gap in the experimental `--tsgo` mode can't hide an
// error from CI.
func RunSvelteCheckTS6(ctx *CheckContext) (CheckResult, error) {
	return runSvelteCheckLane(filepath.Join(ctx.RootDir, "apps", "desktop"), false)
}

// svelteCheckSummary reads the `--output machine` summary line:
// `<ts> COMPLETED 4244 FILES 0 ERRORS 0 WARNINGS 0 FILES_WITH_PROBLEMS`.
var svelteCheckSummary = regexp.MustCompile(`COMPLETED (\d+) FILES (\d+) ERRORS (\d+) WARNINGS`)

// runSvelteCheckLane runs one app's `typecheck:no-sync`, on TS 7 via `--tsgo` or on
// TS 6, and fails on any error or warning.
//
// It asks for `--output machine` and reads the counts off the summary line, failing
// closed when there's none: matching prose ("0 warnings") let "10 warnings" pass.
func runSvelteCheckLane(appDir string, tsgo bool) (CheckResult, error) {
	args := []string{"typecheck:no-sync", "--output", "machine"}
	if tsgo {
		args = append(args, "--tsgo")
	}
	cmd := exec.Command("pnpm", args...)
	cmd.Dir = appDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("svelte-check failed\n%s", indentOutput(output))
	}

	m := svelteCheckSummary.FindStringSubmatch(output)
	if m == nil {
		return CheckResult{}, fmt.Errorf("svelte-check printed no summary line, so its result can't be trusted\n%s", indentOutput(output))
	}
	files, _ := strconv.Atoi(m[1])
	warnings, _ := strconv.Atoi(m[3])
	if warnings > 0 {
		return CheckResult{}, fmt.Errorf("svelte-check found %d %s\n%s", warnings, Pluralize(warnings, "warning", "warnings"), indentOutput(output))
	}

	// In `--tsgo` mode the summary's file count is svelte-check's own emit set plus
	// any file with a diagnostic (442 against TS 6's 4,244 on desktop, 2026-11-10),
	// not the program tsgo type-checked, so the message leaves it out.
	if tsgo {
		return Success("No type errors or warnings on TypeScript 7 (--tsgo)"), nil
	}
	return Success(fmt.Sprintf("%s %s checked on TypeScript 6, no errors or warnings",
		FormatThousands(files), Pluralize(files, "file", "files"))), nil
}
