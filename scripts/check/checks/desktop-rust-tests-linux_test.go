package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestTrimRustTestProgress_CargoTestFormat(t *testing.T) {
	input := `running 4 tests
test foo::bar ... ok
test foo::baz ... ignored
test foo::qux ... ignored, real API call (set ANTHROPIC_API_KEY to run)
test foo::doom ... FAILED

failures:

---- foo::doom stdout ----
thread 'foo::doom' (17241) panicked at apps/desktop/src-tauri/src/foo.rs:42:
assertion failed: 1 == 2
note: run with ` + "`RUST_BACKTRACE=1`" + ` environment variable

failures:
    foo::doom

test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

error: test failed, to rerun pass ` + "`--lib`" + `
`
	out := trimRustTestProgress(input)

	if strings.Contains(out, "test foo::bar ... ok") {
		t.Errorf("expected ok line to be dropped, got:\n%s", out)
	}
	if strings.Contains(out, "test foo::baz ... ignored") && !strings.Contains(out, "test foo::doom") {
		// the bare-ignored line should be gone, but doom (FAILED) should remain
		t.Errorf("expected ignored line to be dropped, got:\n%s", out)
	}
	if strings.Contains(out, "test foo::qux ... ignored, real") {
		t.Errorf("expected ignored-with-reason line to be dropped, got:\n%s", out)
	}
	for _, want := range []string{
		"running 4 tests",
		"test foo::doom ... FAILED",
		"failures:",
		"---- foo::doom stdout ----",
		"thread 'foo::doom'",
		"assertion failed: 1 == 2",
		"test result: FAILED. 3 passed; 1 failed",
		"error: test failed",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected output to contain %q, got:\n%s", want, out)
		}
	}
}

func TestTrimRustTestProgress_NextestFormat(t *testing.T) {
	input := `------------
 Nextest run ID 8a3f with nextest profile: default
    Starting 4 tests across 1 binary
        PASS [   0.001s] cmdr_lib foo::bar
        SKIP [   0.000s] cmdr_lib foo::baz (reason: opt-in)
        PASS [   0.002s] cmdr_lib foo::qux
        FAIL [   0.001s] cmdr_lib foo::doom

--- STDOUT:              cmdr_lib foo::doom ---

thread 'foo::doom' panicked at src/foo.rs:42:
assertion failed: 1 == 2

--- STDERR:              cmdr_lib foo::doom ---


Summary [   0.005s] 4 tests run: 3 passed, 1 failed, 0 skipped
        FAIL [   0.001s] cmdr_lib foo::doom

error: test run failed
`
	out := trimRustTestProgress(input)

	for _, drop := range []string{
		"PASS [   0.001s] cmdr_lib foo::bar",
		"SKIP [   0.000s] cmdr_lib foo::baz",
		"PASS [   0.002s] cmdr_lib foo::qux",
	} {
		if strings.Contains(out, drop) {
			t.Errorf("expected output to NOT contain %q, got:\n%s", drop, out)
		}
	}
	for _, want := range []string{
		"FAIL [   0.001s] cmdr_lib foo::doom",
		"--- STDOUT:              cmdr_lib foo::doom ---",
		"thread 'foo::doom'",
		"Summary [   0.005s] 4 tests run: 3 passed, 1 failed",
		"error: test run failed",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected output to contain %q, got:\n%s", want, out)
		}
	}
}

