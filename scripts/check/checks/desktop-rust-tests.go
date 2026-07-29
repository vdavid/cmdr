package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunRustTests runs Rust tests using cargo-nextest.
func RunRustTests(ctx *CheckContext) (CheckResult, error) {
	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

	// Check if cargo-nextest is installed
	if !CommandExists("cargo-nextest") {
		installCmd := exec.Command("cargo", "install", "cargo-nextest", "--version", "0.9.136", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-nextest: %w", err)
		}
	}

	// `--features virtual-mtp` compiles in the virtual MTP device, which is the
	// only way ~29 MTP tests (backends/mtp_test, mtp_archive_test,
	// mtp_read_range_test, mtp_scan_oracle_tests, connection/path_cache_sync_test)
	// can run at all. Without it they're silently filtered out and protect
	// nothing. The feature is test-only and never enters a production build; it
	// costs ~2-4 s on a ~27 s suite.
	cmd := exec.Command("cargo", "nextest", "run", "--locked", "--features", "virtual-mtp")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		// Trim the per-test PASS/SKIP lines (the Linux lane already does): on a 4 800-test
		// suite they bury the diagnosis and the actual panics under thousands of lines.
		// FAIL/LEAK/TIMEOUT/SLOW and every panic body survive.
		return CheckResult{}, fmt.Errorf("rust tests failed\n%s", indentOutput(withFailureDiagnosis(trimRustTestProgress(output))))
	}

	// Parse test count from output: "X tests run:"
	re := regexp.MustCompile(`(\d+) tests? run`)
	matches := re.FindStringSubmatch(output)
	message := "All tests passed"
	count := -1
	if len(matches) > 1 {
		count, _ = strconv.Atoi(matches[1])
		message = fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))
	}

	// A retry-rescued run still exits 0. Report it as a warning rather than a pass:
	// the retry budget in `.config/nextest.toml` is a tolerance for real-FSEvents
	// lossiness, not a licence to hide flakes.
	if flaky := ParseFlakyTests(output); len(flaky) > 0 {
		return CheckResult{
			Code:    ResultWarning,
			Message: message + "; " + FlakySummary(flaky),
			Total:   count,
			Issues:  len(flaky),
			Changes: -1,
		}, nil
	}

	result := Success(message)
	result.Total = count
	return result, nil
}

// withFailureDiagnosis prefixes nextest output with the deadline-class breakdown, so a
// reader can tell a cap kill from an in-test `wait_until` expiry without opening a file.
// Falls through to the raw output when nothing is classifiable (a build failure, say).
func withFailureDiagnosis(output string) string {
	diagnosis := DiagnoseRustFailures(ClassifyRustFailures(output))
	if diagnosis == "" {
		return output
	}
	return diagnosis + "\n" + output
}
