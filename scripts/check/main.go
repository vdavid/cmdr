package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/signal"
	"sort"
	"strings"
	"syscall"
	"time"

	"cmdr/scripts/check/checks"

	"golang.org/x/term"
)

// stringSlice implements flag.Value for accumulating multiple flag values
type stringSlice []string

func (s *stringSlice) String() string {
	return strings.Join(*s, ",")
}

func (s *stringSlice) Set(value string) error {
	for v := range strings.SplitSeq(value, ",") {
		v = strings.TrimSpace(v)
		if v != "" {
			*s = append(*s, v)
		}
	}
	return nil
}

// cliFlags holds the parsed command-line flags.
type cliFlags struct {
	rustOnly        bool
	svelteOnly      bool
	goOnly          bool
	appNames        []string
	checkNames      []string
	ciMode          bool
	verbose         bool
	includeSlow     bool
	onlySlow        bool
	fast            bool
	failFast        bool
	noLog           bool
	fresh           bool // bypass the input-fingerprint cache: run everything selected, then refresh its entries
	onlyFreestyle   bool
	preferFreestyle bool
	freestyleRemote bool   // set on the VM side to filter freestyle-compatible checks
	graph           bool   // render the DependsOn graph (with weights + lanes) and exit
	graphFormat     string // tree (default) | mermaid | dot
}

func main() {
	// SMB orchestrator: lazily set after we know which checks are selected.
	// Captured by the signal handler so Ctrl+C also tears down containers.
	var smb *SmbOrchestrator

	// Kill all child process groups on Ctrl+C / SIGTERM so no orphans are left
	// behind, AND tear down SMB containers we started (so a re-run starts
	// clean and the user doesn't see zombie smb-consumer-* containers in
	// `docker ps`).
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-sigCh
		checks.KillAllProcesses()
		if smb != nil {
			smb.Stop()
		}
		os.Exit(130) // 128 + SIGINT(2)
	}()

	// Validate check configuration at startup to catch nickname collisions early
	if err := checks.ValidateCheckNames(reservedSelectorNames...); err != nil {
		printError("Bad check configuration: %v", err)
		os.Exit(1)
	}

	var cliArgs []string
	if len(os.Args) > 1 {
		cliArgs = os.Args[1:]
	}
	flags, err := parseFlags(cliArgs)
	if errors.Is(err, flag.ErrHelp) {
		showUsage()
		return
	}
	if err != nil {
		printError("%v", err)
		os.Exit(1)
	}

	rootDir, err := findRootDir()
	if err != nil {
		printError("Error: %v", err)
		os.Exit(1)
	}

	if handled := handleFreestyleFlags(rootDir, flags); handled {
		return
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

	// --graph renders the dependency graph of the selected checks (before the
	// slow/fast/CI filters, so every lane is shown with its size badge) and exits.
	if handleGraphFlag(flags, checksToRun) {
		return
	}

	checksToRun = applyLaneFilters(checksToRun, flags)

	// Plan the input-fingerprint cache BEFORE pnpm install and SMB bring-up, so a
	// run whose SMB/node checks are all cache hits never starts containers or runs
	// `pnpm install`. plan.toRun holds the cache misses; plan.cached the hits.
	plan := planSelectedChecks(ctx, flags, checksToRun)
	checksToRun = plan.toRun

	ensurePnpmIfNeeded(ctx, checksToRun)

	smb = setupSmbOrchestratorIfNeeded(rootDir, checksToRun)
	if smb != nil {
		defer smb.Stop()
	}

	runChecks(ctx, checksToRun, plan, flags.failFast, flags.noLog)
}

// planSelectedChecks runs cache planning over the lane-filtered set and exits
// early with "No checks to run." when nothing remains (neither to run nor
// cached). Extracted from main() to keep it under the gocyclo threshold.
func planSelectedChecks(ctx *checks.CheckContext, flags *cliFlags, checksToRun []checks.CheckDefinition) *cachePlan {
	if len(checksToRun) == 0 {
		fmt.Println("No checks to run.")
		os.Exit(0)
	}
	plan := planCache(ctx, flags, checksToRun)
	if len(plan.toRun) == 0 && len(plan.cached) == 0 {
		fmt.Println("No checks to run.")
		os.Exit(0)
	}
	return plan
}

