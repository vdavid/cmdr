package checks

import "path/filepath"

// RunWebsiteOxfmt runs oxfmt on the website.
func RunWebsiteOxfmt(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "website")
	return runOxfmtCheck(ctx, dir, []string{"*.ts", "*.astro", "*.css", "*.js"})
}
