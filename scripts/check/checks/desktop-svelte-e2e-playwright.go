package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

// Parallel sharding: one Tauri instance per shard, plus a sequential MTP shard.
// Each shard owns its own Unix socket, MCP port, data dir, and fixture dir so
// the instances don't clobber each other. The MTP shard runs alone because
// the virtual MTP backing dir (/tmp/cmdr-mtp-e2e-fixtures) is shared by every
// Tauri instance; running MTP tests in two shards at once would corrupt it.
const (
	socketTimeout    = 60 * time.Second
	processKillGrace = 3 * time.Second

	// Two non-MTP shards plus one MTP shard. Three Tauri instances total.
	// Bumping this further pays diminishing returns: MTP stays single-shard
	// and the non-MTP file durations (file-watching ~78s, accessibility ~66s)
	// already balance well across two shards.
	nonMtpShards = 2

	mcpPortBase = 9429
)

type shardSpec struct {
	name       string
	kind       string // "mtp" or "non-mtp"
	socketPath string
	mcpPort    int
	dataDir    string
	fixtureDir string
	logFile    string
	jsonReport string
	// For non-mtp shards, Playwright's --shard arg ("1/2", "2/2"). Empty for mtp.
	playwrightShard string
}

type appHandle struct {
	cmd    *exec.Cmd
	exited <-chan struct{}
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

	binaryPath, err := buildTauriBinary(ctx, desktopDir, timestamp)
	if err != nil {
		return CheckResult{}, err
	}

	shards := planShards(desktopDir, timestamp, pid)

	cleanupFixtures, err := allocateShardFixtures(desktopDir, shards)
	if err != nil {
		return CheckResult{}, err
	}
	defer cleanupFixtures()

	for _, s := range shards {
		os.Remove(s.socketPath)
		stopProcessOnPort(strconv.Itoa(s.mcpPort))
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

	results := runShardsInParallel(desktopDir, shards)
	return aggregateShardResults(results, len(shards))
}

// buildTauriBinary compiles the Tauri binary with the playwright-e2e feature
// flag, returns the path to the built binary, and code-signs it for Keychain
// access on macOS. Errors include the build log path for post-mortem.
func buildTauriBinary(ctx *CheckContext, desktopDir string, timestamp int64) (string, error) {
	buildCmd := exec.Command("pnpm", "test:e2e:playwright:build")
	buildCmd.Dir = desktopDir
	buildOutput, err := RunCommand(buildCmd, true)
	if err != nil {
		buildLog := fmt.Sprintf("/tmp/cmdr-e2e-playwright-build-%d.log", timestamp)
		appendToLogFile(buildLog, buildOutput)
		return "", fmt.Errorf("tauri build failed (log: %s)\n%s", buildLog, indentOutput(buildOutput))
	}

	binaryPath, err := findTauriBinary(ctx.RootDir)
	if err != nil {
		return "", err
	}
	if err := codesignDevBinary(binaryPath); err != nil {
		return "", err
	}
	return binaryPath, nil
}

// allocateShardFixtures creates one fixture directory per shard and returns a
// cleanup function that removes them all. On error, any fixtures created so
// far are removed before returning.
func allocateShardFixtures(desktopDir string, shards []shardSpec) (func(), error) {
	for i := range shards {
		fixtureDir, err := createE2EFixtures(desktopDir)
		if err != nil {
			for j := range i {
				os.RemoveAll(shards[j].fixtureDir)
			}
			return func() {}, err
		}
		shards[i].fixtureDir = fixtureDir
	}
	cleanup := func() {
		for _, s := range shards {
			os.RemoveAll(s.fixtureDir)
		}
	}
	return cleanup, nil
}

// startShardApps launches one Tauri instance per shard. Returns the handles, a
// cleanup function that gracefully stops every app that managed to start, and
// any start error. The cleanup function is always safe to call.
func startShardApps(binaryPath string, shards []shardSpec) ([]*appHandle, func(), error) {
	apps := make([]*appHandle, 0, len(shards))
	cleanup := func() {
		for i, app := range apps {
			cleanupTauriApp(app.cmd, app.exited, shards[i].dataDir, shards[i].socketPath)
		}
	}
	for _, s := range shards {
		app, startErr := startTauriApp(binaryPath, s)
		if startErr != nil {
			return apps, cleanup, fmt.Errorf("failed to start app for %s: %w", s.name, startErr)
		}
		apps = append(apps, app)
	}
	return apps, cleanup, nil
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

// planShards builds the per-shard plan. Shard 0 is the MTP lane; shards
// 1..N are the non-MTP lanes, split by Playwright's --shard X/N.
func planShards(_ string, timestamp int64, pid int) []shardSpec {
	shards := make([]shardSpec, 0, nonMtpShards+1)

	mkLog := func(name string) string {
		return fmt.Sprintf("/tmp/cmdr-e2e-playwright-%s-%d.log", name, timestamp)
	}
	mkJSON := func(name string) string {
		return fmt.Sprintf("/tmp/cmdr-e2e-report-%s.json", name)
	}

	// MTP shard (sequential lane)
	shards = append(shards, shardSpec{
		name:       "mtp",
		kind:       "mtp",
		socketPath: fmt.Sprintf("/tmp/tauri-playwright-mtp-%d.sock", pid),
		mcpPort:    mcpPortBase,
		dataDir:    fmt.Sprintf("/tmp/cmdr-e2e-data-mtp-%d", pid),
		logFile:    mkLog("mtp"),
		jsonReport: mkJSON("mtp"),
	})

	// Non-MTP shards
	for i := 1; i <= nonMtpShards; i++ {
		shards = append(shards, shardSpec{
			name:            fmt.Sprintf("non-mtp-%d", i),
			kind:            "non-mtp",
			socketPath:      fmt.Sprintf("/tmp/tauri-playwright-nonmtp%d-%d.sock", i, pid),
			mcpPort:         mcpPortBase + i,
			dataDir:         fmt.Sprintf("/tmp/cmdr-e2e-data-nonmtp%d-%d", i, pid),
			logFile:         mkLog(fmt.Sprintf("nonmtp%d", i)),
			jsonReport:      mkJSON(fmt.Sprintf("nonmtp%d", i)),
			playwrightShard: fmt.Sprintf("%d/%d", i, nonMtpShards),
		})
	}
	return shards
}

// runShardsInParallel launches one Playwright process per shard and waits for
// all to finish.
func runShardsInParallel(desktopDir string, shards []shardSpec) []shardResult {
	results := make([]shardResult, len(shards))
	var wg sync.WaitGroup
	for i, s := range shards {
		wg.Add(1)
		go func(idx int, shard shardSpec) {
			defer wg.Done()
			results[idx] = runShard(desktopDir, shard)
		}(i, s)
	}
	wg.Wait()
	return results
}

// runShard executes one Playwright process for a single shard.
func runShard(desktopDir string, s shardSpec) shardResult {
	args := []string{
		"exec", "playwright", "test",
		"--config", "test/e2e-playwright/playwright.config.ts",
		"--project", "tauri",
	}
	if s.playwrightShard != "" {
		args = append(args, "--shard", s.playwrightShard)
	}
	cmd := exec.Command("pnpm", args...)
	cmd.Dir = desktopDir
	cmd.Env = append(os.Environ(),
		"CMDR_E2E_START_PATH="+s.fixtureDir,
		"CMDR_MCP_PORT="+strconv.Itoa(s.mcpPort),
		"CMDR_PLAYWRIGHT_SOCKET="+s.socketPath,
		"CMDR_E2E_SHARD_KIND="+s.kind,
		"CMDR_E2E_JSON_REPORT="+s.jsonReport,
		"CMDR_E2E_OUTPUT_DIR="+fmt.Sprintf("/tmp/cmdr-e2e-results-%s", s.name),
	)
	// Only the MTP shard is allowed to wipe the shared virtual MTP backing
	// directory in globalSetup. The non-MTP shards must skip it to avoid
	// stomping on the MTP shard's mid-run state.
	if s.kind != "mtp" {
		cmd.Env = append(cmd.Env, "CMDR_E2E_SKIP_MTP_FIXTURES=1")
	}
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

// parsePlaywrightTotals extracts "N passed", "N failed", "N skipped" counts
// from Playwright's tail summary. Missing counters are zero.
func parsePlaywrightTotals(output string) (passed, failed, skipped int) {
	rePassed := regexp.MustCompile(`(\d+) passed`)
	reFailed := regexp.MustCompile(`(\d+) failed`)
	reSkipped := regexp.MustCompile(`(\d+) skipped`)
	if m := rePassed.FindStringSubmatch(output); len(m) > 1 {
		passed, _ = strconv.Atoi(m[1])
	}
	if m := reFailed.FindStringSubmatch(output); len(m) > 1 {
		failed, _ = strconv.Atoi(m[1])
	}
	if m := reSkipped.FindStringSubmatch(output); len(m) > 1 {
		skipped, _ = strconv.Atoi(m[1])
	}
	return passed, failed, skipped
}

// findTauriBinary locates the built Cmdr binary by querying rustc for the host triple.
func findTauriBinary(rootDir string) (string, error) {
	rustcCmd := exec.Command("rustc", "-vV")
	output, err := RunCommand(rustcCmd, true)
	if err != nil {
		return "", fmt.Errorf("failed to get rust host triple: %w", err)
	}

	var triple string
	for line := range strings.SplitSeq(output, "\n") {
		if rest, ok := strings.CutPrefix(line, "host:"); ok {
			triple = strings.TrimSpace(rest)
			break
		}
	}
	if triple == "" {
		return "", fmt.Errorf("could not parse host triple from `rustc -vV` output")
	}

	binaryPath := filepath.Join(rootDir, "target", triple, "release", "Cmdr")
	if _, err := os.Stat(binaryPath); err != nil {
		return "", fmt.Errorf("built binary not found at %s", binaryPath)
	}
	return binaryPath, nil
}

// createE2EFixtures creates the E2E fixture directory tree (~170 MB) via the shared
// Node.js helper. Returns the fixture directory path. Each call generates a
// unique timestamped path so multiple shards do not collide.
func createE2EFixtures(desktopDir string) (string, error) {
	script := `import { createFixtures } from "./test/e2e-shared/fixtures.js"; console.log(createFixtures())`
	cmd := exec.Command("npx", "tsx", "-e", script)
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return "", fmt.Errorf("failed to create fixtures: %w\n%s", err, indentOutput(output))
	}

	// The script is `console.log(createFixtures())` so the path is on its own
	// line. Scan all lines for one starting with "/"; npm may inject update
	// notices after our output.
	for line := range strings.SplitSeq(strings.TrimSpace(output), "\n") {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "/tmp/cmdr-e2e-") {
			return trimmed, nil
		}
	}
	return "", fmt.Errorf("could not parse fixture path from output:\n%s", indentOutput(output))
}

// startTauriApp launches the Tauri binary in the background for one shard.
// Returns the appHandle (cmd + an exited channel that closes on process exit).
func startTauriApp(binaryPath string, s shardSpec) (*appHandle, error) {
	lf, err := os.Create(s.logFile)
	if err != nil {
		return nil, fmt.Errorf("failed to create log file %s: %w", s.logFile, err)
	}

	// Record the RUST_LOG the app will see, so log readers can tell at a glance
	// whether trace-level output was requested.
	fmt.Fprintf(lf, "=== shard=%s socket=%s mcp_port=%d ===\n", s.name, s.socketPath, s.mcpPort)
	if rustLog := os.Getenv("RUST_LOG"); rustLog != "" {
		fmt.Fprintf(lf, "=== RUST_LOG=%s ===\n", rustLog)
	} else {
		fmt.Fprintln(lf, "=== RUST_LOG unset (default warn level) ===")
	}

	cmd := exec.Command(binaryPath)
	cmd.Env = append(os.Environ(),
		"CMDR_DATA_DIR="+s.dataDir,
		"CMDR_MCP_PORT="+strconv.Itoa(s.mcpPort),
		"CMDR_MCP_ENABLED=true",
		"CMDR_E2E_START_PATH="+s.fixtureDir,
		"CMDR_PLAYWRIGHT_SOCKET="+s.socketPath,
		// Canonical "we're under E2E" marker; soft test hooks gate on this.
		// See docs/testing.md § "E2E env-var hooks" and src-tauri/src/test_mode.rs.
		"CMDR_E2E_MODE=1",
	)
	// Only the MTP shard registers the virtual MTP device. Non-MTP shards skip
	// the startup wipe-and-recreate of the shared backing dir
	// (/tmp/cmdr-mtp-e2e-fixtures), which would otherwise race with the MTP
	// shard's setup and corrupt its in-memory device state.
	if s.kind != "mtp" {
		cmd.Env = append(cmd.Env, "CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP=1")
	}
	cmd.Stdout = lf
	cmd.Stderr = lf
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}

	if err := cmd.Start(); err != nil {
		lf.Close()
		return nil, err
	}
	lf.Close()

	exited := make(chan struct{})
	go func() {
		cmd.Wait()
		close(exited)
	}()

	return &appHandle{cmd: cmd, exited: exited}, nil
}

