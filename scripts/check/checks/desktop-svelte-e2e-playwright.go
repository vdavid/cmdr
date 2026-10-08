package checks

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Parallel sharding: one Tauri instance per shard, plus a sequential MTP shard.
// Each shard owns its own Unix socket, MCP port, data dir, and fixture dir so
// the instances don't clobber each other. The MTP shard runs alone because the
// run's virtual MTP backing dir is shared by every Tauri instance IN THE RUN;
// running MTP tests in two shards at once would corrupt it.
//
// Everything a run owns carries its pid, so two suites (two worktrees checking at
// once, which is the normal case) never touch the same path. `checks/DETAILS.md`
// § "Nothing a shard owns is shared between runs" is the canonical list.
const (
	// Two non-MTP shards plus one MTP shard. Three Tauri instances total.
	// Bumping this further pays diminishing returns: MTP stays single-shard
	// and the non-MTP file durations (file-watching ~78s, accessibility ~66s)
	// already balance well across two shards.
	nonMtpShards = 2
)

type shardSpec struct {
	name string
	kind string // "mtp" or "non-mtp"
	// instanceID is the per-shard CMDR_INSTANCE_ID stamped into the launched binary's env.
	// Drives the macOS Keychain SERVICE_NAME suffix and the productName so Activity Monitor /
	// pgrep can target shards individually. Format: `e2e-<short-name>-<pid>` where
	// <short-name> is `mtp` or `nonmtpN`. See planShards for the mapping.
	instanceID string
	socketPath string
	// mcpPort comes from the OS (reserveMcpPorts), never a fixed base: a second
	// suite starting while this one runs would otherwise want the same port.
	mcpPort    int
	dataDir    string
	fixtureDir string
	logFile    string
	jsonReport string
	// outputDir is where Playwright writes this shard's recordings and error
	// contexts. Run-scoped, so a concurrent suite can't overwrite the evidence of
	// a failure while someone is reading it.
	outputDir string
	// mtpFixtureRoot backs the virtual MTP device. One per RUN, not per shard: the
	// MTP shard wipes and recreates it while the others are told to leave it alone.
	mtpFixtureRoot string
	// For non-mtp shards, Playwright's --shard arg ("1/2", "2/2"). Empty for mtp.
	playwrightShard string
}

type shardResult struct {
	shard   shardSpec
	output  string
	passed  int
	failed  int
	skipped int
	err     error
}

