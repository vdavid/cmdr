package main

import (
	"flag"
	"fmt"
	"os"
	"time"
)

func main() {
	var (
		backendOnly   = flag.Bool("backend", false, "Run only backend checks")
		backendOnly2  = flag.Bool("backend-only", false, "Run only backend checks")
		frontendOnly  = flag.Bool("frontend", false, "Run only frontend checks")
		frontendOnly2 = flag.Bool("frontend-only", false, "Run only frontend checks")
		docsOnly      = flag.Bool("docs", false, "Run only docs checks")
		checkName     = flag.String("check", "", "Run a single check by name")
		ciMode        = flag.Bool("ci", false, "Disable auto-fixing (for CI)")
		verbose       = flag.Bool("verbose", false, "Show detailed output")
		help          = flag.Bool("help", false, "Show help message")
		h             = flag.Bool("h", false, "Show help message")
	)
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

	// If running a single check
	if *checkName != "" {
		startTime := time.Now()
		check := getCheckByName(*checkName)
		if check == nil {
			printError("Error: Unknown check name: %s", *checkName)
			_, err := fmt.Fprintf(os.Stderr, "Run with --help to see available checks\n")
			if err != nil {
				fmt.Println("Error writing to stderr")
				return
			}
			os.Exit(1)
		}
		err := runCheck(check, ctx)
		totalDuration := time.Since(startTime)
		fmt.Println()
		if err != nil {
			fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)
			os.Exit(1)
		}
		fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)
		os.Exit(0)
	}

	// Determine what to run
	runBackend := true
	runFrontend := true
	runDocs := true
	if *backendOnly || *backendOnly2 {
		runFrontend = false
		runDocs = false
	}
	if *frontendOnly || *frontendOnly2 {
		runBackend = false
		runDocs = false
	}
	if *docsOnly {
		runBackend = false
		runFrontend = false
	}

	fmt.Println("🔍 Running all checks...")
	fmt.Println()

	startTime := time.Now()
	var failed bool
	var allFailedChecks []string

	if runBackend {
		backendFailed, failedChecks := runBackendChecks(ctx)
		failed = backendFailed
		allFailedChecks = append(allFailedChecks, failedChecks...)
	}

	if runFrontend {
		frontendFailed, failedChecks := runFrontendChecks(ctx)
		failed = frontendFailed || failed
		allFailedChecks = append(allFailedChecks, failedChecks...)
	}

	if runDocs {
		docsFailed, failedChecks := runDocsChecks(ctx)
		failed = docsFailed || failed
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
				fmt.Printf("  ./scripts/check.sh --check %s\n", checkName)
			}
		}
		os.Exit(1)
	} else {
		fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)
		fmt.Printf("%s✅ All checks passed!%s\n", colorGreen, colorReset)
		os.Exit(0)
	}
}