// ensurePnpmIfNeeded installs pnpm deps when any selected check needs them.
func ensurePnpmIfNeeded(ctx *checks.CheckContext, checksToRun []checks.CheckDefinition) {
	if needsPnpmInstall(checksToRun) {
		if err := ensurePnpmDependencies(ctx); err != nil {
			printError("Error: %v", err)
			os.Exit(1)
		}
	}
}

// applyLaneFilters narrows the selected checks by the slow/CI-only/fast/freestyle
// lane flags, in the established order. Extracted from main() to keep it under
// the gocyclo threshold.
func applyLaneFilters(checksToRun []checks.CheckDefinition, flags *cliFlags) []checks.CheckDefinition {
	checksToRun = checks.FilterSlowChecks(checksToRun, flags.includeSlow)
	checksToRun = checks.FilterCIOnlyChecks(checksToRun, flags.ciMode, flags.checkNames)
	checksToRun = checks.FilterFastChecks(checksToRun, flags.fast, flags.checkNames)
	if flags.freestyleRemote {
		checksToRun = checks.FilterFreestyleCompat(checksToRun)
	}
	return filterOnlySlow(checksToRun, flags.onlySlow)
}

// handleGraphFlag renders the dependency graph and reports whether it handled
// the run (so main returns early). Extracted from main to keep it under the
// gocyclo threshold.
func handleGraphFlag(flags *cliFlags, checksToRun []checks.CheckDefinition) bool {
	if !flags.graph {
		return false
	}
	if err := renderGraph(checksToRun, flags.graphFormat, term.IsTerminal(int(os.Stdout.Fd()))); err != nil {
		printError("Error: %v", err)
		os.Exit(1)
	}
	return true
}

// filterOnlySlow keeps only the slow checks when --only-slow is set, else
// returns the input unchanged.
func filterOnlySlow(checksToRun []checks.CheckDefinition, onlySlow bool) []checks.CheckDefinition {
	if !onlySlow {
		return checksToRun
	}
	var slow []checks.CheckDefinition
	for _, c := range checksToRun {
		if c.IsSlow {
			slow = append(slow, c)
		}
	}
	return slow
}

// setupSmbOrchestratorIfNeeded inspects the planned check set for any check
// with a non-empty NeedsSmb. If any are present, it constructs and starts the
// orchestrator. Returns nil when no SMB-using check was scheduled (in which
// case main never needs to defer a stop).
//
// Extracted from main() so main stays under the gocyclo threshold; the
// orchestrator's lifecycle is logically separate from the rest of startup.
func setupSmbOrchestratorIfNeeded(rootDir string, checksToRun []checks.CheckDefinition) *SmbOrchestrator {
	modes := collectModes(checksToRun)
	if len(modes) == 0 {
		return nil
	}
	smb := NewSmbOrchestrator(rootDir)
	if err := smb.EnsureStarted(modes); err != nil {
		printError("Error: %v", err)
		os.Exit(1)
	}
	return smb
}

// handleFreestyleFlags dispatches --only-freestyle / --prefer-freestyle if set.
// Returns true if a freestyle mode was handled (caller should return).
func handleFreestyleFlags(rootDir string, flags *cliFlags) bool {
	if !flags.onlyFreestyle && !flags.preferFreestyle {
		return false
	}

	args := os.Args
	if len(args) > 1 {
		args = args[1:]
	} else {
		args = nil
	}

	if flags.preferFreestyle {
		os.Exit(preferFreestyleRun(rootDir, args, flags))
		return true // unreachable, but keeps the compiler happy
	}

	// --only-freestyle
	if err := freestyleRun(rootDir, args); err != nil {
		printError("Freestyle error: %v", err)
		os.Exit(1)
	}
	return true
}

// reservedSelectorNames are the app and tech-group keywords accepted as
// positional selectors (and by --app / the group flags). ValidateCheckNames
// rejects any check ID/nickname that would shadow one, because positional
// resolution tries check names first.
var reservedSelectorNames = []string{"desktop", "website", "api-server", "scripts", "rust", "svelte", "go"}