// RunDesktopE2EPlaywright runs Playwright E2E tests against the real Tauri app.
// Self-contained lifecycle: build binary → start N Tauri apps → run N Playwright
// processes in parallel → cleanup.
func RunDesktopE2EPlaywright(ctx *CheckContext) (CheckResult, error) {
	if runtime.GOOS != "darwin" {
		return Skipped("macOS only (use desktop-e2e-linux for Linux)"), nil
	}

	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	timestamp := time.Now().Unix()
	pid := os.Getpid()

	binaryPath, err := EnsureE2EBinary(ctx, timestamp, io.Discard)
	if err != nil {
		return CheckResult{}, err
	}

	mcpPorts, err := reserveMcpPorts(nonMtpShards + 1)
	if err != nil {
		return CheckResult{}, err
	}
	shards := planShards(desktopDir, timestamp, pid, mcpPorts)

	// Sampled HERE, before the apps launch, so it reads the load somebody else has on
	// the machine rather than the load this lane is about to add. See `e2eWaitScale`.
	waitScale := e2eWaitScale(LoadPerCore())

	// Deferred first, so it runs LAST: the apps have to be stopped before their
	// backing dir goes. The data dirs and fixture trees clean themselves up the
	// same way; the reports and logs deliberately survive for post-mortems and are
	// aged out by sweepStaleCheckArtifacts instead.
	defer os.RemoveAll(shards[0].mtpFixtureRoot)

	sweepStaleCheckArtifacts(time.Now())

	cleanupFixtures, err := allocateShardFixtures(desktopDir, shards)
	if err != nil {
		return CheckResult{}, err
	}
	defer cleanupFixtures()

	// ❌ Never clear the way by killing whoever holds the port. Two suites run at
	// once whenever two worktrees are busy, and the ports were fixed until a
	// starting run SIGTERM'd a running one's app mid-test: 38 failures that read
	// like a product bug, one real and 37 cascading off a dead socket. The socket
	// path below is pid-scoped, so removing it can only touch our own leftovers.
	for _, s := range shards {
		os.Remove(s.socketPath)
	}

	apps, cleanupApps, err := startShardApps(binaryPath, shards)
	defer cleanupApps()
	if err != nil {
		return CheckResult{}, err
	}

	for i, s := range shards {
		if err := waitForPlaywrightSocket(s.socketPath, apps[i].exited, s.logFile); err != nil {
			return CheckResult{}, fmt.Errorf("[%s] %w", s.name, err)
		}
	}

	runStart := time.Now()
	results := runShardsInParallel(desktopDir, shards, waitScale)

	// The union of the shards' JSON reports covers the whole suite (MTP shard +
	// the non-MTP shard split).
	reportPaths := make([]string, len(shards))
	for i, s := range shards {
		reportPaths[i] = s.jsonReport
	}
	// Before the verdict, so a red run records WHICH specs went red (`test-log.go`).
	// The check's own error is a shard-level summary; this is the only place the
	// individual spec names reach the log.
	recordPlaywrightTests(ctx, reportPaths, runStart)

	result, err := aggregateShardResults(results, len(shards))
	if err != nil {
		// Never believe a red lane until the failures have been re-run alone, with every
		// shard's app still up and nothing else competing (`e2e-contention.go`). The
		// per-test log supplies each failing spec's own record alongside the verdict, so
		// "has this gone red before?" is answered before it's asked.
		failing := collectE2EFailures(shards, runStart)
		history, historyErr := lookupE2ESpecHistory(failing)
		return resolveE2EFailure(err, failing, playwrightRerunner(desktopDir, pid, waitScale), LoadPerCore, history, historyErr)
	}

	// Warn-only duration flagging.
	result = applyE2EDurationWarnings(ctx, result, reportPaths, "macos")
	// Retry-passes last so the flake line is the final thing read. Local runs are at
	// `retries: 0`, so this is normally a no-op here and does its work on CI.
	return applyE2EFlakyWarning(result, reportPaths), nil
}

// aggregateShardResults sums per-shard test counts, persists each shard's
// output to its log file, and turns any per-shard failures into a single
// summary error.
func aggregateShardResults(results []shardResult, totalShards int) (CheckResult, error) {
	var (
		totalPassed int
		failed      []shardResult
	)
	for _, r := range results {
		totalPassed += r.passed
		appendToLogFile(r.shard.logFile, "\n\n=== Playwright test output ===\n"+r.output)
		if r.err != nil {
			failed = append(failed, r)
		}
	}

	if len(failed) > 0 {
		var msg strings.Builder
		for _, r := range failed {
			summary := extractE2ETestOutput(r.output)
			fmt.Fprintf(&msg, "[%s] failed (full log: %s)\n%s\n", r.shard.name, r.shard.logFile, indentOutput(summary))
		}
		return CheckResult{}, fmt.Errorf("playwright E2E tests failed across %d %s\n%s",
			len(failed), Pluralize(len(failed), "shard", "shards"), msg.String())
	}

	if totalPassed > 0 {
		return Success(fmt.Sprintf("%d %s passed across %d %s",
			totalPassed, Pluralize(totalPassed, "test", "tests"),
			totalShards, Pluralize(totalShards, "shard", "shards"))), nil
	}
	return Success("All Playwright E2E tests passed"), nil
}

// shardInstanceID returns the CMDR_INSTANCE_ID for a shard short-name. Format:
// `e2e-<short>-<pid>`. The wrapper / binary derive the macOS Keychain suffix from this and
// instance-id.ts reshapes it into `Cmdr (E2E <short>)` for the Dock label so cleanup
// scripts can `pgrep -f 'Cmdr (E2E '` cleanly. See P3 in
// docs/specs/instance-isolation-plan.md.
func shardInstanceID(shortName string, pid int) string {
	return fmt.Sprintf("e2e-%s-%d", shortName, pid)
}

