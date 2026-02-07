package checks

import "path/filepath"

// RunDesktopPrettier formats code with Prettier.
func RunDesktopPrettier(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "desktop")
	return runPrettierCheck(ctx, dir, []string{"*.ts", "*.svelte", "*.css", "*.js"})
}