// Verbatim from a real 4 802-test container run (2026-08-02). nextest right-aligns the
// progress index to the total's width, so tests 1-999 carry an internally padded counter.
// Reading the counter as one non-space token let exactly those lines survive the filter.
func TestTrimRustTestProgress_PaddedProgressCounter(t *testing.T) {
	input := `                    PASS [   0.094s] (   1/4802) cmdr accent_color_linux::tests::rgb_floats_clamps_out_of_range
                    PASS [   0.156s] (  91/4802) cmdr agent::llm::genai_impl::tests::ai_error_maps_to_typed_agent_error
                    PASS [   0.185s] ( 493/4802) cmdr agent::llm::genai_impl::tests::declared_tools_are_never_strict
                    PASS [   0.211s] (4256/4802) cmdr-fs sqlite_util::tests::a_passing_one
                    SKIP [   0.000s] (  12/4802) cmdr_lib foo::baz (reason: opt-in)
             TERMINATING [>  8.000s] (─────────) cmdr-fs sqlite_util::tests::cached_pages_come_from_the_shared_slab
                 TIMEOUT [   8.025s] (4256/4802) cmdr-fs sqlite_util::tests::cached_pages_come_from_the_shared_slab
                    FAIL [   0.004s] (4785/4802) index-query::bin/index-query broken::one
`
	out := trimRustTestProgress(input)

	if strings.Contains(out, "PASS [") || strings.Contains(out, "SKIP [") {
		t.Errorf("every PASS/SKIP line must be dropped whatever its counter padding, got:\n%s", out)
	}
	for _, want := range []string{"TERMINATING", "TIMEOUT [   8.025s]", "FAIL [   0.004s]"} {
		if !strings.Contains(out, want) {
			t.Errorf("expected %q to survive, got:\n%s", want, out)
		}
	}
}

// The same padding hit the classifier harder: an unmatched status line means the failure
// is never seen, so it gets no diagnosis and no contention re-run at all.
func TestClassifyRustFailures_PaddedProgressCounter(t *testing.T) {
	input := `                    FAIL [   0.007s] (  42/4802) cmdr_lib starved::one
                 TIMEOUT [   8.025s] ( 256/4802) cmdr-fs slow::one
                    LEAK [   0.100s] (   7/4802) cmdr_lib leaky::one
`
	failures := ClassifyRustFailures(input)
	if len(failures) != 3 {
		t.Fatalf("expected all three status lines to be classified, got %+v", failures)
	}
	byName := map[string]FailureClass{}
	for _, f := range failures {
		byName[f.Name] = f.Class
	}
	if byName["starved::one"] != ClassOther || byName["slow::one"] != ClassNextestCap || byName["leaky::one"] != ClassLeak {
		t.Errorf("classes are wrong: %+v", byName)
	}
}

func TestTrimRustTestProgress_PanicMessageWithTestPhrase(t *testing.T) {
	// A panic body that happens to contain a "test ... ok" substring on its
	// own line MUST be preserved. Anchoring to the start of the line is what
	// protects this case; only the cargo harness emits unindented `test `
	// lines.
	input := `test foo::bar ... FAILED

failures:

---- foo::bar stdout ----
thread 'foo::bar' panicked at:
  expected: "test foo::baz ... ok"
  actual:   "test foo::baz ... FAILED"

test result: FAILED. 0 passed; 1 failed
`
	out := trimRustTestProgress(input)

	for _, want := range []string{
		`expected: "test foo::baz ... ok"`,
		`actual:   "test foo::baz ... FAILED"`,
		"test foo::bar ... FAILED",
		"test result: FAILED",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected output to contain %q, got:\n%s", want, out)
		}
	}
}

func TestTrimRustTestProgress_EmptyAndUnrelatedInputUnchanged(t *testing.T) {
	cases := []string{
		"",
		"no test markers here\njust some build output\n",
		"error: linker `cc` not found\n",
	}
	for _, c := range cases {
		if got := trimRustTestProgress(c); got != c {
			t.Errorf("expected unchanged for %q, got %q", c, got)
		}
	}
}

func TestTrimBuildNoise_NoCompilingLinePassesThrough(t *testing.T) {
	// When provisioning fails before cargo runs, there's no `Compiling …`
	// line to anchor on. apt is silenced at source via -qq +
	// DEBIAN_FRONTEND=noninteractive in provisionScript, so the captured
	// output should already be clean (rustup info + the actual error).
	// trimBuildNoise must return it verbatim, no length-based truncation.
	input := `info: syncing channel updates for stable-aarch64-unknown-linux-gnu
info: latest update on 2026-04-16 for version 1.95.0 (59807616e 2026-04-14)
info: downloading 3 components
OrbStack ERROR: Dynamic loader not found: /lib64/ld-linux-x86-64.so.2
This usually means that you're running an x86 program on an arm64 OS without multi-arch libraries.
For more details and instructions, see https://orb.cx/multiarch
`
	if got := trimBuildNoise(input); got != input {
		t.Errorf("expected unchanged when no Compiling line is present, got:\n%s", got)
	}
}