// planShards builds the per-shard plan. Shard 0 is the MTP lane; shards
// 1..N are the non-MTP lanes, split by Playwright's --shard X/N.
//
// mcpPorts comes from reserveMcpPorts and must hold one port per shard.
func planShards(_ string, timestamp int64, pid int, mcpPorts []int) []shardSpec {
	shards := make([]shardSpec, 0, nonMtpShards+1)

	// Everything below carries the pid. The timestamp alone doesn't scope a path:
	// two suites can start in the same second, and `os.Create` on a log truncates
	// whatever the other run was writing to.
	mkLog := func(name string) string {
		return fmt.Sprintf("/tmp/cmdr-e2e-playwright-%s-%d-%d.log", name, timestamp, pid)
	}
	// The report is the run's evidence: `e2e-test-log.go` turns it into the per-test
	// log, `e2e-durations.go` flags slow specs from it, and `e2e-flaky.go` counts
	// retry-passes out of it. A fixed path let a concurrent suite answer all three
	// questions about a run it never took part in. Readers take the path from here,
	// and `scripts/e2e-test-timings` picks the newest match.
	mkJSON := func(name string) string {
		return fmt.Sprintf("/tmp/cmdr-e2e-report-%s-%d.json", name, pid)
	}
	// Playwright's recordings and error contexts: the only picture of what a failure
	// looked like, and a concurrent suite overwriting them takes it away.
	mkOutputDir := func(name string) string {
		return fmt.Sprintf("/tmp/cmdr-e2e-results-%s-%d", name, pid)
	}

	mtpRoot := mtpFixtureRootForRun(pid)

	// MTP shard (sequential lane)
	shards = append(shards, shardSpec{
		name:           "mtp",
		kind:           "mtp",
		instanceID:     shardInstanceID("mtp", pid),
		socketPath:     fmt.Sprintf("/tmp/tauri-playwright-mtp-%d.sock", pid),
		mcpPort:        mcpPorts[0],
		dataDir:        fmt.Sprintf("/tmp/cmdr-e2e-data-mtp-%d", pid),
		logFile:        mkLog("mtp"),
		jsonReport:     mkJSON("mtp"),
		outputDir:      mkOutputDir("mtp"),
		mtpFixtureRoot: mtpRoot,
	})

	// Non-MTP shards
	for i := 1; i <= nonMtpShards; i++ {
		shortName := fmt.Sprintf("nonmtp%d", i)
		name := fmt.Sprintf("non-mtp-%d", i)
		shards = append(shards, shardSpec{
			name:            name,
			kind:            "non-mtp",
			instanceID:      shardInstanceID(shortName, pid),
			socketPath:      fmt.Sprintf("/tmp/tauri-playwright-nonmtp%d-%d.sock", i, pid),
			mcpPort:         mcpPorts[i],
			dataDir:         fmt.Sprintf("/tmp/cmdr-e2e-data-nonmtp%d-%d", i, pid),
			logFile:         mkLog(shortName),
			jsonReport:      mkJSON(shortName),
			outputDir:       mkOutputDir(name),
			mtpFixtureRoot:  mtpRoot,
			playwrightShard: fmt.Sprintf("%d/%d", i, nonMtpShards),
		})
	}
	return shards
}

// runShardsInParallel launches one Playwright process per shard and waits for
// all to finish.
func runShardsInParallel(desktopDir string, shards []shardSpec, waitScale float64) []shardResult {
	results := make([]shardResult, len(shards))
	var wg sync.WaitGroup
	for i, s := range shards {
		wg.Add(1)
		go func(idx int, shard shardSpec) {
			defer wg.Done()
			results[idx] = runShard(desktopDir, shard, waitScale)
		}(i, s)
	}
	wg.Wait()
	return results
}

// runShard executes one Playwright process for a single shard.
func runShard(desktopDir string, s shardSpec, waitScale float64) shardResult {
	args := []string{
		"exec", "playwright", "test",
		"--config", "test/e2e-playwright/playwright.config.ts",
		"--project", "tauri",
	}
	if s.playwrightShard != "" {
		args = append(args, "--shard", s.playwrightShard)
	}
	// Niced, like the app it drives: the lane yields the machine to whoever is using it
	// rather than competing for it. See `e2eNiceIncrement`.
	cmd := niceCommand("pnpm", args...)
	cmd.Dir = desktopDir
	cmd.Env = shardPlaywrightEnv(s, s.jsonReport, s.outputDir, waitScale)
	output, err := RunCommand(cmd, true)
	passed, failed, skipped := parsePlaywrightTotals(output)
	return shardResult{
		shard:   s,
		output:  output,
		passed:  passed,
		failed:  failed,
		skipped: skipped,
		err:     err,
	}
}

