package checks

import "path/filepath"

// RunDesktopOxfmt runs oxfmt on the desktop app.
func RunDesktopOxfmt(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "desktop")
	return runOxfmtCheck(ctx, dir, []string{"*.ts", "*.svelte", "*.css", "*.js"})
}
