package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunTypeDrift detects drift between Rust and TypeScript type definitions.
func RunTypeDrift(ctx *CheckContext) (CheckResult, error) {
	cmd := exec.Command("pnpm", "check:type-drift")
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("type drift detected between Rust and TypeScript\n%s", indentOutput(output))
	}

	// Try to extract type count from output (e.g., "Checked 42 types")
	re := regexp.MustCompile(`(\d+) types?`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("%d %s in sync", count, Pluralize(count, "type", "types"))), nil
	}

	return Success("All types in sync"), nil
}
