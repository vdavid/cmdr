package checks

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// AuditResult represents the JSON output from pnpm audit.
type AuditResult struct {
	Advisories map[string]Advisory `json:"advisories"`
	Metadata   AuditMetadata       `json:"metadata"`
}

// Advisory represents a single vulnerability advisory.
type Advisory struct {
	ModuleName         string    `json:"module_name"`
	Severity           string    `json:"severity"`
	Title              string    `json:"title"`
	VulnerableVersions string    `json:"vulnerable_versions"`
	PatchedVersions    string    `json:"patched_versions"`
	Findings           []Finding `json:"findings"`
	URL                string    `json:"url"`
}

// Finding represents where a vulnerable package was found.
type Finding struct {
	Version string   `json:"version"`
	Paths   []string `json:"paths"`
}

// AuditMetadata contains summary counts.
type AuditMetadata struct {
	Vulnerabilities   VulnerabilityCounts `json:"vulnerabilities"`
	Dependencies      int                 `json:"dependencies"`
	TotalDependencies int                 `json:"totalDependencies"`
}

// VulnerabilityCounts contains counts by severity.
type VulnerabilityCounts struct {
	Info     int `json:"info"`
	Low      int `json:"low"`
	Moderate int `json:"moderate"`
	High     int `json:"high"`
	Critical int `json:"critical"`
}

// RunPnpmAudit runs pnpm audit for production dependencies only.
// Dev dependencies are excluded since they don't ship to users.
func RunPnpmAudit(ctx *CheckContext) (CheckResult, error) {
	cmd := exec.Command("pnpm", "audit", "--prod", "--json")
	cmd.Dir = ctx.RootDir
	output, _ := RunCommand(cmd, true) // pnpm audit exits non-zero if vulns found

	var result AuditResult
	if err := json.Unmarshal([]byte(output), &result); err != nil {
		return CheckResult{}, fmt.Errorf("failed to parse pnpm audit output: %w\n%s", err, output)
	}

	total := result.Metadata.Vulnerabilities.Critical +
		result.Metadata.Vulnerabilities.High +
		result.Metadata.Vulnerabilities.Moderate +
		result.Metadata.Vulnerabilities.Low +
		result.Metadata.Vulnerabilities.Info

	if total == 0 {
		deps := result.Metadata.Dependencies
		return Success(fmt.Sprintf("%d production %s checked", deps, Pluralize(deps, "dep", "deps"))), nil
	}

	// Build concise summary
	var sb strings.Builder
	sb.WriteString(formatVulnSummary(result.Metadata.Vulnerabilities))
	sb.WriteString("\n")

	// Group advisories by severity for better output
	bySeverity := groupBySeverity(result.Advisories)

	// Output in severity order: critical, high, moderate, low, info
	for _, severity := range []string{"critical", "high", "moderate", "low", "info"} {
		advisories := bySeverity[severity]
		if len(advisories) == 0 {
			continue
		}

		for _, adv := range advisories {
			// Find the shortest path for each advisory
			shortestPath := findShortestPath(adv)
			depth := strings.Count(shortestPath, ">") + 1

			fixable := ""
			if adv.PatchedVersions != "" && adv.PatchedVersions != "<0.0.0" {
				if depth <= 2 {
					fixable = " [fixable]"
				} else {
					fixable = " [transitive]"
				}
			}

			sb.WriteString(fmt.Sprintf("  %s %s%s: %s\n",
				severityIcon(severity),
				adv.ModuleName,
				fixable,
				truncate(adv.Title, 60)))
			sb.WriteString(fmt.Sprintf("    → %s\n", shortestPath))
		}
	}

	return CheckResult{}, fmt.Errorf("found %d production %s\n%s",
		total,
		Pluralize(total, "vulnerability", "vulnerabilities"),
		sb.String())
}

func formatVulnSummary(v VulnerabilityCounts) string {
	var parts []string
	if v.Critical > 0 {
		parts = append(parts, fmt.Sprintf("%d critical", v.Critical))
	}
	if v.High > 0 {
		parts = append(parts, fmt.Sprintf("%d high", v.High))
	}
	if v.Moderate > 0 {
		parts = append(parts, fmt.Sprintf("%d moderate", v.Moderate))
	}
	if v.Low > 0 {
		parts = append(parts, fmt.Sprintf("%d low", v.Low))
	}
	if v.Info > 0 {
		parts = append(parts, fmt.Sprintf("%d info", v.Info))
	}
	return strings.Join(parts, ", ")
}

func groupBySeverity(advisories map[string]Advisory) map[string][]Advisory {
	result := make(map[string][]Advisory)
	for _, adv := range advisories {
		result[adv.Severity] = append(result[adv.Severity], adv)
	}
	// Sort each group by module name for consistent output
	for severity := range result {
		sort.Slice(result[severity], func(i, j int) bool {
			return result[severity][i].ModuleName < result[severity][j].ModuleName
		})
	}
	return result
}

func findShortestPath(adv Advisory) string {
	shortest := ""
	for _, finding := range adv.Findings {
		for _, path := range finding.Paths {
			if shortest == "" || len(path) < len(shortest) {
				shortest = path
			}
		}
	}
	return shortest
}

func severityIcon(severity string) string {
	switch severity {
	case "critical":
		return "🔴"
	case "high":
		return "🟠"
	case "moderate":
		return "🟡"
	case "low":
		return "🔵"
	default:
		return "⚪"
	}
}

func truncate(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	return s[:maxLen-3] + "..."
}

// EnsurePnpmDependencies runs pnpm install to ensure all dependencies are installed.
// In CI mode, uses --frozen-lockfile to fail if lockfile is out of sync.
func EnsurePnpmDependencies(ctx *CheckContext) error {
	args := []string{"install"}
	if ctx.CI {
		args = append(args, "--frozen-lockfile")
	}

	cmd := exec.Command("pnpm", args...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return fmt.Errorf("pnpm install failed:\n%s", indentOutput(output))
	}
	return nil
}

// GetPnpmApps returns the apps that use pnpm (have a package.json).
func GetPnpmApps(rootDir string) []string {
	apps := []string{"desktop", "website", "license-server"}
	var result []string
	for _, app := range apps {
		pkgPath := filepath.Join(rootDir, "apps", app, "package.json")
		if fileExists(pkgPath) {
			result = append(result, app)
		}
	}
	return result
}

func fileExists(path string) bool {
	cmd := exec.Command("test", "-f", path)
	return cmd.Run() == nil
}
