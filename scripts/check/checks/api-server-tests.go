package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunApiServerTests runs tests on the API server.
func RunApiServerTests(ctx *CheckContext) (CheckResult, error) {
	serverDir := filepath.Join(ctx.RootDir, "apps", "api-server")

	cmd := exec.Command("pnpm", "test")
	cmd.Dir = serverDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("tests failed\n%s", indentOutput(output))
	}

	// Extract test count
	re := regexp.MustCompile(`Tests\s+(\d+) passed`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		result := Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests")))
		result.Total = count
		return result, nil
	}
	return Success("All tests passed"), nil
}