// waitForPlaywrightSocket polls for the named Unix socket to appear, with a timeout.
func waitForPlaywrightSocket(socketPath string, appExited <-chan struct{}, logFile string) error {
	deadline := time.Now().Add(socketTimeout)
	ticker := time.NewTicker(500 * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-appExited:
			logContent := readLogTail(logFile, 50)
			return fmt.Errorf("app exited before socket appeared (log: %s)\n%s", logFile, indentOutput(logContent))
		case <-ticker.C:
			if fi, err := os.Stat(socketPath); err == nil && fi.Mode()&os.ModeSocket != 0 {
				return nil
			}
			if time.Now().After(deadline) {
				logContent := readLogTail(logFile, 50)
				return fmt.Errorf("socket %s did not appear within %s (log: %s)\n%s",
					socketPath, socketTimeout, logFile, indentOutput(logContent))
			}
		}
	}
}

// cleanupTauriApp kills the app process group, removes the socket, and cleans up the data dir.
func cleanupTauriApp(cmd *exec.Cmd, exited <-chan struct{}, dataDir, socketPath string) {
	if cmd == nil || cmd.Process == nil {
		return
	}

	_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGTERM)

	select {
	case <-exited:
	case <-time.After(processKillGrace):
		_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
		<-exited
	}

	os.Remove(socketPath)
	os.RemoveAll(dataDir)
}

