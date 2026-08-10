package checks

import (
	"path/filepath"
)

// RunImportCycles uses oxlint's import plugin to detect circular imports in TypeScript/Svelte code.
func RunImportCycles(ctx *CheckContext) (CheckResult, error) {
	return runImportCyclesCheck(ctx, filepath.Join(ctx.RootDir, "apps", "desktop"))
}
