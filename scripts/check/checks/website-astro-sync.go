package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunWebsiteAstroSync runs `astro sync`, which generates `apps/website/.astro/`
// (including `types.d.ts`, where the `astro:content` module and the blog
// collection's schema types live). The directory is gitignored.
//
// On a fresh tree (a new clone or worktree) it doesn't exist until something
// syncs, and until then `getCollection('blog')` resolves to `any`. The
// type-aware ESLint rules then report every use of a post as an
// `no-unsafe-member-access` / `no-unsafe-assignment` violation: 90 of them
// across five files, all false. `website-eslint` depends on this check so it
// always runs first.
func RunWebsiteAstroSync(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "website")
	cmd := exec.Command("pnpm", "exec", "astro", "sync")
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("astro sync failed\n%s", indentOutput(output))
	}
	return Success("Generated .astro"), nil
}