// stopProcessOnPort finds and terminates any process listening on the given TCP port.
func stopProcessOnPort(port string) {
	cmd := exec.Command("lsof", "-ti:"+port)
	output, err := RunCommand(cmd, true)
	if err != nil || strings.TrimSpace(output) == "" {
		return
	}
	for pidStr := range strings.SplitSeq(strings.TrimSpace(output), "\n") {
		if pid, err := strconv.Atoi(strings.TrimSpace(pidStr)); err == nil {
			_ = syscall.Kill(pid, syscall.SIGTERM)
		}
	}
	time.Sleep(500 * time.Millisecond)
}

// extractE2ETestOutput returns a concise failure summary for E2E test runs.
// The captured output has four sections with stable delimiters:
//
//	§1 setup/build         → trimmed at the last "Starting Tauri app..."
//	§2 per-test progress   → ✓/- markers (and their preceding annotation
//	                          lines) are dropped; ✘ markers and their
//	                          preceding annotation lines are kept
//	§3 failure blocks      → kept verbatim (numbered `N) [tauri] …` blocks
//	                          plus the final `N failed / M flaky / X passed`
//	                          tally)
//	§4 post-ELIFECYCLE     → dropped (this is the Tauri stdout dump and
//	                          out-of-order build output Docker flushes after
//	                          the run exits, already saved in the full log
//	                          file the surrounding error message links to)
//
// If the run died before reaching the test phase (e.g. SMB container setup
// failed silently in desktop-e2e-linux), none of §1, §3, or the tally exist.
// We detect that by absence of all of: the Tauri start marker, a numbered
// failure block, and a `N passed`/`N failed` tally. In that case the full
// pre-ELIFECYCLE transcript is kept, the verbose `docker compose ps` table
// is dropped, and a one-line hint is prepended.
//
// The Tauri-marker check alone is insufficient because the macOS playwright
// shards start Tauri in the Go check (with its stdout going to a log file),
// so the marker never appears in Playwright's stdout, even on a successful
// run.
func extractE2ETestOutput(output string) string {
	// Extract any SMB-stack readiness lines from §1 BEFORE we trim the setup
	// phase away. These banners come from desktop-e2e-linux's pre-flight and
	// post-flight probes and are crucial signal for diagnosing SMB-related
	// test failures — they answer "were the SMB containers healthy when
	// tests started / ended?" Without preserving them, every SMB failure
	// looks like a pure Cmdr-side bug.
	smbBanners := extractSMBBanners(output)

	tauriStarted := strings.Contains(output, "Starting Tauri app...")
	if tauriStarted {
		idx := strings.LastIndex(output, "Starting Tauri app...")
		output = output[idx:]
	}
	if idx := strings.Index(output, "[ELIFECYCLE]"); idx >= 0 {
		if eol := strings.IndexByte(output[idx:], '\n'); eol >= 0 {
			output = output[:idx+eol]
		} else {
			output = output[:idx]
		}
	}
	lines := strings.Split(output, "\n")
	boundary := len(lines)
	for i, line := range lines {
		if failureBlockHeaderRE.MatchString(stripANSI(line)) {
			boundary = i
			break
		}
	}
	kept := filterTestProgress(lines[:boundary])
	kept = append(kept, lines[boundary:]...)

	if isPreTestFailure(output, lines, boundary) {
		kept = dropDockerComposePsTable(kept)
		kept = append(
			[]string{"note: tests did not reach the run phase; failure was in pre-test setup. See full log for details.", ""},
			kept...,
		)
	}

	if len(smbBanners) > 0 {
		kept = append(smbBanners, append([]string{""}, kept...)...)
	}
	return strings.Join(kept, "\n")
}

