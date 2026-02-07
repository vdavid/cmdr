package checks

import "path/filepath"

// RunWebsiteESLint runs ESLint on the website.
func RunWebsiteESLint(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "website")
	return runESLintCheck(ctx, dir, []string{"*.ts", "*.astro", "*.js"}, true)
}