// parseFlags parses command-line flags and positional selectors (check
// names, app names, and tech groups, in any order and mix; commas work too).
// Returns flag.ErrHelp when help was requested.
func parseFlags(args []string) (*cliFlags, error) {
	fs := flag.NewFlagSet("pnpm check", flag.ContinueOnError)
	fs.SetOutput(io.Discard) // Errors are returned, not printed; main owns the output
	var (
		rustOnly        = fs.Bool("rust", false, "Run only Rust checks")
		rustOnly2       = fs.Bool("rust-only", false, "Run only Rust checks")
		svelteOnly      = fs.Bool("svelte", false, "Run only Svelte/desktop checks")
		svelteOnly2     = fs.Bool("svelte-only", false, "Run only Svelte/desktop checks")
		goOnly          = fs.Bool("go", false, "Run only Go checks (scripts)")
		goOnly2         = fs.Bool("go-only", false, "Run only Go checks (scripts)")
		appNames        stringSlice
		checkNames      stringSlice
		ciMode          = fs.Bool("ci", false, "Disable auto-fixing (for CI)")
		verbose         = fs.Bool("verbose", false, "Show detailed output")
		includeSlow     = fs.Bool("include-slow", false, "Include slow checks (excluded by default)")
		onlySlow        = fs.Bool("only-slow", false, "Run only slow checks")
		fast            = fs.Bool("fast", false, "Run only the curated fast pre-commit check set")
		failFast        = fs.Bool("fail-fast", false, "Stop on first failure")
		noLog           = fs.Bool("no-log", false, "Disable CSV stats logging")
		fresh           = fs.Bool("fresh", false, "Bypass the input-fingerprint cache: run everything selected, then refresh the cache")
		onlyFreestyle   = fs.Bool("only-freestyle", false, "Run only freestyle-compatible checks on a VM (skip the rest)")
		preferFreestyle = fs.Bool("prefer-freestyle", false, "Run freestyle-compatible checks on VM + the rest locally in parallel")
		freestyleRemote = fs.Bool("freestyle-remote", false, "Filter to freestyle-compatible checks only (used internally on the VM)")
		graph           = fs.Bool("graph", false, "Render the check dependency graph (with CPU weights + size lanes) and exit")
		graphFormat     = fs.String("graph-format", "tree", "Graph output format: tree | mermaid | dot")
		help            = fs.Bool("help", false, "Show help message")
		h               = fs.Bool("h", false, "Show help message")
	)
	fs.Var(&appNames, "app", "Run checks for specific apps (repeatable or comma-separated)")
	fs.Var(&checkNames, "check", "Run specific checks by ID (same as naming them positionally)")

	positionals, err := parseInterspersed(fs, args)
	if err != nil {
		return nil, err
	}
	if *help || *h {
		return nil, flag.ErrHelp
	}
	if *fast && (*includeSlow || *onlySlow) {
		return nil, errors.New("--fast is mutually exclusive with --include-slow / --only-slow")
	}

	flags := &cliFlags{
		rustOnly:        *rustOnly || *rustOnly2,
		svelteOnly:      *svelteOnly || *svelteOnly2,
		goOnly:          *goOnly || *goOnly2,
		appNames:        appNames,
		checkNames:      checkNames,
		ciMode:          *ciMode,
		verbose:         *verbose,
		onlySlow:        *onlySlow,
		fast:            *fast,
		failFast:        *failFast,
		noLog:           *noLog || *ciMode,
		fresh:           *fresh,
		onlyFreestyle:   *onlyFreestyle,
		preferFreestyle: *preferFreestyle,
		freestyleRemote: *freestyleRemote,
		graph:           *graph,
		graphFormat:     *graphFormat,
	}

	if err := applyPositionalSelectors(flags, positionals); err != nil {
		return nil, err
	}

	// Named checks (positional or --check) run even when slow, same escape
	// hatch as before; group/app selectors keep the default lanes.
	flags.includeSlow = *includeSlow || *onlySlow || len(flags.checkNames) > 0

	return flags, nil
}

// applyPositionalSelectors classifies each positional token (splitting on
// commas) into the matching cliFlags fields.
func applyPositionalSelectors(flags *cliFlags, positionals []string) error {
	for _, token := range positionals {
		for part := range strings.SplitSeq(token, ",") {
			part = strings.TrimSpace(part)
			if part == "" {
				continue
			}
			if err := applySelector(flags, part); err != nil {
				return err
			}
		}
	}
	return nil
}

// parseInterspersed parses flags and positional args in any order. Go's
// stdlib flag stops at the first positional arg, so this re-parses the
// remainder until everything is consumed.
func parseInterspersed(fs *flag.FlagSet, args []string) ([]string, error) {
	var positionals []string
	for {
		if err := fs.Parse(args); err != nil {
			return nil, err
		}
		rest := fs.Args()
		if len(rest) == 0 {
			return positionals, nil
		}
		n := 0
		for n < len(rest) && !strings.HasPrefix(rest[n], "-") {
			n++
		}
		if n == 0 {
			// Only reachable after a literal `--`: treat the rest as positional.
			return append(positionals, rest...), nil
		}
		positionals = append(positionals, rest[:n]...)
		if n == len(rest) {
			return positionals, nil
		}
		args = rest[n:]
	}
}