// smbBannerRE matches the pre-flight and post-flight SMB readiness banners
// emitted by `e2e-linux.sh` (via `log_info`/`log_warn`). ANSI colour codes
// are stripped before matching. Anchored on substring rather than start of
// line so the `[INFO]` / `[WARN]` prefix is tolerated.
var smbBannerRE = regexp.MustCompile(`SMB (?:e2e stack ready|post-flight): .+`)

// extractSMBBanners pulls out the SMB readiness banners (pre-flight and
// post-flight) from raw output. Returned strings have ANSI escapes removed
// and a `[SMB] ` prefix added so they're trivially greppable in the
// failing-test summary.
func extractSMBBanners(output string) []string {
	var out []string
	for line := range strings.SplitSeq(output, "\n") {
		stripped := stripANSI(line)
		if m := smbBannerRE.FindString(stripped); m != "" {
			out = append(out, "[SMB] "+m)
		}
	}
	return out
}

// playwrightTallyRE matches the Playwright run-summary lines like `1 failed`,
// `42 passed (1.2m)`, `3 flaky`. Presence of any of these, or of a
// `\d+) [tauri]` failure block, proves the run reached the test phase.
var playwrightTallyRE = regexp.MustCompile(`(?m)^\s*\d+\s+(?:passed|failed|flaky|skipped)\b`)

