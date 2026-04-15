package checks

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
)

// cargoAuditReport is the top-level JSON output from `cargo audit --json`.
type cargoAuditReport struct {
	Lockfile struct {
		DependencyCount int `json:"dependency-count"`
	} `json:"lockfile"`
	Vulnerabilities struct {
		Count int                   `json:"count"`
		List  []cargoAuditVulnEntry `json:"list"`
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
	ID    string `json:"id"`
	Title string `json:"title"`
}

type cargoAuditPackage struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

// Advisories for upstream transitive dependencies with no fix available.
// All of these are pulled in by Tauri/wry/tao and can't be resolved by us.
var cargoAuditIgnoredAdvisories = []string{
	"RUSTSEC-2023-0071", // rsa timing sidechannel (via sspi → smb, no fix released)
	"RUSTSEC-2024-0370", // proc-macro-error unmaintained (via gtk3-macros, glib-macros)
	"RUSTSEC-2024-0411", // gdkwayland-sys unmaintained
	"RUSTSEC-2024-0412", // gdk unmaintained
	"RUSTSEC-2024-0413", // gtk-rs GTK3 unmaintained
	"RUSTSEC-2024-0414", // gtk-rs GTK3 unmaintained
	"RUSTSEC-2024-0415", // gtk-rs GTK3 unmaintained
	"RUSTSEC-2024-0416", // gtk-rs GTK3 unmaintained
	"RUSTSEC-2024-0417", // gdkx11 unmaintained
	"RUSTSEC-2024-0418", // gdk-sys unmaintained
	"RUSTSEC-2024-0419", // gtk3-macros unmaintained
	"RUSTSEC-2024-0420", // gtk-sys unmaintained
	"RUSTSEC-2024-0421", // gtk-sys unmaintained
	"RUSTSEC-2024-0422", // gtk-sys unmaintained
	"RUSTSEC-2024-0423", // gtk-sys unmaintained
	"RUSTSEC-2024-0424", // gtk-sys unmaintained
	"RUSTSEC-2024-0429", // glib unsound VariantStrIter (via GTK3 bindings)
	"RUSTSEC-2024-0436", // paste unmaintained (via rav1e → ravif → image)
	"RUSTSEC-2025-0052", // async-std unmaintained (dev-dep only, not shipped)
	"RUSTSEC-2025-0057", // fxhash unmaintained (via tauri-utils → kuchikiki)
	"RUSTSEC-2025-0075", // unic-char-range unmaintained (via tauri-utils → urlpattern)
	"RUSTSEC-2025-0080", // unic-common unmaintained
	"RUSTSEC-2025-0081", // unic-char-property unmaintained
	"RUSTSEC-2025-0098", // unic-ucd-version unmaintained
	"RUSTSEC-2025-0100", // unic-ucd-ident unmaintained
	"RUSTSEC-2026-0097", // rand unsound (0.7/0.8, not using rand::rng())
}

// buildCargoAuditCmd constructs the cargo audit command with --json and all ignores.
func buildCargoAuditCmd() *exec.Cmd {
	args := []string{"audit", "--json"}
	for _, id := range cargoAuditIgnoredAdvisories {
		args = append(args, "--ignore", id)
	}
	return exec.Command("cargo", args...)
}

// formatAuditReport builds a compact one-line-per-advisory summary.
func formatAuditReport(report cargoAuditReport) string {
	var lines []string

	// Header
	var parts []string
	if report.Vulnerabilities.Count > 0 {
		parts = append(parts, fmt.Sprintf("%d %s",
			report.Vulnerabilities.Count,
			Pluralize(report.Vulnerabilities.Count, "vulnerability", "vulnerabilities")))
	}
	totalWarnings := countAuditWarnings(report)
	if totalWarnings > 0 {
		parts = append(parts, fmt.Sprintf("%d %s",
			totalWarnings, Pluralize(totalWarnings, "warning", "warnings")))
	}
	lines = append(lines, strings.Join(parts, ", "), "")

	for _, v := range report.Vulnerabilities.List {
		lines = append(lines, fmt.Sprintf("VULN  %s %s — %s", v.Package.Name, v.Package.Version, v.Advisory.Title))
		fix := "no fix available"
		if len(v.Versions.Patched) > 0 {
			fix = "upgrade to " + strings.Join(v.Versions.Patched, " or ")
		}
		lines = append(lines, fmt.Sprintf("      %s  |  %s", fix, v.Advisory.ID))
	}
	for kind, entries := range report.Warnings {
		for _, w := range entries {
			lines = append(lines, fmt.Sprintf("WARN  %s %s — %s: %s (%s)",
				w.Package.Name, w.Package.Version, kind, w.Advisory.Title, w.Advisory.ID))
		}
	}
	return strings.Join(lines, "\n")
}

func countAuditWarnings(report cargoAuditReport) int {
	total := 0
	for _, entries := range report.Warnings {
		total += len(entries)
	}
	return total
}

// parseAuditJSON extracts and parses the JSON object from cargo-audit output.
// RunCommand concatenates stderr after stdout, so we find the JSON boundaries.
func parseAuditJSON(output string) (cargoAuditReport, error) {
	jsonStart := strings.Index(output, "{")
	jsonEnd := strings.LastIndex(output, "}")
	if jsonStart < 0 || jsonEnd < 0 || jsonEnd < jsonStart {
		return cargoAuditReport{}, fmt.Errorf("no JSON found in cargo-audit output\n%s", indentOutput(output))
	}
	var report cargoAuditReport
	if err := json.Unmarshal([]byte(output[jsonStart:jsonEnd+1]), &report); err != nil {
		return cargoAuditReport{}, fmt.Errorf("failed to parse cargo-audit JSON: %w\n%s", err, indentOutput(output))
	}
	return report, nil
}

// RunCargoAudit checks for security vulnerabilities.
func RunCargoAudit(ctx *CheckContext) (CheckResult, error) {
	if !CommandExists("cargo-audit") {
		installCmd := exec.Command("cargo", "install", "cargo-audit")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-audit: %w", err)
		}
	}

	cmd := buildCargoAuditCmd()
	cmd.Dir = ctx.RootDir
	output, _ := RunCommand(cmd, true)

	report, err := parseAuditJSON(output)
	if err != nil {
		return CheckResult{}, err
	}

	if report.Vulnerabilities.Count == 0 && countAuditWarnings(report) == 0 {
		return Success(fmt.Sprintf("Scanned %d %s",
			report.Lockfile.DependencyCount,
			Pluralize(report.Lockfile.DependencyCount, "crate", "crates"))), nil
	}

	summary := formatAuditReport(report)
	if report.Vulnerabilities.Count > 0 {
		return CheckResult{}, fmt.Errorf("security vulnerabilities found\n%s", indentOutput(summary))
	}

	totalWarnings := countAuditWarnings(report)
	return CheckResult{
		Code:    ResultWarning,
		Message: fmt.Sprintf("%d %s\n%s", totalWarnings, Pluralize(totalWarnings, "warning", "warnings"), indentOutput(summary)),
		Total:   -1, Issues: -1, Changes: -1,
	}, nil
}
