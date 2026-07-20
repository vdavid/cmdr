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
		return CheckResult{}, fmt.Errorf("rust tests failed\n%s", indentOutput(output))
	}

	// Parse test count from output: "X tests run:"
	re := regexp.MustCompile(`(\d+) tests? run`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		result := Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests")))
		result.Total = count
		return result, nil
	}
	return Success("All tests passed"), nil
}
