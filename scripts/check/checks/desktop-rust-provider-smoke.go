package checks

import (
	"fmt"
	"os"
	"os/exec"
)

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

	args := append([]string{"nextest", "run", "--locked", "--lib", "--run-ignored", "only"}, laneArgs...)
	cmd := exec.Command("cargo", append(args, provider.testModule)...)
	cmd.Dir = ctx.RootDir
	cmd.Env = append(os.Environ(), provider.envVar+"="+key)
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("the %s smoke test failed\n%s", provider.name, indentOutput(output))
	}
	return Success(provider.name + " real-API smoke passed"), nil
}
