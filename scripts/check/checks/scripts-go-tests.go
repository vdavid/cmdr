package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
)

// RunGoTests runs Go tests.
func RunGoTests(ctx *CheckContext) (CheckResult, error) {
	allModules, err := FindAllGoModules(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allFailures []string
	pkgCount := 0

	for goDir, modules := range allModules {
		baseDir := filepath.Join(ctx.RootDir, goDir)
		for _, mod := range modules {
			modDir := filepath.Join(baseDir, mod)
			modLabel := filepath.Join(goDir, mod)

			cmd := exec.Command("go", "test", "./...")
			cmd.Dir = modDir
			output, err := RunCommand(cmd, true)
			if err != nil {
				allFailures = append(allFailures, fmt.Sprintf("[%s]\n%s", modLabel, output))
				continue
			}

			// Count passed packages from "ok" lines
			re := regexp.MustCompile(`(?m)^ok\s+`)
			matches := re.FindAllString(output, -1)
			pkgCount += len(matches)

			// Also count "no test files" as passed packages
			noTestRe := regexp.MustCompile(`(?m)\[no test files]`)
			noTestMatches := noTestRe.FindAllString(output, -1)
			pkgCount += len(noTestMatches)
		}
	}

	if len(allFailures) > 0 {
		return CheckResult{}, fmt.Errorf("tests failed\n%s", indentOutput(strings.Join(allFailures, "\n")))
	}

	if pkgCount > 0 {
		return Success(fmt.Sprintf("%d %s passed", pkgCount, Pluralize(pkgCount, "package", "packages"))), nil
	}
	return Success("All tests passed"), nil
}
