package checks

import "path/filepath"

// RunLicenseServerESLint runs ESLint on the license server.
func RunLicenseServerESLint(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "license-server")
	return runESLintCheck(ctx, dir, []string{"*.ts", "*.js"}, true)
}
