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

// cliFlags holds the parsed command-line flags.
type cliFlags struct {
	rustOnly    bool
	svelteOnly  bool
	goOnly      bool
	appName     string
	checkNames  []string
	ciMode      bool
	verbose     bool
	includeSlow bool
	failFast    bool
}

func main() {
	// Validate check configuration at startup to catch nickname collisions early
	if err := checks.ValidateCheckNames(); err != nil {
		printError("Bad check configuration: %v", err)
		os.Exit(1)
	}

	flags := parseFlags()
	if flags == nil {
		return // Help was shown
	}

	rootDir, err := findRootDir()
	if err != nil {
		printError("Error: %v", err)
		os.Exit(1)
	}

	ctx := &checks.CheckContext{
		CI:      flags.ciMode,
		Verbose: flags.verbose,
		RootDir: rootDir,
	}

	checksToRun, err := selectChecks(flags)
	if err != nil {
		printError("Error: %v", err)
		os.Exit(1)
	}

	checksToRun = checks.FilterSlowChecks(checksToRun, flags.includeSlow)

	if len(checksToRun) == 0 {
		fmt.Println("No checks to run.")
		os.Exit(0)
	}

	runChecks(ctx, checksToRun, flags.failFast)
}

// parseFlags parses command-line flags and returns nil if help was shown.
func parseFlags() *cliFlags {
	var (
		rustOnly    = flag.Bool("rust", false, "Run only Rust checks")
		rustOnly2   = flag.Bool("rust-only", false, "Run only Rust checks")
		svelteOnly  = flag.Bool("svelte", false, "Run only Svelte/desktop checks")
		svelteOnly2 = flag.Bool("svelte-only", false, "Run only Svelte/desktop checks")
		goOnly      = flag.Bool("go", false, "Run only Go checks (scripts)")
		goOnly2     = flag.Bool("go-only", false, "Run only Go checks (scripts)")
		appName     = flag.String("app", "", "Run checks for a specific app (desktop, website, license-server, scripts)")
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
		return nil
	}

	return &cliFlags{
		rustOnly:    *rustOnly || *rustOnly2,
		svelteOnly:  *svelteOnly || *svelteOnly2,
		goOnly:      *goOnly || *goOnly2,
		appName:     *appName,
		checkNames:  checkNames,
		ciMode:      *ciMode,
		verbose:     *verbose,
		includeSlow: *includeSlow || len(checkNames) > 0,
		failFast:    *failFast,
	}
}

// selectChecks determines which checks to run based on flags.
func selectChecks(flags *cliFlags) ([]checks.CheckDefinition, error) {
	if len(flags.checkNames) > 0 {
		return selectChecksByID(flags.checkNames)
	}
	if flags.appName != "" {
		return selectChecksByApp(flags.appName)
	}
	if flags.rustOnly {
		return checks.GetChecksByTech(checks.AppDesktop, "🦀 Rust"), nil
	}
	if flags.svelteOnly {
		return checks.GetChecksByTech(checks.AppDesktop, "🎨 Svelte"), nil
	}
	if flags.goOnly {
		return checks.GetChecksByTech(checks.AppScripts, "🐹 Go"), nil
	}
	return checks.AllChecks, nil
}

// selectChecksByID returns checks matching the given IDs.
func selectChecksByID(names []string) ([]checks.CheckDefinition, error) {
	var result []checks.CheckDefinition
	for _, name := range names {
		check := checks.GetCheckByID(name)
		if check == nil {
			return nil, fmt.Errorf("unknown check ID: %s\nRun with --help to see available checks", name)
		}
		result = append(result, *check)
	}
	return result, nil
}

// selectChecksByApp returns checks for the given app name.
func selectChecksByApp(appName string) ([]checks.CheckDefinition, error) {
	switch strings.ToLower(appName) {
	case "desktop":
		return checks.GetChecksByApp(checks.AppDesktop), nil
	case "website":
		return checks.GetChecksByApp(checks.AppWebsite), nil
	case "license-server":
		return checks.GetChecksByApp(checks.AppLicenseServer), nil
	case "scripts":
		return checks.GetChecksByApp(checks.AppScripts), nil
	default:
		return nil, fmt.Errorf("unknown app: %s\nAvailable apps: desktop, website, license-server, scripts", appName)
	}
}

// runChecks executes the checks and prints results.
func runChecks(ctx *checks.CheckContext, checksToRun []checks.CheckDefinition, failFast bool) {
	fmt.Printf("🔍 Running %d checks...\n\n", len(checksToRun))

	startTime := time.Now()
	runner := NewRunner(ctx, checksToRun, failFast)
	failed, failedChecks := runner.Run()

	totalDuration := time.Since(startTime)
	fmt.Println()
	fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)

	if failed {
		printFailure(failedChecks)
		os.Exit(1)
	}

	fmt.Printf("%s✅ All checks passed!%s\n", colorGreen, colorReset)
}

// printFailure prints the failure message with rerun instructions.
func printFailure(failedChecks []string) {
	fmt.Printf("%s❌ Some checks failed.%s\n", colorRed, colorReset)
	if len(failedChecks) > 0 {
		fmt.Println()
		checkWord := "check"
		if len(failedChecks) > 1 {
			checkWord = "checks"
		}
		fmt.Printf("To rerun the failed %s: ./scripts/check.sh --check %s\n", checkWord, strings.Join(failedChecks, ","))
	}
}

// showUsage displays the help message with dynamically generated check list.
func showUsage() {
	fmt.Println("Usage: go run ./scripts/check [OPTIONS]")
	fmt.Println()
	fmt.Println("Run code quality checks for the Cmdr project.")
	fmt.Println()
	fmt.Println("OPTIONS:")
	fmt.Println("    --app NAME               Run checks for a specific app (desktop, website, license-server, scripts)")
	fmt.Println("    --rust, --rust-only      Run only Rust checks (desktop)")
	fmt.Println("    --svelte, --svelte-only  Run only Svelte checks (desktop)")
	fmt.Println("    --go, --go-only          Run only Go checks (scripts)")
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
		name := check.CLIName()
		if check.IsSlow {
			name += " (slow)"
		}
		groupMap[key].ids = append(groupMap[key].ids, name)
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
		g, ok := groupMap[key]
		if !ok {
			continue
		}
		fmt.Printf("  %s: %s\n", checks.AppDisplayName(g.app), g.tech)
		for _, id := range g.ids {
			fmt.Printf("    - %s\n", id)
		}
	}
}
