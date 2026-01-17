package main

import (
	"flag"
	"fmt"
	"os"
	"sort"
	"strings"
	"time"

	"vmail/scripts/check/checks"
)

// stringSlice implements flag.Value for accumulating multiple flag values
type stringSlice []string

func (s *stringSlice) String() string {
	return strings.Join(*s, ",")
}

func (s *stringSlice) Set(value string) error {
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
		includeSlow = flag.Bool("include-slow", false, "Include slow checks (excluded by default)")
		failFast    = flag.Bool("fail-fast", false, "Stop on first failure")
		help        = flag.Bool("help", false, "Show help message")
		h           = flag.Bool("h", false, "Show help message")
	)
	flag.Var(&checkNames, "check", "Run specific checks by ID (can be repeated or comma-separated)")
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

	ctx := &checks.CheckContext{
		CI:      *ciMode,
		Verbose: *verbose,
		RootDir: rootDir,
	}

	// Determine which checks to run
	var checksToRun []checks.CheckDefinition

	if len(checkNames) > 0 {
		// Run specific checks by ID
		for _, name := range checkNames {
			check := getCheckByID(name)
			if check == nil {
				printError("Error: Unknown check ID: %s", name)
				fmt.Fprintf(os.Stderr, "Run with --help to see available checks\n")
				os.Exit(1)
			}
			checksToRun = append(checksToRun, *check)
		}
		// When running specific checks, include slow ones
		*includeSlow = true
	} else if *appName != "" {
		app := strings.ToLower(*appName)
		switch app {
		case "desktop":
			checksToRun = getChecksByApp(checks.AppDesktop)
		case "website":
			checksToRun = getChecksByApp(checks.AppWebsite)
		case "license-server":
			checksToRun = getChecksByApp(checks.AppLicenseServer)
		default:
			printError("Error: Unknown app: %s", *appName)
			fmt.Fprintf(os.Stderr, "Available apps: desktop, website, license-server\n")
			os.Exit(1)
		}
	} else if *rustOnly || *rustOnly2 {
		checksToRun = getChecksByTech(checks.AppDesktop, "🦀 Rust")
	} else if *svelteOnly || *svelteOnly2 {
		checksToRun = getChecksByTech(checks.AppDesktop, "🎨 Svelte")
	} else {
		checksToRun = getAllChecks()
	}

	// Filter slow checks unless included
	checksToRun = filterSlowChecks(checksToRun, *includeSlow)

	if len(checksToRun) == 0 {
		fmt.Println("No checks to run.")
		os.Exit(0)
	}

	fmt.Printf("🔍 Running %d checks...\n\n", len(checksToRun))

	startTime := time.Now()
	runner := NewRunner(ctx, checksToRun, *failFast)
	failed, failedChecks := runner.Run()

	totalDuration := time.Since(startTime)
	fmt.Println()
	fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)

	if failed {
		fmt.Printf("%s❌ Some checks failed.%s\n", colorRed, colorReset)
		if len(failedChecks) > 0 {
			fmt.Println()
			checkWord := "check"
			if len(failedChecks) > 1 {
				checkWord = "checks"
			}
			fmt.Printf("To rerun the failed %s: ./scripts/check.sh --check %s\n", checkWord, strings.Join(failedChecks, ","))
		}
		os.Exit(1)
	}

	fmt.Printf("%s✅ All checks passed!%s\n", colorGreen, colorReset)
	os.Exit(0)
}

// showUsage displays the help message with dynamically generated check list.
func showUsage() {
	fmt.Println("Usage: go run ./scripts/check [OPTIONS]")
	fmt.Println()
	fmt.Println("Run code quality checks for the Cmdr project.")
	fmt.Println()
	fmt.Println("OPTIONS:")
	fmt.Println("    --app NAME               Run checks for a specific app (desktop, website, license-server)")
	fmt.Println("    --rust, --rust-only      Run only Rust checks (desktop)")
	fmt.Println("    --svelte, --svelte-only  Run only Svelte checks (desktop)")
	fmt.Println("    --check ID               Run specific checks by ID (can be repeated or comma-separated)")
	fmt.Println("    --ci                     Disable auto-fixing (for CI)")
	fmt.Println("    --verbose                Show detailed output")
	fmt.Println("    --include-slow           Include slow checks (excluded by default)")
	fmt.Println("    --fail-fast              Stop on first failure")
	fmt.Println("    -h, --help               Show this help message")
	fmt.Println()
	fmt.Println("If no options are provided, runs all non-slow checks for all apps.")
	fmt.Println()
	fmt.Println("EXAMPLES:")
	fmt.Println("    go run ./scripts/check                              # Run all checks")
	fmt.Println("    go run ./scripts/check --app desktop                # Run only desktop app checks")
	fmt.Println("    go run ./scripts/check --check desktop-rust-clippy  # Run specific check")
	fmt.Println("    go run ./scripts/check --include-slow               # Include slow checks")
	fmt.Println("    go run ./scripts/check --ci --fail-fast             # CI mode, stop on first failure")
	fmt.Println()
	fmt.Println("Available checks:")
	fmt.Println()

	// Group checks by app and tech
	type checkGroup struct {
		app  checks.App
		tech string
		ids  []string
	}

	groupMap := make(map[string]*checkGroup)
	var groupOrder []string

	for _, check := range checks.AllChecks {
		key := string(check.App) + "|" + check.Tech
		if _, ok := groupMap[key]; !ok {
			groupMap[key] = &checkGroup{
				app:  check.App,
				tech: check.Tech,
			}
			groupOrder = append(groupOrder, key)
		}
		id := check.ID
		if check.IsSlow {
			id += " (slow)"
		}
		groupMap[key].ids = append(groupMap[key].ids, id)
	}

	// Sort groups
	sort.Slice(groupOrder, func(i, j int) bool {
		gi, gj := groupMap[groupOrder[i]], groupMap[groupOrder[j]]
		if gi.app != gj.app {
			return gi.app < gj.app
		}
		return gi.tech < gj.tech
	})

	for _, key := range groupOrder {
		g := groupMap[key]
		fmt.Printf("  %s: %s\n", appDisplayName(g.app), g.tech)
		for _, id := range g.ids {
			fmt.Printf("    - %s\n", id)
		}
	}
}