// shardManagedPrefsFile is where a shard's managed-policy override plist goes, in its own data
// dir, so a spec pushing a policy reaches only its own shard's app. The app and the Playwright
// process both get it as `CMDR_MANAGED_PREFS_FILE`.
func shardManagedPrefsFile(s shardSpec) string {
	return filepath.Join(s.dataDir, "managed-prefs.plist")
}

// shardPlaywrightEnv builds the environment one Playwright process runs under.
//
// The report and output dir are parameters rather than fields off the shard so the
// isolation re-run (`e2e-contention.go`) can reach the same app through the same env
// while writing its evidence somewhere else. Everything else has to be identical, or a
// re-run would answer a different question than the run it is judging: an MTP spec needs
// its shard's `CMDR_MTP_FIXTURE_ROOT` and virtual device, a non-MTP spec needs the shard
// that was told to leave that root alone.
func shardPlaywrightEnv(s shardSpec, jsonReport, outputDir string, waitScale float64) []string {
	env := append(os.Environ(),
		"CMDR_E2E_START_PATH="+s.fixtureDir,
		// How far the suite stretches every wait for this run, from the load somebody
		// else had on the machine when the lane started (`e2eWaitScale`). A busy box
		// makes the suite SLOWER rather than redder; `wait-budget.ts` is the consumer,
		// and an unset var there means 1, so a hand-run `npx playwright test` and the
		// Linux Docker lane are unaffected.
		"CMDR_E2E_WAIT_SCALE="+strconv.FormatFloat(waitScale, 'f', -1, 64),
		// Ask Cmdr has no real AI provider under E2E; this flag routes its send path
		// through the deterministic scripted fake LLM (see commands/agent.rs), so
		// ask-cmdr.spec.ts can assert streamed text. Safe: no other spec sends AI messages.
		"CMDR_E2E_ASK_CMDR_FAKE=1",
		// Specs that assert on persisted state (for example
		// viewer-wordwrap-persistence.spec.ts) read the instance's
		// settings.json directly, so the test process needs the same
		// per-shard data dir the app launch below gets. Without it, the
		// spec's module-scope guard throws during collection and kills the
		// whole shard before any test runs.
		"CMDR_DATA_DIR="+s.dataDir,
		"CMDR_MCP_PORT="+strconv.Itoa(s.mcpPort),
		"CMDR_PLAYWRIGHT_SOCKET="+s.socketPath,
		"CMDR_E2E_SHARD_KIND="+s.kind,
		"CMDR_E2E_JSON_REPORT="+jsonReport,
		"CMDR_E2E_OUTPUT_DIR="+outputDir,
		// The MTP specs assert against the backing dir directly (mtp-fixtures.ts),
		// so the Playwright process has to agree with the app about where it is.
		"CMDR_MTP_FIXTURE_ROOT="+s.mtpFixtureRoot,
		// The plist that stands in for the organization's managed preferences: absent (no
		// policy) until `managed-policy.spec.ts` writes one, and the app re-reads it on change.
		"CMDR_MANAGED_PREFS_FILE="+shardManagedPrefsFile(s),
	)
	// Only the MTP shard is allowed to wipe this run's virtual MTP backing
	// directory in globalSetup. The non-MTP shards must skip it to avoid
	// stomping on the MTP shard's mid-run state.
	if s.kind != "mtp" {
		env = append(env, "CMDR_E2E_SKIP_MTP_FIXTURES=1")
	}
	return env
}

// lookupE2ESpecHistory fetches each failing spec's record from the per-test log. It is
// instrumentation, so it never colours the lane's verdict: a missing or slow log comes
// back as an error the report mentions in passing and nothing else.
func lookupE2ESpecHistory(files []e2eFailingFile) (map[string]TestHistory, error) {
	var ids []string
	for _, f := range files {
		ids = append(ids, f.keys...)
	}
	if len(ids) == 0 {
		return nil, nil
	}
	path, err := TestLogPath()
	if err != nil {
		return nil, err
	}
	return LookupTestHistory(path, ids, testLogHistoryBudget)
}
