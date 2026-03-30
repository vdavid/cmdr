package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"strings"
	"time"

	"cmdr/scripts/check/checks"
)

const (
	freestyleAPIBase  = "https://api.freestyle.sh"
	freestyleVMIDFile = ".freestyle-vm-id"
)

// freestyleRun offloads check runs to a freestyle.sh VM.
// Only checks marked FreestyleCompat run (skips Rust compilation, Docker, etc.).
func freestyleRun(rootDir string, args []string) error {
	apiKey, err := getFreestyleAPIKey()
	if err != nil {
		return err
	}

	vmID, err := ensureFreestyleVM(rootDir, apiKey)
	if err != nil {
		return err
	}

	if err := wakeAndVerifyVM(apiKey, vmID); err != nil {
		fmt.Printf("⚠️  VM unhealthy, replacing it...\n")
		deleteVM(apiKey, vmID)
		vmID, err = createFreestyleVM(rootDir, apiKey)
		if err != nil {
			return err
		}
	}

	tempBranch, err := pushSyncBranch(rootDir)
	if err != nil {
		return err
	}
	defer cleanupSyncBranch(rootDir, tempBranch)

	fmt.Print("📤 Syncing to VM... ")
	syncStart := time.Now()
	if err := fetchSyncOnVM(apiKey, vmID, tempBranch); err != nil {
		return fmt.Errorf("sync failed: %w", err)
	}
	fmt.Printf("%sdone%s %s\n", colorGreen, colorReset, formatDuration(time.Since(syncStart)))

	filteredArgs := filterFreestyleArgs(args)
	printSkippedChecksIfUnfiltered(filteredArgs)

	remoteCmd := fmt.Sprintf(
		"export MISE_TRUSTED_CONFIG_PATHS=/root/cmdr && eval \"$(/root/.local/bin/mise activate bash)\" && cd /root/cmdr && ./scripts/check.sh --ci --no-log --freestyle-remote %s",
		strings.Join(filteredArgs, " "),
	)

	fmt.Printf("🚀 Running checks on freestyle.sh VM %s...\n\n", vmID)
	return execOnVM(apiKey, vmID, remoteCmd)
}

// preferFreestyleRun runs freestyle-compatible checks on a VM and the rest locally, in parallel.
// Returns exit code 0 on success, 1 on failure.
func preferFreestyleRun(rootDir string, args []string, flags *cliFlags) int {
	// Start freestyle (VM) checks in a goroutine
	freestyleErrCh := make(chan error, 1)
	go func() {
		freestyleErrCh <- freestyleRun(rootDir, args)
	}()

	// Run local-only (freestyle-incompat) checks
	ctx := &checks.CheckContext{
		CI:      flags.ciMode,
		Verbose: flags.verbose,
		RootDir: rootDir,
	}

	localChecks, err := selectChecks(flags)
	if err != nil {
		printError("Error: %v", err)
		return 1
	}
	localChecks = checks.FilterSlowChecks(localChecks, flags.includeSlow)
	localChecks = checks.FilterFreestyleIncompat(localChecks)

	if flags.onlySlow {
		var slow []checks.CheckDefinition
		for _, c := range localChecks {
			if c.IsSlow {
				slow = append(slow, c)
			}
		}
		localChecks = slow
	}

	localFailed := false
	if len(localChecks) > 0 {
		fmt.Printf("\n🏠 Running %d local-only checks (freestyle-incompatible)...\n\n", len(localChecks))

		if needsPnpmInstall(localChecks) {
			if err := ensurePnpmDependencies(ctx); err != nil {
				printError("Error: %v", err)
				localFailed = true
			}
		}

		if !localFailed {
			startTime := time.Now()
			runner := NewRunner(ctx, localChecks, flags.failFast, flags.noLog)
			failed, failedChecks := runner.Run()

			totalDuration := time.Since(startTime)
			fmt.Println()
			fmt.Printf("%s⏱️  Local checks runtime: %s%s\n", colorYellow, formatDuration(totalDuration), colorReset)

			if failed {
				printFailure(failedChecks)
				localFailed = true
			} else {
				fmt.Printf("%s✅ Local checks passed!%s\n", colorGreen, colorReset)
			}
		}
	}

	// Wait for freestyle to finish
	fmt.Printf("\n⏳ Waiting for freestyle VM checks to finish...\n")
	freestyleErr := <-freestyleErrCh
	freestyleFailed := freestyleErr != nil
	if freestyleFailed {
		printError("Freestyle error: %v", freestyleErr)
	}

	// Final summary
	fmt.Println()
	if localFailed || freestyleFailed {
		fmt.Printf("%s❌ Some checks failed.%s\n", colorRed, colorReset)
		if localFailed {
			fmt.Printf("  Local checks: %sFAILED%s\n", colorRed, colorReset)
		}
		if freestyleFailed {
			fmt.Printf("  Freestyle checks: %sFAILED%s\n", colorRed, colorReset)
		}
		return 1
	}

	fmt.Printf("%s✅ All checks passed! (local + freestyle)%s\n", colorGreen, colorReset)
	return 0
}

