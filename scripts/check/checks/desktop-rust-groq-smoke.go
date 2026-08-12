package checks

import (
	"fmt"
	"os"
	"os/exec"
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

	// One named test in the app crate, but selected the way every other lane
	// selects: a `cmd.Dir`-scoped run asks cargo about one package, which resolves
	// dependency features differently from `--workspace` and rebuilds the four
	// first-party crates to answer it (measured at 100 s on a warm tree). The
	// positional filter is what narrows the run to one test.
	laneArgs, err := HostCargoLaneArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	if !CommandExists("cargo-nextest") {
		installCmd := exec.Command("cargo", "install", "cargo-nextest", "--version", "0.9.136", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-nextest: %w", err)
		}
	}

	args := append([]string{"nextest", "run", "--locked", "--lib", "--run-ignored", "only"}, laneArgs...)
	cmd := exec.Command("cargo", append(args, "ai::client_real_groq_test")...)
	cmd.Dir = ctx.RootDir
	cmd.Env = append(os.Environ(), "GROQ_API_KEY="+key)
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("the Groq smoke test failed\n%s", indentOutput(output))
	}
	return Success("Groq translate-pipeline smoke passed"), nil
}
