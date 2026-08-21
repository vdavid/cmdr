package checks

import (
	"fmt"
	"net"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// The per-shard app lifecycle for the macOS Playwright lane: fixtures, ports,
// and the Tauri process itself. The check file next door owns the shard plan and
// the run; this file owns everything a shard needs to exist and to be torn down
// again. `checks/DETAILS.md` § "Nothing a shard owns is shared between runs" is
// the canonical list of what each shard owns.

const (
	socketTimeout    = 60 * time.Second
	processKillGrace = 3 * time.Second
)

type appHandle struct {
	cmd    *exec.Cmd
	exited <-chan struct{}
}

// allocateShardFixtures creates one fixture directory per shard and returns a
// cleanup function that removes them all. On error, any fixtures created so
// far are removed before returning.
func allocateShardFixtures(desktopDir string, shards []shardSpec) (func(), error) {
	for i := range shards {
		fixtureDir, err := createE2EFixtures(desktopDir, shards[i].instanceID)
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

// reserveMcpPorts asks the OS for `count` free loopback ports, one per shard.
//
// Every listener stays open until the last one is reserved: closing each before
// opening the next lets the kernel hand the same port back, and two shards on one
// port is the collision this exists to avoid. They're released on return, so the
// apps can bind them — a window, but a freshly-released ephemeral port isn't what
// the OS reaches for next.
//
// ❌ Never go back to a fixed base port. It's what let one suite's pre-flight kill
// another suite's running app.
func reserveMcpPorts(count int) ([]int, error) {
	listeners := make([]net.Listener, 0, count)
	defer func() {
		for _, l := range listeners {
			l.Close()
		}
	}()

	ports := make([]int, 0, count)
	for range count {
		l, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			return nil, fmt.Errorf("couldn't reserve an MCP port for the E2E shards: %w", err)
		}
		listeners = append(listeners, l)
		ports = append(ports, l.Addr().(*net.TCPAddr).Port)
	}
	return ports, nil
}

// mtpFixtureRootForRun is the backing directory for this run's virtual MTP device.
//
// Run-scoped, because the MTP shard WIPES it at startup and between tests. Shared,
// a suite starting while another is mid-MTP-spec deletes the tree that spec is
// asserting against, and the victim reports a missing file it created itself.
// `mtp-fixtures.ts` reads the same path out of `CMDR_MTP_FIXTURE_ROOT`, and the app
// out of `CMDR_VIRTUAL_MTP`; the prefix is what that helper's delete guard allows.
func mtpFixtureRootForRun(pid int) string {
	return fmt.Sprintf("/tmp/cmdr-mtp-e2e-fixtures-%d", pid)
}

// createE2EFixtures creates the E2E fixture directory tree (~170 MB) via the shared
// Node.js helper. Returns the fixture directory path. Each call generates a
// unique timestamped path under /tmp/cmdr-e2e-fixtures-<instance>-<ts>/ so
// parallel shards never collide. Bulk .dat files are hard-linked from a shared
// cache at /tmp/cmdr-e2e-fixtures-cache/ (built on first call); see
// e2e-shared/fixtures.ts for the cache build protocol.
func createE2EFixtures(desktopDir, instanceID string) (string, error) {
	// The instance ID is passed via env (not a CLI arg) because tsx's `-e` form
	// evaluates a single expression string; smuggling args through that would
	// need a wrapper file. Env is the path of least surprise.
	script := `import { createFixtures } from "./test/e2e-shared/fixtures.js"; console.log(createFixtures(process.env.CMDR_INSTANCE_ID))`
	cmd := exec.Command("npx", "tsx", "-e", script)
	cmd.Dir = desktopDir
	cmd.Env = append(os.Environ(), "CMDR_INSTANCE_ID="+instanceID)
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
	if err := pinUiLanguage(s.dataDir); err != nil {
		return nil, err
	}
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

	cmd := exec.Command(binaryPath, enUsLocaleArgs()...)
	cmd.Env = append(os.Environ(),
		// CMDR_INSTANCE_ID drives the macOS Keychain SERVICE_NAME suffix
		// ("Cmdr-<instance>") and the Dock label ("Cmdr (E2E <kind>)") so parallel shards
		// never collide on credentials and `pgrep -f 'Cmdr (E2E '` can target them. The
		// data dir / port / socket below are still composed explicitly: the checker holds
		// the per-shard derivation rather than asking the binary to recompute from the
		// instance ID, keeping the Rust side env-driven (precedence rules in
		// docs/specs/instance-isolation-plan.md § "Precedence rules").
		"CMDR_INSTANCE_ID="+s.instanceID,
		"CMDR_DATA_DIR="+s.dataDir,
		"CMDR_MCP_PORT="+strconv.Itoa(s.mcpPort),
		"CMDR_MCP_ENABLED=true",
		"CMDR_E2E_START_PATH="+s.fixtureDir,
		"CMDR_PLAYWRIGHT_SOCKET="+s.socketPath,
		// Canonical "we're under E2E" marker; soft test hooks gate on this.
		// See docs/testing.md § "E2E env-var hooks" and src-tauri/src/test_mode.rs.
		"CMDR_E2E_MODE=1",
		// Answer the FDA probe the same way on every Mac, which is what the suite already
		// assumes. On a machine that never granted Full Disk Access the gate stays pending,
		// and that costs 88 of 290 tests: every MTP spec plus an onboarding-wizard cascade.
		// Why, and the alternative: e2e-playwright/DETAILS.md § "The Full Disk Access pin".
		"CMDR_MOCK_FDA=granted",
		// Drive Ask Cmdr's send path through the deterministic scripted fake LLM
		// (commands/agent.rs::resolve_agent_llm gates on this), so ask-cmdr.spec.ts can
		// assert send-and-render with no provider. It MUST live on the APP process env:
		// resolve_agent_llm runs in the app, not the Playwright runner.
		"CMDR_E2E_ASK_CMDR_FAKE=1",
		// Pause a search's cover walk before each directory read, so a spec can watch
		// results arrive and a snapshot pane grow mid-walk instead of racing ground
		// that finishes in milliseconds (search-live + search-walk-handoff). Cover
		// walks only, so background indexing is untouched. Those specs walk a directory
		// CHAIN, which no walker parallelism can overlap, so this is a per-directory
		// floor rather than an average: 30 levels can't finish in under three seconds.
		"CMDR_E2E_WALK_THROTTLE_MS=100",
	)
	// Only the MTP shard registers the virtual MTP device, at THIS run's backing
	// dir. Non-MTP shards skip the startup wipe-and-recreate, which would
	// otherwise race with the MTP shard's setup and corrupt its in-memory device
	// state. A path value (rather than `1`) is what points the app away from the
	// machine-wide default: see `virtual_device.rs::decide_startup_root`.
	if s.kind == "mtp" {
		cmd.Env = append(cmd.Env, "CMDR_VIRTUAL_MTP="+s.mtpFixtureRoot)
	} else {
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