// applySelector classifies one positional selector. Check IDs/nicknames
// behave like --check (named checks run even if slow or CI-only); app names
// and tech-group keywords behave like --app / --rust / --svelte / --go
// (default lanes apply). Keep the keywords here in sync with
// reservedSelectorNames; a test guards the pairing.
func applySelector(flags *cliFlags, name string) error {
	if checks.GetCheckByID(name) != nil {
		flags.checkNames = append(flags.checkNames, name)
		return nil
	}
	switch strings.ToLower(name) {
	case "desktop", "website", "api-server", "scripts":
		flags.appNames = append(flags.appNames, strings.ToLower(name))
	case "rust":
		flags.rustOnly = true
	case "svelte":
		flags.svelteOnly = true
	case "go":
		flags.goOnly = true
	default:
		return fmt.Errorf("unknown check or group: %s\nRun 'pnpm check --help' to see available checks and groups", name)
	}
	return nil
}

// selectChecks determines which checks to run based on flags.
// Selectors are additive: `pnpm check clippy svelte` runs clippy plus all Svelte checks.
func selectChecks(flags *cliFlags) ([]checks.CheckDefinition, error) {
	hasFilter := len(flags.checkNames) > 0 || len(flags.appNames) > 0 || flags.rustOnly || flags.svelteOnly || flags.goOnly
	if !hasFilter {
		return checks.AllChecks, nil
	}

	seen := make(map[string]bool)
	var result []checks.CheckDefinition
	addUnique := func(cs []checks.CheckDefinition) {
		for _, c := range cs {
			if !seen[c.ID] {
				seen[c.ID] = true
				result = append(result, c)
			}
		}
	}

	if len(flags.checkNames) > 0 {
		named, err := selectChecksByID(flags.checkNames)
		if err != nil {
			return nil, err
		}
		addUnique(named)
	}
	for _, appName := range flags.appNames {
		byApp, err := selectChecksByApp(appName)
		if err != nil {
			return nil, err
		}
		addUnique(byApp)
	}
	if flags.rustOnly {
		addUnique(checks.GetChecksByTech(checks.AppDesktop, "🦀 Rust"))
	}
	if flags.svelteOnly {
		addUnique(checks.GetChecksByTech(checks.AppDesktop, "🎨 Svelte"))
	}
	if flags.goOnly {
		addUnique(checks.GetChecksByTech(checks.AppScripts, "🐹 Go"))
	}

	return result, nil
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
	case "api-server":
		return checks.GetChecksByApp(checks.AppApiServer), nil
	case "scripts":
		return checks.GetChecksByApp(checks.AppScripts), nil
	default:
		return nil, fmt.Errorf("unknown app: %s\nAvailable apps: desktop, website, api-server, scripts", appName)
	}
}

// runChecks executes the cache-miss checks, replays cache hits, records this
// run's passes into the cache, and prints the summary.
func runChecks(ctx *checks.CheckContext, checksToRun []checks.CheckDefinition, plan *cachePlan, failFast, noLog bool) {
	total := len(checksToRun) + len(plan.cached)
	if len(plan.cached) > 0 {
		fmt.Printf("🔍 Running %d %s (%d cached)...\n\n", total, checks.Pluralize(total, "check", "checks"), len(plan.cached))
	} else {
		fmt.Printf("🔍 Running %d %s...\n\n", total, checks.Pluralize(total, "check", "checks"))
	}

	startTime := time.Now()
	runner := NewRunner(ctx, checksToRun, plan.cached, failFast, noLog)
	failed, failedChecks := runner.Run()

	// Record this run's passing fingerprints (no-op when caching is disabled).
	plan.recordRun(ctx.RootDir, runner.RunStates())

	totalDuration := time.Since(startTime)
	fmt.Println()
	fmt.Printf("%s⏱️  Total runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)

	if failed {
		printFailure(failedChecks)
		os.Exit(1)
	}

	if runner.CachedCount() > 0 {
		fmt.Printf("%s✅ All checks passed!%s %s(%d ran, %d cached)%s\n",
			colorGreen, colorReset, colorDim, runner.RanCount(), runner.CachedCount(), colorReset)
	} else {
		fmt.Printf("%s✅ All checks passed!%s\n", colorGreen, colorReset)
	}
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
		fmt.Printf("To rerun the failed %s: pnpm check %s\n", checkWord, strings.Join(failedChecks, " "))
	}
}

