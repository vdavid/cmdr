package checks

import "path/filepath"

// RunWebsitePrettier runs Prettier on the website.
func RunWebsitePrettier(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "website")
	return runPrettierCheck(ctx, dir, []string{"*.ts", "*.astro", "*.css", "*.js"})
}
