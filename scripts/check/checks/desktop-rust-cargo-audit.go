package checks

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
)

// cargoAuditReport is the top-level JSON output from `cargo audit --json`.
type cargoAuditReport struct {
	Lockfile        struct{ DependencyCount int `json:"dependency-count"` } `json:"lockfile"`
	Vulnerabilities struct {
		Count int                    `json:"count"`
		List  []cargoAuditVulnEntry  `json:"list"`
	} `json:"vulnerabilities"`
	Warnings map[string][]cargoAuditWarningEntry `json:"warnings"`
}

type cargoAuditVulnEntry struct {
	Advisory cargoAuditAdvisory `json:"advisory"`
	Versions struct {
		Patched []string `json:"patched"`
	} `json:"versions"`
	Package cargoAuditPackage `json:"package"`
}

type cargoAuditWarningEntry struct {
	Advisory cargoAuditAdvisory `json:"advisory"`
	Package  cargoAuditPackage  `json:"package"`
}

type cargoAuditAdvisory struct {
	ID            string  `json:"id"`
	Package       string  `json:"package"`
	Title         string  `json:"title"`
	Informational *string `json:"informational"`
}

type cargoAuditPackage struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

// RunCargoAudit checks for security vulnerabilities.
func RunCargoAudit(ctx *CheckContext) (CheckResult, error) {
	// Check if cargo-audit is installed
	if !CommandExists("cargo-audit") {
		installCmd := exec.Command("cargo", "install", "cargo-audit")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-audit: %w", err)
		}
	}

	// Ignore advisories for upstream Tauri dependencies with no fix available:
	// RUSTSEC-2023-0071: rsa timing sidechannel (via sspi → smb, no fix released)
	// RUSTSEC-2024-0413..0416: gtk-rs GTK3 bindings unmaintained (used by wry/tao)
	// RUSTSEC-2024-0421..0424: gtk-sys unmaintained variants
	cmd := exec.Command("cargo", "audit", "--json",
		"--ignore", "RUSTSEC-2023-0071",
		"--ignore", "RUSTSEC-2024-0413",
		"--ignore", "RUSTSEC-2024-0414",
		"--ignore", "RUSTSEC-2024-0415",
		"--ignore", "RUSTSEC-2024-0416",
		"--ignore", "RUSTSEC-2024-0421",
		"--ignore", "RUSTSEC-2024-0422",
		"--ignore", "RUSTSEC-2024-0423",
		"--ignore", "RUSTSEC-2024-0424",
	)
	cmd.Dir = ctx.RootDir
	output, _ := RunCommand(cmd, true)
	// cargo audit exits non-zero when vulns are found, so we ignore the error
	// and parse the JSON output instead.

	var report cargoAuditReport
	if err := json.Unmarshal([]byte(output), &report); err != nil {
		return CheckResult{}, fmt.Errorf("failed to parse cargo-audit JSON output: %w\n%s", err, indentOutput(output))
	}

	// Count warnings across all categories
	totalWarnings := 0
	for _, entries := range report.Warnings {
		totalWarnings += len(entries)
	}

	// No issues at all
	if report.Vulnerabilities.Count == 0 && totalWarnings == 0 {
		return Success(fmt.Sprintf("Scanned %d %s",
			report.Lockfile.DependencyCount,
			Pluralize(report.Lockfile.DependencyCount, "crate", "crates"),
		)), nil
	}

	// Build compact summary
	var lines []string

	// Header line
	var parts []string
	if report.Vulnerabilities.Count > 0 {
		parts = append(parts, fmt.Sprintf("%d %s",
			report.Vulnerabilities.Count,
			Pluralize(report.Vulnerabilities.Count, "vulnerability", "vulnerabilities"),
		))
	}
	if totalWarnings > 0 {
		parts = append(parts, fmt.Sprintf("%d %s",
			totalWarnings,
			Pluralize(totalWarnings, "warning", "warnings"),
		))
	}
	lines = append(lines, strings.Join(parts, ", "))
	lines = append(lines, "")

	// Vulnerabilities
	for _, v := range report.Vulnerabilities.List {
		lines = append(lines, fmt.Sprintf("VULN  %s %s — %s",
			v.Package.Name, v.Package.Version, v.Advisory.Title))
		fix := "no fix available"
		if len(v.Versions.Patched) > 0 {
			fix = "upgrade to " + strings.Join(v.Versions.Patched, " or ")
		}
		lines = append(lines, fmt.Sprintf("      %s  |  %s", fix, v.Advisory.ID))
	}

	// Warnings by category
	for kind, entries := range report.Warnings {
		for _, w := range entries {
			lines = append(lines, fmt.Sprintf("WARN  %s %s — %s: %s (%s)",
				w.Package.Name, w.Package.Version, kind, w.Advisory.Title, w.Advisory.ID))
		}
	}

	summary := strings.Join(lines, "\n")

	if report.Vulnerabilities.Count > 0 {
		return CheckResult{}, fmt.Errorf("security vulnerabilities found\n%s", indentOutput(summary))
	}

	// Warnings only — not a failure
	return CheckResult{
		Code:    ResultWarning,
		Message: fmt.Sprintf("%d %s\n%s", totalWarnings, Pluralize(totalWarnings, "warning", "warnings"), indentOutput(summary)),
		Total:   -1, Issues: -1, Changes: -1,
	}, nil
}
