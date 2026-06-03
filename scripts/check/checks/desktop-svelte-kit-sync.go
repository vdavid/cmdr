package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopSvelteKitSync runs `svelte-kit sync`, which generates
// `apps/desktop/.svelte-kit/tsconfig.json`. That file is gitignored, and
// `apps/desktop/tsconfig.json` extends it, so the type-aware ESLint passes and
// svelte-check can't build a TypeScript program without it.
//
// On a fresh tree (a new clone or worktree) the file doesn't exist until
// something syncs. The type-aware checks depend on this one so it always runs
// first. Without that ordering, the type-aware `eslint --fix` pass would run
// against a broken projectService: every imported type resolves to "could not
// be resolved", the type-aware rules go silent, their `eslint-disable`
// directives look unused, and `--fix` deletes them across the repo. See the
// Decision in checks/CLAUDE.md.
func RunDesktopSvelteKitSync(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "desktop")
	cmd := exec.Command("pnpm", "exec", "svelte-kit", "sync")
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("svelte-kit sync failed\n%s", indentOutput(output))
	}
	return Success("Generated .svelte-kit"), nil
}
