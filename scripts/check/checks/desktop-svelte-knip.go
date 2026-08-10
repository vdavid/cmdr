package checks

import (
	"path/filepath"
)

// RunKnip finds unused code, dependencies, and exports.
func RunKnip(ctx *CheckContext) (CheckResult, error) {
	return runKnipCheck(ctx, filepath.Join(ctx.RootDir, "apps", "desktop"))
}
