package checks

import (
	"fmt"
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