// needsPnpmInstall returns true if any of the checks require pnpm dependencies.
func needsPnpmInstall(checksToRun []checks.CheckDefinition) bool {
	for _, check := range checksToRun {
		// Checks that need pnpm: Svelte, Astro, TS (api-server)
		switch check.App {
		case checks.AppDesktop, checks.AppWebsite, checks.AppApiServer:
			return true
		}
	}
	return false
}

// ensurePnpmDependencies runs pnpm install before checks.
func ensurePnpmDependencies(ctx *checks.CheckContext) error {
	fmt.Print("📦 Ensuring pnpm dependencies are installed... ")
	startTime := time.Now()

	skipped, err := checks.EnsurePnpmDependencies(ctx)
	if err != nil {
		fmt.Printf("%sFAILED%s\n", colorRed, colorReset)
		return err
	}

	duration := time.Since(startTime)
	if skipped {
		fmt.Printf("%sOK%s (skipped, lockfile unchanged)\n\n", colorGreen, colorReset)
	} else {
		fmt.Printf("%sOK%s (%s)\n\n", colorGreen, colorReset, formatDuration(duration))
	}
	return nil
}

// showUsage displays the help message with dynamically generated check list.
func showUsage() {
	fmt.Println("Usage: pnpm check [OPTIONS] [CHECK|GROUP ...]")
	fmt.Println()
	fmt.Println("Run code quality checks for the Cmdr project.")
	fmt.Println()
	fmt.Println("Name what to run as positional args, in any mix (flags can go anywhere):")
	fmt.Println("    - Check IDs or nicknames (run even if slow/CI-only): oxfmt, clippy, website-build, ...")
	fmt.Println("    - App names: desktop, website, api-server, scripts")
	fmt.Println("    - Tech groups: rust, svelte, go")
	fmt.Println("Comma-separated works too: pnpm check oxfmt,clippy")
	fmt.Println()
	fmt.Println("OPTIONS:")
	fmt.Println("    --app NAME               Run checks for specific apps (repeatable or comma-separated)")
	fmt.Println("    --rust, --rust-only      Run only Rust checks (desktop)")
	fmt.Println("    --svelte, --svelte-only  Run only Svelte checks (desktop)")
	fmt.Println("    --go, --go-only          Run only Go checks (scripts)")
	fmt.Println("    --check ID               Run specific checks by ID (same as naming them positionally)")
	fmt.Println("    --ci                     Disable auto-fixing (for CI)")
	fmt.Println("    --verbose                Show detailed output")
	fmt.Println("    --include-slow           Include slow checks (excluded by default)")
	fmt.Println("    --only-slow              Run only slow checks")
	fmt.Println("    --fast                   Run only the curated fast pre-commit check set")
	fmt.Println("    --only-freestyle         Run freestyle-compatible checks on a VM (skip the rest)")
	fmt.Println("    --prefer-freestyle       Run compat checks on VM + the rest locally in parallel")
	fmt.Println("    --fresh                  Bypass the input-fingerprint cache: run everything selected, then refresh it")
	fmt.Println("    --fail-fast              Stop on first failure")
	fmt.Println("    --no-log                 Disable CSV stats logging (~/cmdr-check-log.csv)")
	fmt.Println("    --graph                  Render the check dependency graph (weights + lanes) and exit")
	fmt.Println("    --graph-format FORMAT    Graph output format: tree (default) | mermaid | dot")
	fmt.Println("    -h, --help               Show this help message")
	fmt.Println()
	fmt.Println("If nothing is named, runs all non-slow checks for all apps.")
	fmt.Println()
	fmt.Println("EXAMPLES:")
	fmt.Println("    pnpm check                       # Run all checks")
	fmt.Println("    pnpm check oxfmt                 # Run one check")
	fmt.Println("    pnpm check clippy rustfmt        # Run several checks")
	fmt.Println("    pnpm check rust                  # Run the Rust group")
	fmt.Println("    pnpm check website --verbose     # Run website checks with detailed output")
	fmt.Println("    pnpm check --include-slow        # Include slow checks")
	fmt.Println("    pnpm check --fast                # Pre-commit lane (fastest)")
	fmt.Println("    pnpm check --ci --fail-fast      # CI mode, stop on first failure")
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
