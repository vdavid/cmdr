package checks

import "path/filepath"

// RunApiServerPrettier runs Prettier on the API server.
func RunApiServerPrettier(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "api-server")
	return runPrettierCheck(ctx, dir, []string{"*.ts", "*.js"})
}
