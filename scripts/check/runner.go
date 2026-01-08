package main

import (
	"fmt"
	"time"
)

// showUsage displays the help message.
func showUsage() {
	fmt.Println("Usage: go run ./scripts/check [OPTIONS]")
	fmt.Println()
	fmt.Println("Run code quality checks for the Cmdr project.")
	fmt.Println()
	fmt.Println("OPTIONS:")
	fmt.Println("    --rust, --rust-only      Run only Rust checks")
	fmt.Println("    --svelte, --svelte-only  Run only Svelte checks")
	fmt.Println("    --check NAME             Run a single check by name")
	fmt.Println("    --ci                     Disable auto-fixing (for CI)")
	fmt.Println("    --verbose                Show detailed output")
	fmt.Println("    -h, --help               Show this help message")
	fmt.Println()
	fmt.Println("If no options are provided, runs all checks (Rust and Svelte).")
	fmt.Println()
	fmt.Println("Available check names:")
	fmt.Println("  Rust: rustfmt, clippy, cargo-audit, cargo-deny, rust-tests")
	fmt.Println("  Svelte: prettier, eslint, svelte-tests, e2e-tests")
	fmt.Println()
	fmt.Println("Each check displays its execution time in the format: OK (123ms) or FAILED (1.23s)")
}

// runCheck runs a single check and displays the result.
func runCheck(check Check, ctx *CheckContext) error {
	fmt.Printf("  • %s... ", check.Name())
	start := time.Now()
	err := check.Run(ctx)
	duration := time.Since(start)

	if err != nil {
		fmt.Printf("%sFAILED%s (%s)\n", colorRed, colorReset, formatDuration(duration))
		// Always show error details on failure
		fmt.Printf("      Error: %v\n", err)
		return err
	}
	fmt.Printf("%sOK%s (%s)\n", colorGreen, colorReset, formatDuration(duration))
	return nil
}

// runRustChecks runs all Rust checks.
func runRustChecks(ctx *CheckContext) (bool, []string) {
	fmt.Println("🦀 Rust checks...")
	checks := getRustChecks()
	var failed bool
	var failedChecks []string
	for _, check := range checks {
		fmt.Printf("  • %s... ", check.Name())
		start := time.Now()
		err := check.Run(ctx)
		duration := time.Since(start)

		if err != nil {
			fmt.Printf("%sFAILED%s (%s)\n", colorRed, colorReset, formatDuration(duration))
			fmt.Printf("      Error: %v\n", err)
			failed = true
			failedChecks = append(failedChecks, getCheckCLIName(check))
		} else {
			fmt.Printf("%sOK%s (%s)\n", colorGreen, colorReset, formatDuration(duration))
		}
	}
	return failed, failedChecks
}

// runSvelteChecks runs all Svelte checks.
func runSvelteChecks(ctx *CheckContext) (bool, []string) {
	fmt.Println()
	fmt.Println("🎨 Svelte checks...")
	checks := getSvelteChecks()
	var failed bool
	var failedChecks []string
	for _, check := range checks {
		fmt.Printf("  • %s... ", check.Name())
		start := time.Now()
		err := check.Run(ctx)
		duration := time.Since(start)

		if err != nil {
			fmt.Printf("%sFAILED%s (%s)\n", colorRed, colorReset, formatDuration(duration))
			fmt.Printf("      Error: %v\n", err)
			failed = true
			failedChecks = append(failedChecks, getCheckCLIName(check))
		} else {
			fmt.Printf("%sOK%s (%s)\n", colorGreen, colorReset, formatDuration(duration))
		}
	}
	return failed, failedChecks
}

// formatDuration formats a duration in a human-readable way.
func formatDuration(d time.Duration) string {
	if d < time.Second {
		return fmt.Sprintf("%dms", d.Milliseconds())
	}
	if d < time.Minute {
		return fmt.Sprintf("%.2fs", d.Seconds())
	}
	minutes := int(d.Minutes())
	seconds := int(d.Seconds()) % 60
	return fmt.Sprintf("%dm%ds", minutes, seconds)
}
