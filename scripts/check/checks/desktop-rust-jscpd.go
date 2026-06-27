package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
)

// jscpdVersion pins the jscpd CLI. It MUST stay pinned (repo policy:
// checks/CLAUDE.md § "every tool install pins --version"). An unpinned `npx
// jscpd` pulled the 5.x rewrite — whose CLI renamed/removed flags (`--ignore`,
// `--reporters` gone) — and the arg-parse error got misreported as a
// duplication failure, reddening CI for a no-Rust-change commit. Bump
// deliberately and re-validate the flags below against the new major.
const jscpdVersion = "4.2.3"

// RunJscpdRust detects code duplication in Rust files.
func RunJscpdRust(ctx *CheckContext) (CheckResult, error) {
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")
	jscpdSpec := "jscpd@" + jscpdVersion

	// Check if the pinned jscpd is available via npx; install it if not.
	cmd := exec.Command("npx", jscpdSpec, "--version")
	if _, err := RunCommand(cmd, true); err != nil {
		installCmd := exec.Command("npm", "install", "-g", jscpdSpec)
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install %s: %w", jscpdSpec, err)
		}
	}

	// Run jscpd on Rust source files
	cmd = exec.Command("npx", jscpdSpec,
		rustSrcDir,
		"--format", "rust",
		"--min-lines", "5",
		"--min-tokens", "100",
		"--threshold", "2",
		// Exclude test code: jscpd guards duplication in production Rust, not
		// tests (which are intentionally repetitive). These globs cover every
		// test-file convention in this repo without over-matching production names
		// like `latest.rs`: `test*.rs` (prefix), `*_test.rs` / `*_tests.rs`
		// (suffix), and `*_test_*.rs` (shared `*_test_support.rs` fixture modules).
		// Comma-separated (not a brace alternation) because jscpd splits --ignore
		// on commas, which would break a `{...}` group.
		"--ignore", "**/test*.rs,**/*_test.rs,**/*_tests.rs,**/*_test_*.rs",
		"--reporters", "console",
	)
	output, err := RunCommand(cmd, true)
	if err != nil {
		// Only the specific over-threshold marker counts as a duplication
		// failure. Matching the generic word "threshold" once misfired on the
		// 5.x `--help` usage text (which lists `--threshold`), turning a CLI
		// arg-parse error into a bogus "duplication exceeds threshold" report.
		// Anything else is a real tool error and must surface verbatim.
		if strings.Contains(output, "found too many duplicates") || strings.Contains(output, "duplicated lines") {
			return CheckResult{}, fmt.Errorf("code duplication exceeds threshold (2%%)\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("jscpd failed\n%s", indentOutput(output))
	}

	// Parse duplication percentage
	re := regexp.MustCompile(`(\d+\.?\d*)% \(`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		return Success(fmt.Sprintf("%s%% duplication", matches[1])), nil
	}
	return Success("No significant duplication"), nil
}