// isPreTestFailure reports whether the captured output looks like the run
// died before reaching the Playwright test phase. True only if NONE of these
// are present: the `Starting Tauri app...` marker, a `\d+) [tauri] …`
// failure-block header, or a `\d+ (passed|failed|flaky|skipped)` tally line.
func isPreTestFailure(rawOutput string, prefilterLines []string, failureBlockBoundary int) bool {
	if strings.Contains(rawOutput, "Starting Tauri app...") {
		return false
	}
	if failureBlockBoundary < len(prefilterLines) {
		// failureBlockHeaderRE already matched at this index.
		return false
	}
	return !playwrightTallyRE.MatchString(rawOutput)
}

// dockerPsHeaderRE matches the column header emitted by `docker compose ps`.
// The exact column set varies by Docker version but NAME and IMAGE are always
// the first two, separated by run-length whitespace.
var dockerPsHeaderRE = regexp.MustCompile(`^NAME\s+IMAGE\s+COMMAND\b`)

// dockerPsRowRE matches a `docker compose ps` data row, identified by the
// container-status token `Up <duration> [(state)]`. Only used once a header
// has been seen (see dropDockerComposePsTable), so similar phrases in prose
// can't trigger it.
var dockerPsRowRE = regexp.MustCompile(`\bUp \d+\s+\w+(\s+\((healthy|unhealthy|starting)\))?`)

