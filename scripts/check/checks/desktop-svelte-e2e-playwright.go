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
	"syscall"
	"time"
)

const (
	playwrightMCPPort    = "9429"
	playwrightSocketPath = "/tmp/tauri-playwright.sock"
	socketTimeout        = 60 * time.Second
	processKillGrace     = 3 * time.Second
)

// RunDesktopE2EPlaywright runs Playwright E2E tests against the real Tauri app.
// Self-contained lifecycle: build binary → start app → run tests → cleanup.
func RunDesktopE2EPlaywright(ctx *CheckContext) (CheckResult, error) {
	if runtime.GOOS != "darwin" {
		return Skipped("macOS only (use desktop-e2e-linux for Linux)"), nil
	}

	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	timestamp := time.Now().Unix()
	logFile := fmt.Sprintf("/tmp/cmdr-e2e-playwright-%d.log", timestamp)
	dataDir := fmt.Sprintf("/tmp/cmdr-e2e-data-%d", os.Getpid())

	// ── Step 1: Build the Tauri binary ──────────────────────────────────
	buildCmd := exec.Command("pnpm", "test:e2e:playwright:build")
	buildCmd.Dir = desktopDir
	buildOutput, err := RunCommand(buildCmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("tauri build failed (log: %s)\n%s", logFile, indentOutput(buildOutput))
	}

	// ── Step 2: Find the built binary ───────────────────────────────────
	binaryPath, err := findTauriBinary(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	// ── Step 2.5: Code-sign for Keychain access ────────────────────────
	if err := codesignDevBinary(binaryPath); err != nil {
		return CheckResult{}, err
	}

	// ── Step 3: Create fixture directory ────────────────────────────────
	fixtureDir, err := createE2EFixtures(desktopDir)
	if err != nil {
		return CheckResult{}, err
	}
	defer os.RemoveAll(fixtureDir)

	// ── Step 4: Clean stale state ───────────────────────────────────────
	os.Remove(playwrightSocketPath)
	stopProcessOnPort(playwrightMCPPort)

	// ── Step 5: Start the app ───────────────────────────────────────────
	appCmd, appExited, err := startTauriApp(binaryPath, dataDir, fixtureDir, logFile)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to start app: %w", err)
	}
	defer cleanupTauriApp(appCmd, appExited, dataDir)

	// ── Step 6: Wait for the socket ─────────────────────────────────────
	if err := waitForPlaywrightSocket(appExited, logFile); err != nil {
		return CheckResult{}, err
	}

	// ── Step 7: Run the tests ───────────────────────────────────────────
	testCmd := exec.Command("pnpm", "test:e2e:playwright")
	testCmd.Dir = desktopDir
	testCmd.Env = append(os.Environ(),
		"CMDR_E2E_START_PATH="+fixtureDir,
		"CMDR_MCP_PORT="+playwrightMCPPort,
	)
	testOutput, testErr := RunCommand(testCmd, true)

	// Append test output to the log file for post-mortem debugging
	appendToLogFile(logFile, "\n\n=== Playwright test output ===\n"+testOutput)

	if testErr != nil {
		summary := extractE2ETestOutput(testOutput)
		return CheckResult{}, fmt.Errorf("playwright E2E tests failed (full log: %s)\n%s", logFile, indentOutput(summary))
	}

	// ── Step 8: Parse results ───────────────────────────────────────────
	re := regexp.MustCompile(`(\d+) passed`)
	matches := re.FindStringSubmatch(testOutput)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))), nil
	}
	return Success("All Playwright E2E tests passed"), nil
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
// Node.js helper. Returns the fixture directory path.
func createE2EFixtures(desktopDir string) (string, error) {
	script := `import { createFixtures } from "./test/e2e-shared/fixtures.js"; console.log(createFixtures())`
	cmd := exec.Command("npx", "tsx", "-e", script)
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return "", fmt.Errorf("failed to create fixtures: %w\n%s", err, indentOutput(output))
	}

	// The script is `console.log(createFixtures())` so the path is the last line.
	lines := strings.Split(strings.TrimSpace(output), "\n")
	lastLine := strings.TrimSpace(lines[len(lines)-1])
	if strings.HasPrefix(lastLine, "/") {
		return lastLine, nil
	}
	return "", fmt.Errorf("could not parse fixture path from output:\n%s", indentOutput(output))
}

// startTauriApp launches the Tauri binary in the background. Returns the exec.Cmd,
// a channel that closes when the process exits, and any start error.
func startTauriApp(binaryPath, dataDir, fixtureDir, logFile string) (*exec.Cmd, <-chan struct{}, error) {
	lf, err := os.Create(logFile)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to create log file %s: %w", logFile, err)
	}

	cmd := exec.Command(binaryPath)
	cmd.Env = append(os.Environ(),
		"CMDR_DATA_DIR="+dataDir,
		"CMDR_MCP_PORT="+playwrightMCPPort,
		"CMDR_MCP_ENABLED=true",
		"CMDR_E2E_START_PATH="+fixtureDir,
	)
	cmd.Stdout = lf
	cmd.Stderr = lf
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}

	if err := cmd.Start(); err != nil {
		lf.Close()
		return nil, nil, err
	}
	lf.Close()

	// Monitor the process in a goroutine so waitForPlaywrightSocket can detect early exits.
	exited := make(chan struct{})
	go func() {
		cmd.Wait()
		close(exited)
	}()

	return cmd, exited, nil
}

// waitForPlaywrightSocket polls for the Unix socket to appear, with a timeout.
func waitForPlaywrightSocket(appExited <-chan struct{}, logFile string) error {
	deadline := time.Now().Add(socketTimeout)
	ticker := time.NewTicker(500 * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-appExited:
			logContent := readLogTail(logFile, 50)
			return fmt.Errorf("app exited before socket appeared (log: %s)\n%s", logFile, indentOutput(logContent))
		case <-ticker.C:
			if fi, err := os.Stat(playwrightSocketPath); err == nil && fi.Mode()&os.ModeSocket != 0 {
				return nil
			}
			if time.Now().After(deadline) {
				logContent := readLogTail(logFile, 50)
				return fmt.Errorf("socket did not appear within %s (log: %s)\n%s", socketTimeout, logFile, indentOutput(logContent))
			}
		}
	}
}

// cleanupTauriApp kills the app process group, removes the socket, and cleans up the data dir.
// The exited channel is the one returned by startTauriApp — it closes when cmd.Wait() completes.
func cleanupTauriApp(cmd *exec.Cmd, exited <-chan struct{}, dataDir string) {
	if cmd == nil || cmd.Process == nil {
		return
	}

	// SIGTERM the process group (negative PID kills the group)
	_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGTERM)

	// Wait briefly for graceful shutdown, then force kill
	select {
	case <-exited:
	case <-time.After(processKillGrace):
		_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
		<-exited
	}

	os.Remove(playwrightSocketPath)
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

// extractE2ETestOutput returns everything from "Starting Tauri app..." onward,
// stripping the setup preamble (Docker, apt-get, pnpm install, Playwright download).
func extractE2ETestOutput(output string) string {
	idx := strings.LastIndex(output, "Starting Tauri app...")
	if idx >= 0 {
		return output[idx:]
	}
	return output
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

	// Allow CI to override the identity name (for example, "Cmdr Dev CI").
	identity := os.Getenv("CMDR_DEV_SIGNING_IDENTITY")
	if identity == "" {
		identity = "Cmdr Dev"
	}

	// Check if the identity exists in the keychain. If not, skip silently —
	// signing is optional (the app works without it, just with Keychain prompts).
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