// filterFreestyleArgs removes --only-freestyle and --prefer-freestyle from the arg list.
func filterFreestyleArgs(args []string) []string {
	var filtered []string
	for _, a := range args {
		if a != "--only-freestyle" && a != "--prefer-freestyle" {
			filtered = append(filtered, a)
		}
	}
	return filtered
}

// printSkippedChecksIfUnfiltered prints a skip message when no specific check filter is active.
func printSkippedChecksIfUnfiltered(args []string) {
	for _, a := range args {
		if a == "--rust" || a == "--svelte" || a == "--go" || a == "--check" ||
			a == "--rust-only" || a == "--svelte-only" || a == "--go-only" ||
			strings.HasPrefix(a, "--app") {
			return
		}
	}
	skipped := countSkippedChecks()
	if skipped > 0 {
		fmt.Printf("%s⏭️  Skipping %d checks not compatible with freestyle (Rust, Docker)%s\n", colorDim, skipped, colorReset)
	}
}

func countSkippedChecks() int {
	count := 0
	for _, c := range checks.AllChecks {
		if !c.FreestyleCompat {
			count++
		}
	}
	return count
}

// pushSyncBranch creates a temp commit of the working tree and pushes it.
func pushSyncBranch(rootDir string) (string, error) {
	fmt.Print("📤 Pushing sync branch... ")
	startTime := time.Now()

	tempBranch := fmt.Sprintf("freestyle-sync-%d", time.Now().Unix())

	tmpIndex := rootDir + "/.git/freestyle-tmp-index"
	defer os.Remove(tmpIndex)

	realIndex := rootDir + "/.git/index"
	indexData, err := os.ReadFile(realIndex)
	if err != nil {
		return "", fmt.Errorf("failed to read index: %w", err)
	}
	if err := os.WriteFile(tmpIndex, indexData, 0644); err != nil {
		return "", fmt.Errorf("failed to create temp index: %w", err)
	}

	addCmd := exec.Command("git", "add", "-A")
	addCmd.Dir = rootDir
	addCmd.Env = append(os.Environ(), "GIT_INDEX_FILE="+tmpIndex)
	if out, err := addCmd.CombinedOutput(); err != nil {
		return "", fmt.Errorf("failed to stage files: %w\n%s", err, string(out))
	}

	writeTreeCmd := exec.Command("git", "write-tree")
	writeTreeCmd.Dir = rootDir
	writeTreeCmd.Env = append(os.Environ(), "GIT_INDEX_FILE="+tmpIndex)
	treeOut, err := writeTreeCmd.Output()
	if err != nil {
		return "", fmt.Errorf("failed to write tree: %w", err)
	}
	treeHash := strings.TrimSpace(string(treeOut))

	headRef, _ := gitOutput(rootDir, "rev-parse", "HEAD")
	commitHash, err := gitOutput(rootDir, "commit-tree", treeHash, "-p", headRef, "-m", "freestyle-sync")
	if err != nil {
		return "", fmt.Errorf("failed to create sync commit: %w", err)
	}

	if err := gitRun(rootDir, "update-ref", "refs/heads/"+tempBranch, commitHash); err != nil {
		return "", fmt.Errorf("failed to create temp branch: %w", err)
	}

	if err := gitRun(rootDir, "push", "origin", tempBranch, "--force"); err != nil {
		_ = gitRun(rootDir, "branch", "-D", tempBranch)
		return "", fmt.Errorf("failed to push temp branch: %w", err)
	}
	_ = gitRun(rootDir, "branch", "-D", tempBranch)

	fmt.Printf("%sdone%s %s\n", colorGreen, colorReset, formatDuration(time.Since(startTime)))
	return tempBranch, nil
}

func cleanupSyncBranch(rootDir string, tempBranch string) {
	_ = gitRun(rootDir, "push", "origin", "--delete", tempBranch)
}

