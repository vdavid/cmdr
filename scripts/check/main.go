package main

import (
	"flag"
	"fmt"
	"os"
	"strings"
	"time"
)

// stringSlice implements flag.Value for accumulating multiple flag values
type stringSlice []string

func (s *stringSlice) String() string {
	return strings.Join(*s, ",")
}

func (s *stringSlice) Set(value string) error {
	// Support comma-separated values
	for _, v := range strings.Split(value, ",") {
		v = strings.TrimSpace(v)
		if v != "" {
			*s = append(*s, v)
		}
	}
	return nil
}

func main() {
	var (
		rustOnly    = flag.Bool("rust", false, "Run only Rust checks")
		rustOnly2   = flag.Bool("rust-only", false, "Run only Rust checks")
		svelteOnly  = flag.Bool("svelte", false, "Run only Svelte/desktop checks")
		svelteOnly2 = flag.Bool("svelte-only", false, "Run only Svelte/desktop checks")
		appName     = flag.String("app", "", "Run checks for a specific app (desktop, website, license-server)")
		checkNames  stringSlice
		ciMode      = flag.Bool("ci", false, "Disable auto-fixing (for CI)")
		verbose     = flag.Bool("verbose", false, "Show detailed output")
		help        = flag.Bool("help", false, "Show help message")
		h           = flag.Bool("h", false, "Show help message")
	)
	flag.Var(&checkNames, "check", "Run specific checks by name (can be repeated or comma-separated)")
	flag.Parse()

	if *help || *h {
		showUsage()
		os.Exit(0)
	}

	rootDir, err := findRootDir()
	if err != nil {
		printError("Error: %v", err)
		os.Exit(1)
	}

	ctx := &CheckContext{
		CI:      *ciMode,
		Verbose: *verbose,
		RootDir: rootDir,
	}

	// If running specific checks
	if len(checkNames) > 0 {
		startTime := time.Now()
		var failed bool
		var failedChecks []string

		for _, name := range checkNames {
			check := getCheckByName(name)
			if check == nil {
				printError("Error: Unknown check name: %s", name)
				_, err := fmt.Fprintf(os.Stderr, "Run with --help to see available checks\n")
				if err != nil {
					fmt.Println("Error writing to stderr")
					return
				}
				os.Exit(1)
			}
			err := runCheck(check, ctx)
			if err != nil {
				failed = true
				failedChecks = append(failedChecks, name)
			}
		}

		totalDuration := time.Since(startTime)
		fmt.Println()
		fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)
		if failed {
			fmt.Printf("%s❌ Some checks failed.%s\n", colorRed, colorReset)
			if len(failedChecks) > 0 {
				fmt.Println()
				fmt.Println("Failed checks:")
				for _, name := range failedChecks {
					fmt.Printf("  - %s\n", name)
				}
			}
			os.Exit(1)
		}
		os.Exit(0)
	}

	// Determine what to run based on flags
	runRust := true
	runSvelte := true
	runWebsite := true
	runLicenseServer := true

	// --app flag takes precedence
	if *appName != "" {
		app := strings.ToLower(*appName)
		runRust = false
		runSvelte = false
		runWebsite = false
		runLicenseServer = false

		switch app {
		case "desktop":
			runRust = true
			runSvelte = true
		case "website":
			runWebsite = true
		case "license-server":
			runLicenseServer = true
		default:
			printError("Error: Unknown app: %s", *appName)
			fmt.Fprintf(os.Stderr, "Available apps: desktop, website, license-server\n")
			os.Exit(1)
		}
	} else if *rustOnly || *rustOnly2 {
		runSvelte = false
		runWebsite = false
		runLicenseServer = false
	} else if *svelteOnly || *svelteOnly2 {
		runRust = false
		runWebsite = false
		runLicenseServer = false
	}

	fmt.Println("🔍 Running all checks...")
	fmt.Println()

	startTime := time.Now()
	var failed bool
	var allFailedChecks []string

	if runRust {
		rustFailed, failedChecks := runRustChecks(ctx)
		failed = rustFailed
		allFailedChecks = append(allFailedChecks, failedChecks...)
	}

	if runSvelte {
		svelteFailed, failedChecks := runSvelteChecks(ctx)
		failed = svelteFailed || failed
		allFailedChecks = append(allFailedChecks, failedChecks...)
	}

	if runWebsite {
		websiteFailed, failedChecks := runWebsiteChecks(ctx)
		failed = websiteFailed || failed
		allFailedChecks = append(allFailedChecks, failedChecks...)
	}

	if runLicenseServer {
		serverFailed, failedChecks := runLicenseServerChecks(ctx)
		failed = serverFailed || failed
		allFailedChecks = append(allFailedChecks, failedChecks...)
	}

	totalDuration := time.Since(startTime)
	fmt.Println()
	if failed {
		fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)
		fmt.Printf("%s❌ Some checks failed. Please fix the issues above.%s\n", colorRed, colorReset)
		if len(allFailedChecks) > 0 {
			fmt.Println()
			fmt.Println("To rerun a specific check:")
			for _, checkName := range allFailedChecks {
				fmt.Printf("  go run ./scripts/check --check %s\n", checkName)
			}
		}
		os.Exit(1)
	} else {
		fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)
		fmt.Printf("%s✅ All checks passed!%s\n", colorGreen, colorReset)
		os.Exit(0)
	}
}
