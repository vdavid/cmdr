package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunClippy runs Clippy linter with auto-fix.
func RunClippy(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	rustDir := filepath.Join(desktopDir, "src-tauri")

	// Ensure llama-server binaries exist (downloads on macOS, creates placeholder on Linux)
	downloadCmd := exec.Command("go", "run", "scripts/download-llama-server.go")
	downloadCmd.Dir = desktopDir
	if output, err := RunCommand(downloadCmd, true); err != nil {
		return CheckResult{}, fmt.Errorf("failed to prepare llama-server binaries\n%s", indentOutput(output))
	}

	// No source touch here: clippy runs incrementally. With `-D warnings`, a
	// warning becomes a compile error, so a warning-laden build FAILS and cargo
	// does NOT cache it — it's re-surfaced on every run until fixed (verified:
	// warm re-runs of a warning all caught it). Touching lib.rs to force a
	// re-lint of unchanged-clean code only wasted ~22s rebuilding `cmdr_lib`
	// here AND, because the touch bumped a shared source mtime, forced the same
	// rebuild in rust-tests / bindings-fresh / integration (they share
	// `target/`). See docs/notes/check-cpu-contention.md.

	// Run the enforcing check first. On the happy path (no warnings) this is
	// the only build pass we do. --fix is reserved for the failure branch
	// because it ignores -D warnings, can rewrite source files, and re-running
	// it speculatively doubled wall time on every clean run.
	cmd := exec.Command("cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("clippy errors found, run the check script locally\n%s", indentOutput(output))
		}

		// Locally: try to auto-fix, then re-check.
		fixCmd := exec.Command("cargo", "clippy", "--locked", "--all-targets", "--fix", "--allow-dirty", "--allow-staged")
		fixCmd.Dir = rustDir
		_, _ = RunCommand(fixCmd, true)

		// Force a re-lint for the re-check below: `--fix` runs without `-D`, so it
		// succeeds even with unfixable warnings and caches a clean-with-warnings
		// build; without this touch the `-D` re-check could reuse it and miss
		// them. Only reached locally on an already-failing clippy, so it never
		// touches the warm-path cache the other Rust checks share.
		libPath := filepath.Join(rustDir, "src", "lib.rs")
		_ = exec.Command("touch", libPath).Run()
		cmd = exec.Command("cargo", "clippy", "--all-targets", "--", "-D", "warnings")
		cmd.Dir = rustDir
		output, err = RunCommand(cmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("clippy found unfixable issues\n%s", indentOutput(output))
		}
	}

	// Try to extract "Compiling X crates" from output
	re := regexp.MustCompile(`Compiling (\d+) crates?`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		result := Success(fmt.Sprintf("Checked %d %s, no warnings", count, Pluralize(count, "crate", "crates")))
		result.Total = count
		return result, nil
	}

	// Fallback: count "Checking" lines
	re2 := regexp.MustCompile(`(?m)^\s*Checking`)
	checkingMatches := re2.FindAllString(output, -1)
	if len(checkingMatches) > 0 {
		count := len(checkingMatches)
		result := Success(fmt.Sprintf("Checked %d %s, no warnings", count, Pluralize(count, "crate", "crates")))
		result.Total = count
		return result, nil
	}

	return Success("No warnings"), nil
}