func TestTrimBuildNoise_KeepsAptErrorLinesWhenNoCompiling(t *testing.T) {
	// apt's E:/W: lines are the failure path that survives `-qq`. They must
	// always pass through.
	input := `E: Unable to locate package nonexistent
W: Failed to fetch http://example.com/repo
`
	if got := trimBuildNoise(input); got != input {
		t.Errorf("expected apt error lines preserved, got:\n%s", got)
	}
}

func TestTrimBuildNoise_DoesNotTruncateLongOutput(t *testing.T) {
	// 200 distinct rustc error lines must ALL survive. No length-based
	// truncation may exist. Each line is unique so we can count survivors.
	var sb strings.Builder
	for i := range 200 {
		fmt.Fprintf(&sb, "error[E0308]: mismatched types in line %d\n", i)
	}
	input := sb.String()
	out := trimBuildNoise(input)
	for i := range 200 {
		needle := fmt.Sprintf("mismatched types in line %d", i)
		if !strings.Contains(out, needle) {
			t.Errorf("line %d was dropped/truncated, got len(out)=%d", i, len(out))
			break
		}
	}
}

func TestTrimBuildNoise_KeepsCompileErrorAfterAptSuccess(t *testing.T) {
	input := `Setting up libgtk-3-dev:arm64 (3.24.49-3) ...
Processing triggers for libc-bin (2.41-12+deb13u2) ...
   Compiling cmdr_lib v0.1.0
error[E0432]: unresolved import ` + "`crate::foo`" + `
  --> src/lib.rs:42:5
   |
42 | use crate::foo;
   |     ^^^^^ no ` + "`foo`" + ` in the crate root

error: could not compile ` + "`cmdr_lib`" + ` due to previous error
`
	out := trimBuildNoise(input)
	for _, want := range []string{
		"error[E0432]: unresolved import",
		"src/lib.rs:42:5",
		"use crate::foo;",
		"no `foo` in the crate root",
		"could not compile `cmdr_lib`",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected output to contain %q, got:\n%s", want, out)
		}
	}
	// apt chatter before Compiling should have been dropped by the
	// Compiling-anchor pass.
	if strings.Contains(out, "Setting up libgtk-3-dev") {
		t.Errorf("expected pre-Compiling apt line to be dropped, got:\n%s", out)
	}
}

func TestTrimBuildNoise_OutputWithoutNoiseIsUnchanged(t *testing.T) {
	input := `error: something went wrong
help: try doing X
`
	if got := trimBuildNoise(input); got != input {
		t.Errorf("expected unchanged, got %q", got)
	}
}

func TestTrimRustTestProgress_BenchAndLeakAreKept(t *testing.T) {
	// nextest LEAK/TIMEOUT/SLOW and bench results are signal, not noise.
	input := `        LEAK [   0.001s] cmdr_lib foo::leaky
        TIMEOUT [  60.001s] cmdr_lib foo::slow
        SLOW [>60.000s] cmdr_lib foo::sluggish
test bench::throughput ... bench:       1,234 ns/iter (+/- 56)
`
	out := trimRustTestProgress(input)
	if out != input {
		t.Errorf("expected LEAK/TIMEOUT/SLOW/bench lines to be kept, got:\n%s", out)
	}
}

// repoSelection resolves the real workspace's Linux package selection, skipping when the
// test runs somewhere the repo layout isn't visible.
func repoSelection(t *testing.T) []string {
	t.Helper()
	root, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatalf("failed to resolve repo root: %v", err)
	}
	if _, err := os.Stat(filepath.Join(root, "Cargo.toml")); err != nil {
		t.Skipf("repo layout not found from %s: %v", root, err)
	}
	selection, err := linuxSelectionArgs(root)
	if err != nil {
		t.Fatalf("linuxSelectionArgs: %v", err)
	}
	return selection
}

// TestTheContainerRunSelectsForTheContainerNotTheHost pins the trap that makes this
// lane different from every other cargo lane: the check process runs on a Mac while
// the cargo command runs inside a Linux container. Computing the selection from the
// host's OS leaves `cmdr-fsevent-stream` in it, where it dies at `cargo check` with
// `E0455: link kind 'framework' is only supported on Apple targets`.
func TestTheContainerRunSelectsForTheContainerNotTheHost(t *testing.T) {
	selection := repoSelection(t)
	args := append([]string{"--locked"}, selection...)
	script := containerNextestScript(append(args, "--no-fail-fast")...)

	for _, want := range []string{"'--workspace'", "'--exclude' 'cmdr-fsevent-stream'", "'--locked'"} {
		if !strings.Contains(script, want) {
			t.Errorf("expected the container run command to contain %q, got:\n%s", want, script)
		}
	}
}

