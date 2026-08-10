package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// RunDashboardSvelteCheck runs svelte-check for type and a11y validation on the dashboard.
//
// Uses `check:no-sync` (not `pnpm check`, which humans run standalone): the
// `dashboard-svelte-kit-sync` dependency already generated `.svelte-kit/`, and re-syncing here
// would rewrite `.svelte-kit/tsconfig.json` while the parallel `dashboard-eslint` pass reads it.
func RunDashboardSvelteCheck(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "analytics-dashboard")

	cmd := exec.Command("pnpm", "check:no-sync")
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("svelte-check failed\n%s", indentOutput(output))
	}

	lower := strings.ToLower(output)
	if strings.Contains(lower, " warning") && !strings.Contains(lower, "0 warnings") {
		return CheckResult{}, fmt.Errorf("svelte-check found warnings\n%s", indentOutput(output))
	}

	re := regexp.MustCompile(`in (\d+) files?`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("%d %s checked, no errors", count, Pluralize(count, "file", "files"))), nil
	}

	return Success("No type errors"), nil
}
