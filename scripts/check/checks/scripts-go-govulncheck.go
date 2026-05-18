package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunGovulncheck scans every Go module in the repo for known vulnerabilities
// using golang.org/x/vuln/cmd/govulncheck. Static-analysis-based: only flags
// vulns reachable from your code, not every transitive dep that happens to
// have a CVE. Mirrors the role of cargo-audit on the Rust side.
//
// Most of cmdr's Go modules are dep-free tooling scripts; for those,
// govulncheck still checks reachable stdlib calls against the Go vuln DB.
func RunGovulncheck(ctx *CheckContext) (CheckResult, error) {
	govulnBin, err := EnsureGoTool("govulncheck", "golang.org/x/vuln/cmd/govulncheck@v1.3.0")
	if err != nil {
		return CheckResult{}, err
	}

	allModules, err := FindAllGoModules(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string
	modCount := 0

	for goDir, modules := range allModules {
		baseDir := filepath.Join(ctx.RootDir, goDir)
		for _, mod := range modules {
			modDir := filepath.Join(baseDir, mod)
			modLabel := filepath.Join(goDir, mod)
			modCount++

			cmd := exec.Command(govulnBin, "./...")
			cmd.Dir = modDir
			output, err := RunCommand(cmd, true)
			if err != nil {
				// govulncheck exits non-zero when vulnerabilities are found.
				issueText := strings.TrimSpace(output)
				if issueText == "" {
					issueText = err.Error()
				}
				allIssues = append(allIssues, fmt.Sprintf("[%s]\n%s", modLabel, issueText))
			}
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("govulncheck found vulnerabilities\n%s",
			indentOutput(strings.Join(allIssues, "\n")))
	}

	result := Success(fmt.Sprintf("%d %s, no vulns",
		modCount, Pluralize(modCount, "module", "modules")))
	result.Total = modCount
	return result, nil
}