func fetchSyncOnVM(apiKey string, vmID string, tempBranch string) error {
	cmd := fmt.Sprintf(
		"cd /root/cmdr && git fetch origin %s && git checkout --force FETCH_HEAD && git clean -fd",
		tempBranch,
	)
	return execOnVMSilent(apiKey, vmID, cmd)
}

// ensureFreestyleVM returns the VM ID, creating one if needed.
func ensureFreestyleVM(rootDir string, apiKey string) (string, error) {
	vmIDPath := rootDir + "/" + freestyleVMIDFile
	if data, err := os.ReadFile(vmIDPath); err == nil {
		vmID := strings.TrimSpace(string(data))
		if vmID != "" {
			state, err := getVMState(apiKey, vmID)
			if err == nil {
				fmt.Printf("☁️  VM %s (state: %s)\n", vmID, state)
				return vmID, nil
			}
			fmt.Printf("⚠️  Saved VM %s not reachable, creating a new one...\n", vmID)
		}
	}

	return createFreestyleVM(rootDir, apiKey)
}

func createFreestyleVM(rootDir string, apiKey string) (string, error) {
	fmt.Println("☁️  Creating freestyle.sh VM (first-time setup)...")

	body := map[string]any{
		"persistence": map[string]any{"type": "sticky", "priority": 10},
		"aptDeps":     []string{"curl", "build-essential", "pkg-config", "libssl-dev", "git"},
		"workdir":     "/root",
	}
	jsonBody, _ := json.Marshal(body)

	resp, err := freestyleRequest(apiKey, "POST", "/v1/vms", jsonBody)
	if err != nil {
		return "", fmt.Errorf("failed to create VM: %w", err)
	}

	var result struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(resp, &result); err != nil || result.ID == "" {
		return "", fmt.Errorf("unexpected create VM response: %s", string(resp))
	}

	fmt.Printf("☁️  VM created: %s — installing toolchain...\n", result.ID)

	setupScript := `set -e
curl -fsSL https://mise.run | sh
export PATH="$HOME/.local/bin:$PATH"

mkdir -p /root/.config/mise
printf '[settings]\ndisable_tools = ["pnpm"]\n' > /root/.config/mise/config.toml
printf 'export MISE_TRUSTED_CONFIG_PATHS=/root/cmdr\neval "$(/root/.local/bin/mise activate bash)"\n' >> ~/.bashrc

git clone https://github.com/vdavid/cmdr.git /root/cmdr
export MISE_TRUSTED_CONFIG_PATHS=/root/cmdr
cd /root/cmdr

mise install
eval "$(/root/.local/bin/mise activate bash)"

npm install -g pnpm@10
pnpm install --frozen-lockfile

DEBIAN_FRONTEND=noninteractive pnpm --filter website exec playwright install --with-deps chromium

echo "SETUP_COMPLETE"
`
	if err := execOnVM(apiKey, result.ID, setupScript); err != nil {
		return "", fmt.Errorf("VM setup failed: %w", err)
	}

	vmIDPath := rootDir + "/" + freestyleVMIDFile
	if err := os.WriteFile(vmIDPath, []byte(result.ID+"\n"), 0644); err != nil {
		fmt.Printf("⚠️  Could not save VM ID to %s: %v\n", vmIDPath, err)
	}

	fmt.Println("☁️  VM ready!")
	return result.ID, nil
}

// wakeAndVerifyVM runs a lightweight command to wake a suspended VM and verify
// that the basic toolchain (go, pnpm, node) is available. This catches environment
// issues early with a clear error instead of a cryptic "status 255" from a later step.
func wakeAndVerifyVM(apiKey string, vmID string) error {
	fmt.Print("🔌 Waking VM and verifying toolchain... ")
	startTime := time.Now()

	diag := `echo "vm_ok" && which go && which pnpm && which node && go version && pnpm --version && node --version`
	err := execOnVMSilent(apiKey, vmID, diag)
	if err != nil {
		fmt.Printf("%sstale%s\n", colorYellow, colorReset)
		return fmt.Errorf("toolchain missing or broken: %w", err)
	}

	fmt.Printf("%sok%s %s\n", colorGreen, colorReset, formatDuration(time.Since(startTime)))
	return nil
}

// --- API helpers ---

func getFreestyleAPIKey() (string, error) {
	cmd := exec.Command("security", "find-generic-password", "-a", os.Getenv("USER"), "-s", "FREESTYLE_SH_API_TOKEN", "-w")
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("failed to get FREESTYLE_SH_API_TOKEN from Keychain: %w\nStore it with: security add-generic-password -a \"$USER\" -s FREESTYLE_SH_API_TOKEN -w <your-token>", err)
	}
	return strings.TrimSpace(string(out)), nil
}

