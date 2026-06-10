package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunGroqSmoke runs the real-API Groq smoke test (`ai::client_real_groq_test`), which exercises
// our `AiBackend::remote` + `chat_completion` path against a live OpenAI-compatible endpoint.
//
// It SELF-SKIPS when no `GROQ_API_KEY` is available, so it never breaks a run for contributors
// without a key, or CI before the secret is added. Key resolution order:
//  1. `GROQ_API_KEY` env var (how CI passes the GitHub secret).
//  2. macOS Keychain (`security find-generic-password -s GROQ_API_KEY`), so a local `pnpm check`
//     picks up David's key without him having to export it.
func RunGroqSmoke(ctx *CheckContext) (CheckResult, error) {
	key := resolveGroqAPIKey()
	if key == "" {
		return Skipped("GROQ_API_KEY not set (env or Keychain)"), nil
	}

	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

	if !CommandExists("cargo-nextest") {
		installCmd := exec.Command("cargo", "install", "cargo-nextest", "--version", "0.9.136", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-nextest: %w", err)
		}
	}

	cmd := exec.Command("cargo", "nextest", "run", "--locked", "--lib", "--run-ignored", "only",
		"ai::client_real_groq_test")
	cmd.Dir = rustDir
	cmd.Env = append(os.Environ(), "GROQ_API_KEY="+key)
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("the Groq smoke test failed\n%s", indentOutput(output))
	}
	return Success("Groq translate-pipeline smoke passed"), nil
}

// resolveGroqAPIKey returns the Groq key from the env var, falling back to the macOS Keychain.
// Returns "" when neither is available (the caller then skips).
func resolveGroqAPIKey() string {
	if key := strings.TrimSpace(os.Getenv("GROQ_API_KEY")); key != "" {
		return key
	}
	if !CommandExists("security") {
		return ""
	}
	cmd := exec.Command("security", "find-generic-password", "-a", os.Getenv("USER"), "-s", "GROQ_API_KEY", "-w")
	out, err := RunCommand(cmd, true)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(out)
}