// dropDockerComposePsTable removes the column header and data rows of any
// `docker compose ps` block embedded in the output. To avoid eating benign
// prose that happens to contain `Up <N> …`, rows are only dropped after a
// matching `NAME IMAGE COMMAND` header line; the next blank line or
// non-matching line ends the table.
func dropDockerComposePsTable(lines []string) []string {
	out := make([]string, 0, len(lines))
	inTable := false
	for _, line := range lines {
		stripped := stripANSI(line)
		if dockerPsHeaderRE.MatchString(stripped) {
			inTable = true
			continue
		}
		if inTable {
			if strings.TrimSpace(stripped) == "" || !dockerPsRowRE.MatchString(stripped) {
				inTable = false
				out = append(out, line)
				continue
			}
			continue
		}
		out = append(out, line)
	}
	return out
}

// failureBlockHeaderRE matches the first line of a Playwright failure entry,
// e.g. "  1) [tauri] › test/e2e-playwright/smb.spec.ts:206:3 › …". This is
// the §2 → §3 boundary.
var failureBlockHeaderRE = regexp.MustCompile(`^\s*\d+\)\s+\[tauri\]\s`)

// ansiEscapeRE matches ANSI CSI escape sequences (e.g. color codes).
var ansiEscapeRE = regexp.MustCompile(`\x1b\[[0-9;]*[A-Za-z]`)

func stripANSI(s string) string {
	return ansiEscapeRE.ReplaceAllString(s, "")
}

// filterTestProgress collapses Playwright per-test progress output: lines
// preceding a ✓ or - marker (and the marker itself) are dropped, while lines
// preceding a ✘ marker (and the marker) are kept. Lines that have no marker
// at all at the end of the section (typically blank padding before §3) are
// kept too.
func filterTestProgress(lines []string) []string {
	out := make([]string, 0, len(lines))
	var buf []string
	for _, line := range lines {
		trimmed := strings.TrimSpace(stripANSI(line))
		switch {
		case strings.HasPrefix(trimmed, "✘"):
			out = append(out, buf...)
			out = append(out, line)
			buf = buf[:0]
		case strings.HasPrefix(trimmed, "✓"), strings.HasPrefix(trimmed, "- "):
			buf = buf[:0]
		default:
			buf = append(buf, line)
		}
	}
	out = append(out, buf...)
	return out
}

// readLogTail reads the last N lines of a log file.
func readLogTail(path string, n int) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Sprintf("(could not read log: %v)", err)
	}
	lines := strings.Split(string(data), "\n")
	start := max(len(lines)-n, 0)
	return strings.Join(lines[start:], "\n")
}

// appendToLogFile appends text to a log file.
func appendToLogFile(path, text string) {
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return
	}
	defer f.Close()
	f.WriteString(text)
}

// codesignDevBinary signs the binary with a local dev certificate so macOS Keychain
// doesn't prompt on every rebuild. The identity is stable across builds, so Keychain
// items created by a signed binary remain accessible to future signed builds.
// Skipped on non-macOS and when no signing identity is available.
func codesignDevBinary(binaryPath string) error {
	if runtime.GOOS != "darwin" {
		return nil
	}

	identity := os.Getenv("CMDR_DEV_SIGNING_IDENTITY")
	if identity == "" {
		identity = "Cmdr Dev"
	}

	checkCmd := exec.Command("security", "find-identity", "-v", "-p", "codesigning")
	checkOutput, err := RunCommand(checkCmd, true)
	if err != nil || !strings.Contains(checkOutput, "\""+identity+"\"") {
		return nil
	}

	cmd := exec.Command("codesign", "--force", "-s", identity, binaryPath)
	output, err := RunCommand(cmd, true)
	if err != nil {
		return fmt.Errorf("codesign failed for %s: %w\n%s", binaryPath, err, indentOutput(output))
	}
	return nil
}