// The contention re-run MUST carry the same package selection as the failing run. A
// re-run that selected differently could find no tests at all, which reads as
// "everything passed alone" and turns every real Linux failure into a warn.
func TestTheContentionRerunKeepsTheFailingRunsSelection(t *testing.T) {
	selection := repoSelection(t)
	args := containerRerunArgs(ContentionProbeProfile, selection, []string{"a::b"})

	joined := strings.Join(args, " ")
	for _, want := range []string{"--locked", "--profile " + ContentionProbeProfile, strings.Join(selection, " ")} {
		if !strings.Contains(joined, want) {
			t.Errorf("expected re-run args to contain %q, got: %v", want, args)
		}
	}
	if !strings.Contains(joined, "-E test(=a::b)") {
		t.Errorf("expected the re-run to be filtered to the failing test, got: %v", args)
	}
}

// The filter expression carries spaces and parens, so it has to survive `sh -c` intact.
// Unquoted, `sh` splits it and nextest either errors or (worse) selects something else.
func TestTheContainerScriptQuotesTheFilterExpression(t *testing.T) {
	args := containerRerunArgs(ContentionRetryProfile, []string{"--workspace"}, []string{"a::b", "c::d"})
	script := containerNextestScript(args...)

	if !strings.Contains(script, `'test(=a::b) + test(=c::d)'`) {
		t.Errorf("the filter expression must reach nextest as one quoted argument, got:\n%s", script)
	}
	if !strings.Contains(script, "export PATH=/usr/local/go/bin:$PATH") {
		t.Errorf("the re-run needs Go on PATH for build.rs, got:\n%s", script)
	}
}

func TestShellQuoteEscapesEmbeddedQuotes(t *testing.T) {
	if got, want := shellQuote(`it's`), `'it'\''s'`; got != want {
		t.Errorf("shellQuote = %q, want %q", got, want)
	}
}

// Provisioning stops before the tests run: the test run and the contention re-run are
// separate execs into the SAME container, which is what makes re-running a failure alone
// cost seconds instead of a fresh provision plus a full workspace rebuild.
func TestProvisionScriptStopsBeforeRunningTests(t *testing.T) {
	script, err := buildProvisionScript(repoRootForTest(t))
	if err != nil {
		t.Fatalf("buildProvisionScript: %v", err)
	}
	if strings.Contains(script, "cargo nextest run") {
		t.Errorf("provisioning must not run the suite; that's a separate exec:\n%s", script)
	}
	// The Go tarball assertion lives in TestLinuxContainerProvisionsTheMisePinnedGo,
	// which owns the "container Go == .mise.toml" invariant.
	if !strings.Contains(script, "get.nexte.st/"+containerNextestVersion+"/") {
		t.Errorf("the container's nextest must be pinned to %s, got:\n%s", containerNextestVersion, script)
	}
	if strings.Contains(script, "get.nexte.st/latest") {
		t.Errorf("an unpinned nextest can classify contention differently from the host lanes:\n%s", script)
	}
}

func TestParseContainerLoadPerCore(t *testing.T) {
	cases := []struct {
		name string
		out  string
		want float64
	}{
		{"busy container", "24.00 18.00 12.00 9/812 1234\n8\n", 3},
		{"quiet container", "0.40 0.50 0.60 1/200 99\n8\n", 0.05},
		{"unreadable load reads as quiet", "nope\n8\n", 0},
		{"missing nproc reads as quiet", "24.00 18.00 12.00 9/812 1234\n", 0},
		{"zero cores reads as quiet", "24.00 18.00 12.00 9/812 1234\n0\n", 0},
		{"empty reads as quiet", "", 0},
	}
	for _, c := range cases {
		if got := parseContainerLoadPerCore(c.out); got != c.want {
			t.Errorf("%s: parseContainerLoadPerCore = %v, want %v", c.name, got, c.want)
		}
	}
}
