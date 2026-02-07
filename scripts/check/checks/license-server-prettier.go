package checks

import "path/filepath"

// RunLicenseServerPrettier runs Prettier on the license server.
func RunLicenseServerPrettier(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "license-server")
	return runPrettierCheck(ctx, dir, []string{"*.ts", "*.js"})
}
