package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunGroqSmoke runs the real-API Groq smoke test (`ai::client_real_groq_test`), which exercises
// our `AiBackend::remote` + `chat_completion` path against a live OpenAI-compatible endpoint.
//
// It SELF-SKIPS when no `GROQ_API_KEY` is available, so it never breaks a run for contributors
// without a key, or CI before the secret is added. Key resolution (env var, then the `secret`
// sops helper) lives in `ResolveDevSecret`.
func RunGroqSmoke(ctx *CheckContext) (CheckResult, error) {
	key := ResolveDevSecret("GROQ_API_KEY")
	if key == "" {
		return Skipped("GROQ_API_KEY not set (env or sops)"), nil
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
