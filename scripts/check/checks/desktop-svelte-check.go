package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// RunSvelteCheck runs svelte-check for type and a11y validation.
func RunSvelteCheck(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("pnpm", "check")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("svelte-check failed\n%s", indentOutput(output))
	}

	// Check for warnings in output
	if strings.Contains(output, " warning") && !strings.Contains(output, "0 warnings") {
		return CheckResult{}, fmt.Errorf("svelte-check found warnings\n%s", indentOutput(output))
	}

	// Try to extract file count from "svelte-check found 0 errors and 0 warnings in X files"
	re := regexp.MustCompile(`in (\d+) files?`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("%d %s checked, no errors", count, Pluralize(count, "file", "files"))), nil
	}

	return Success("No type errors"), nil
}