func execOnVM(apiKey string, vmID string, command string) error {
	body, _ := json.Marshal(map[string]any{
		"command":   command,
		"timeoutMs": 1800000,
	})

	resp, err := freestyleRequest(apiKey, "POST", fmt.Sprintf("/v1/vms/%s/exec-await", vmID), body)
	if err != nil {
		return err
	}

	var result struct {
		Stdout     *string `json:"stdout"`
		Stderr     *string `json:"stderr"`
		StatusCode *int    `json:"statusCode"`
	}
	if err := json.Unmarshal(resp, &result); err != nil {
		return fmt.Errorf("failed to parse exec response: %w\nRaw: %s", err, string(resp))
	}

	if result.Stdout != nil && *result.Stdout != "" {
		fmt.Print(*result.Stdout)
	}
	if result.Stderr != nil && *result.Stderr != "" {
		fmt.Fprint(os.Stderr, *result.Stderr)
	}

	if result.StatusCode != nil && *result.StatusCode != 0 {
		// Build a detailed error: the stdout/stderr were already printed above,
		// but also include the tail of stderr (or stdout) in the error itself
		// so callers wrapping this error get useful context.
		detail := ""
		if result.Stderr != nil && *result.Stderr != "" {
			detail = lastLines(*result.Stderr, 20)
		} else if result.Stdout != nil && *result.Stdout != "" {
			detail = lastLines(*result.Stdout, 20)
		}
		if detail != "" {
			return fmt.Errorf("remote command exited with status %d:\n%s", *result.StatusCode, detail)
		}
		return fmt.Errorf("remote command exited with status %d (no output captured)", *result.StatusCode)
	}
	return nil
}

func execOnVMSilent(apiKey string, vmID string, command string) error {
	body, _ := json.Marshal(map[string]any{
		"command":   command,
		"timeoutMs": 300000,
	})

	resp, err := freestyleRequest(apiKey, "POST", fmt.Sprintf("/v1/vms/%s/exec-await", vmID), body)
	if err != nil {
		return err
	}

	var result struct {
		Stdout     *string `json:"stdout"`
		Stderr     *string `json:"stderr"`
		StatusCode *int    `json:"statusCode"`
	}
	if err := json.Unmarshal(resp, &result); err != nil {
		return fmt.Errorf("failed to parse exec response: %w", err)
	}

	if result.StatusCode != nil && *result.StatusCode != 0 {
		msg := ""
		if result.Stderr != nil {
			msg = *result.Stderr
		}
		if result.Stdout != nil && msg == "" {
			msg = *result.Stdout
		}
		return fmt.Errorf("command failed (exit %d): %s", *result.StatusCode, msg)
	}
	return nil
}

func deleteVM(apiKey string, vmID string) {
	_, _ = freestyleRequest(apiKey, "DELETE", fmt.Sprintf("/v1/vms/%s", vmID), nil)
}

func getVMState(apiKey string, vmID string) (string, error) {
	resp, err := freestyleRequest(apiKey, "GET", fmt.Sprintf("/v1/vms/%s", vmID), nil)
	if err != nil {
		return "", err
	}

	var result struct {
		State string `json:"state"`
	}
	if err := json.Unmarshal(resp, &result); err != nil {
		return "", err
	}
	return result.State, nil
}

func freestyleRequest(apiKey string, method string, path string, body []byte) ([]byte, error) {
	var reqBody io.Reader
	if body != nil {
		reqBody = bytes.NewReader(body)
	}

	req, err := http.NewRequest(method, freestyleAPIBase+path, reqBody)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", "Bearer "+apiKey)
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}

	client := &http.Client{Timeout: 30 * time.Minute}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("API request failed: %w", err)
	}
	if resp == nil || resp.Body == nil {
		return nil, fmt.Errorf("API returned nil response")
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to read response: %w", err)
	}

	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("API error %d: %s", resp.StatusCode, string(respBody))
	}

	return respBody, nil
}

// lastLines returns the last n non-empty lines of s.
func lastLines(s string, n int) string {
	lines := strings.Split(strings.TrimRight(s, "\n"), "\n")
	if len(lines) <= n {
		return strings.Join(lines, "\n")
	}
	return "...\n" + strings.Join(lines[len(lines)-n:], "\n")
}

// --- git helpers ---

func gitOutput(dir string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func gitRun(dir string, args ...string) error {
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	return cmd.Run()
}
