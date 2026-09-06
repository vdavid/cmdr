package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// smokeStatusFileEnv names the file a smoke test writes an "inconclusive" report into. Its
// Rust counterpart is `smoke_providers::STATUS_FILE_ENV`, which explains why the report
// travels through a file rather than the console: nextest discards a passing test's stdout.
const smokeStatusFileEnv = "CMDR_SMOKE_STATUS_FILE"

// providerSmoke describes one real-API smoke lane: which Rust test module to run, which
// secret feeds it, and how to name the provider in the check's own output.
//
// The model ids themselves live in `apps/desktop/src-tauri/src/ai/smoke_providers.rs` — the
// one place to edit when a provider decommissions one. Nothing here needs to know them.
type providerSmoke struct {
	// Provider name for the SKIPPED / passed line.
	name string
	// Env var the test reads, resolved from env or the sops `secret` helper.
	envVar string
	// Rust test-module filter passed to nextest.
	testModule string
}

// RunGroqSmoke runs Groq's `#[ignore]`-gated real-API smoke, the cheapest of the family.
//
// Every lane here SELF-SKIPS when its key isn't available, so none breaks a run for a
// contributor without a key, or for CI before the secret is added. Key resolution (env var,
// then the `secret` sops helper) lives in `ResolveDevSecret`.
//
// ⚠️ A skip is silent success, which is how the Groq lane sat green in CI for months without
// ever calling Groq: the workflow step existed, the repo secret didn't. Adding a lane here is
// half the job; adding its secret to the repo is the other half.
func RunGroqSmoke(ctx *CheckContext) (CheckResult, error) {
	return runProviderSmoke(ctx, providerSmoke{
		name:       "Groq",
		envVar:     "GROQ_API_KEY",
		testModule: "ai::client_real_groq_test",
	})
}

// RunFireworksSmoke runs the second OpenAI-compatible host, whose account-path model ids
// exercise a name shape Groq's never will.
func RunFireworksSmoke(ctx *CheckContext) (CheckResult, error) {
	return runProviderSmoke(ctx, providerSmoke{
		name:       "Fireworks AI",
		envVar:     "FIREWORKS_AI_API_KEY",
		testModule: "ai::client_real_fireworks_test",
	})
}

// RunAnthropicSmoke covers the one non-OpenAI wire format we ship: Anthropic's native
// streaming protocol.
func RunAnthropicSmoke(ctx *CheckContext) (CheckResult, error) {
	return runProviderSmoke(ctx, providerSmoke{
		name:       "Anthropic",
		envVar:     "ANTHROPIC_API_KEY",
		testModule: "ai::client_real_anthropic_test",
	})
}

// RunOpenAiSmoke covers all three OpenAI legs: chat-completions, the Responses API, and the
// chat-completions reasoning models that reject a custom temperature.
func RunOpenAiSmoke(ctx *CheckContext) (CheckResult, error) {
	return runProviderSmoke(ctx, providerSmoke{
		name:       "OpenAI",
		envVar:     "OPENAI_API_KEY",
		testModule: "ai::client_real_openai_test",
	})
}

// RunGeminiSmoke covers the SECOND native wire format we ship, and the only provider whose
// free tier flaps hard enough to need a third outcome — see `inconclusive` below.
func RunGeminiSmoke(ctx *CheckContext) (CheckResult, error) {
	return runProviderSmoke(ctx, providerSmoke{
		name:       "Google Gemini",
		envVar:     "GEMINI_API_KEY",
		testModule: "ai::client_real_gemini_test",
	})
}

func runProviderSmoke(ctx *CheckContext, provider providerSmoke) (CheckResult, error) {
	key := ResolveDevSecret(provider.envVar)
	if key == "" {
		return Skipped(provider.envVar + " not set (env or sops)"), nil
	}

	// A handful of named tests in the app crate, but selected the way every other lane
	// selects: a `cmd.Dir`-scoped run asks cargo about one package, which resolves
	// dependency features differently from `--workspace` and rebuilds the four
	// first-party crates to answer it (measured at 100 s on a warm tree). The
	// positional filter is what narrows the run to one module.
	laneArgs, err := HostCargoLaneArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	if err := EnsureCargoNextest(); err != nil {
		return CheckResult{}, err
	}

	// Where the tests report "I never got a verdict" (see `inconclusive` below). Handed to
	// every provider, so any lane can start reporting one without new plumbing.
	statusDir, err := os.MkdirTemp("", "cmdr-smoke-status")
	if err != nil {
		return CheckResult{}, err
	}
	defer func() { _ = os.RemoveAll(statusDir) }()
	statusFile := filepath.Join(statusDir, "inconclusive.txt")

	args := append([]string{"nextest", "run", "--locked", "--lib", "--run-ignored", "only"}, laneArgs...)
	cmd := exec.Command("cargo", append(args, provider.testModule)...)
	cmd.Dir = ctx.RootDir
	cmd.Env = append(os.Environ(), provider.envVar+"="+key, smokeStatusFileEnv+"="+statusFile)
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("the %s smoke test failed\n%s", provider.name, indentOutput(output))
	}
	if reasons := inconclusive(statusFile); reasons != "" {
		return CheckResult{
			Code: ResultWarning,
			Message: fmt.Sprintf(
				"%s couldn't be reached well enough to prove anything (NOT a pass, NOT a stale model pin)\n%s",
				provider.name, indentOutput(reasons)),
			Total: -1, Issues: -1, Changes: -1,
		}, nil
	}
	return Success(provider.name + " real-API smoke passed"), nil
}

// inconclusive reads back what the tests wrote to the status file, or "" when they wrote
// nothing.
//
// ⚠️ This is the THIRD outcome, and it deliberately isn't a pass or a failure. A provider
// that flaps (Gemini's free tier answers 200, 503, and a bodyless 404 to the same request
// within minutes) would otherwise force a choice between a red lane nobody trusts and a green
// one that proves nothing — the second being exactly how the Groq lane sat green for months.
// A warn prints in yellow even in quiet mode and can't be mistaken for coverage.
//
// A missing file is the normal case: a lane only writes here when it gives up. Identical
// lines collapse: every test in a module gives up for the same reason, and repeating it once
// per test buries the one sentence that matters.
func inconclusive(statusFile string) string {
	contents, err := os.ReadFile(statusFile)
	if err != nil {
		return ""
	}
	seen := map[string]bool{}
	var unique []string
	for _, line := range strings.Split(string(contents), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || seen[line] {
			continue
		}
		seen[line] = true
		unique = append(unique, line)
	}
	return strings.Join(unique, "\n")
}
