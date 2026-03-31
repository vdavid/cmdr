package checks

import "path/filepath"

// RunApiServerOxfmt runs oxfmt on the API server.
func RunApiServerOxfmt(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "api-server")
	return runOxfmtCheck(ctx, dir, []string{"*.ts", "*.js"})
}
