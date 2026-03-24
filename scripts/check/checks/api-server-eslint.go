package checks

import "path/filepath"

// RunApiServerESLint runs ESLint on the API server.
func RunApiServerESLint(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "api-server")
	return runESLintCheck(ctx, dir, []string{"*.ts", "*.js"}, true)
}
