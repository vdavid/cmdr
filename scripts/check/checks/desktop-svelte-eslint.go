package checks

import "path/filepath"

// RunDesktopESLint lints and fixes code with ESLint.
func RunDesktopESLint(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "desktop")
	return runESLintCheck(ctx, dir, []string{"*.ts", "*.svelte", "*.js"}, false)
}
