package checks

import (
	"path/filepath"
)

// RunStylelint validates CSS and catches undefined custom properties.
func RunStylelint(ctx *CheckContext) (CheckResult, error) {
	return runStylelintCheck(ctx, filepath.Join(ctx.RootDir, "apps", "desktop"))
}
