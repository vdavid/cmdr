package checks

import (
	"strings"
	"testing"
)

func TestExtractE2ETestOutput_PreTestSetupFailure(t *testing.T) {
	// Captured output when SMB containers came up but the test runner never
	// reached `Starting Tauri app...` (silent setup failure exiting 2). No
	// Tauri marker, no failure block, no Playwright tally — pre-test hint
	// must fire.
	input := `Run npm run preview to preview your production build locally.
> Using @sveltejs/adapter-static
  Wrote site to "build"
  ✔ done
[INFO] Using Linux target: aarch64-unknown-linux-gnu
[INFO] Starting SMB containers (e2e)...
Starting E2E SMB servers (guest, auth, 50shares, unicode)...
Waiting for containers to be healthy...
NAME                                   IMAGE                                COMMAND                  SERVICE                 CREATED         STATUS                   PORTS
smb-consumer-smb-consumer-50shares-1   smb-consumer-smb-consumer-50shares   "smbd --foreground -…"   smb-consumer-50shares   4 seconds ago   Up 3 seconds (healthy)   0.0.0.0:10483->445/tcp, [::]:10483->445/tcp
smb-consumer-smb-consumer-auth-1       smb-consumer-smb-consumer-auth       "smbd --foreground -…"   smb-consumer-auth       4 seconds ago   Up 3 seconds (healthy)   0.0.0.0:10481->445/tcp, [::]:10481->445/tcp
SMB servers ready! Connection URLs:
  smb://localhost:10480/public    # smb-consumer-guest (no auth)
Use './stop.sh' to stop all containers.
[ELIFECYCLE] Command failed with exit code 2.
post-elifecycle noise that should be dropped
`
	out := extractE2ETestOutput(input)

	if !strings.HasPrefix(out, "note: tests did not reach the run phase") {
		t.Errorf("expected pre-test hint prefix, got:\n%s", out)
	}
	for _, drop := range []string{
		"NAME                                   IMAGE",
		"smb-consumer-smb-consumer-50shares-1",
		"smb-consumer-smb-consumer-auth-1",
		"post-elifecycle noise",
	} {
		if strings.Contains(out, drop) {
			t.Errorf("expected output to NOT contain %q, got:\n%s", drop, out)
		}
	}
	for _, want := range []string{
		"Starting E2E SMB servers",
		"Waiting for containers to be healthy",
		"SMB servers ready!",
		"[ELIFECYCLE] Command failed with exit code 2.",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected output to contain %q, got:\n%s", want, out)
		}
	}
}

func TestExtractE2ETestOutput_PlaywrightTallyOnlySuppressesPreTestHint(t *testing.T) {
	// The real-world false positive: a macOS playwright shard run that DID
	// reach the test phase (failure blocks + tally present) but produced no
	// `Starting Tauri app...` marker because Tauri's stdout is routed to a
	// log file by the Go check, not Playwright. The pre-test hint must NOT
	// fire here.
	input := `   ✘  42 [tauri] › test/e2e-playwright/conflict-copy.spec.ts:153:3 › Per-file conflict decisions (Layout A) › Copy with mixed per-file conflict decisions (16.0s)

  1) [tauri] › test/e2e-playwright/conflict-copy.spec.ts:153:3 › Per-file conflict decisions (Layout A) › Copy with mixed per-file conflict decisions

    Test timeout of 8000ms exceeded.
    Error: expect(received).toBe(expected) // Object.is equality

  1 failed
    [tauri] › test/e2e-playwright/conflict-copy.spec.ts:153:3 › Per-file conflict decisions (Layout A) › Copy with mixed per-file conflict decisions
  1 skipped
  65 passed (1.2m)
[ELIFECYCLE] Command failed with exit code 1.
`
	out := extractE2ETestOutput(input)
	if strings.HasPrefix(out, "note:") {
		t.Errorf("did not expect any pre-test hint (run reached test phase), got:\n%s", out)
	}
	for _, want := range []string{
		"✘  42 [tauri]",
		"1) [tauri]",
		"Test timeout of 8000ms exceeded",
		"1 failed",
		"65 passed",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected output to contain %q, got:\n%s", want, out)
		}
	}
}

func TestExtractE2ETestOutput_TallyAlonePreventsHint(t *testing.T) {
	// Sanity: an all-passed run (tally but no failure block) also must not
	// trigger the pre-test hint.
	input := `[INFO] Setting up...
   42 passed (1.2m)
[ELIFECYCLE] Command failed with exit code 0.
`
	out := extractE2ETestOutput(input)
	if strings.HasPrefix(out, "note:") {
		t.Errorf("did not expect any pre-test hint (tally present), got:\n%s", out)
	}
}

func TestExtractE2ETestOutput_TauriStartedKeepsExistingBehavior(t *testing.T) {
	input := `noise before
[INFO] Starting SMB containers (e2e)...
Starting Tauri app...
   ✘ test/example.spec.ts:2:1 › fails

  1) [tauri] › test/example.spec.ts:2:1 › fails

     Error: assertion failed

  1 failed
[ELIFECYCLE] Command failed with exit code 1.
post-elifecycle dump that must be dropped
`
	out := extractE2ETestOutput(input)

	if strings.HasPrefix(out, "note: Tauri app never started") {
		t.Errorf("did not expect pre-test hint when Tauri started, got:\n%s", out)
	}
	for _, drop := range []string{
		"noise before",
		"Starting SMB containers",
		"post-elifecycle dump",
	} {
		if strings.Contains(out, drop) {
			t.Errorf("expected output to NOT contain %q, got:\n%s", drop, out)
		}
	}
	for _, want := range []string{
		"✘ test/example.spec.ts:2:1",
		"1) [tauri] › test/example.spec.ts:2:1",
		"Error: assertion failed",
		"1 failed",
		"[ELIFECYCLE]",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("expected output to contain %q, got:\n%s", want, out)
		}
	}
}

func TestDropDockerComposePsTable(t *testing.T) {
	input := []string{
		"some progress line",
		"NAME                IMAGE       COMMAND   SERVICE   CREATED   STATUS   PORTS",
		`container-a   image-a   "cmd"   svc-a   1 second ago   Up 2 seconds (healthy)   80/tcp`,
		`container-b   image-b   "cmd"   svc-b   1 second ago   Up 2 minutes   80/tcp`,
		`container-c   image-c   "cmd"   svc-c   1 second ago   Up 5 seconds (unhealthy)   80/tcp`,
		`container-d   image-d   "cmd"   svc-d   1 second ago   Up 1 second (starting)   80/tcp`,
		"unrelated trailing line",
	}
	out := dropDockerComposePsTable(input)
	want := []string{
		"some progress line",
		"unrelated trailing line",
	}
	if strings.Join(out, "|") != strings.Join(want, "|") {
		t.Errorf("expected\n%v\ngot\n%v", want, out)
	}
}

func TestDropDockerComposePsTable_DoesNotEatProseWithUpDigits(t *testing.T) {
	// Benign sentences containing "Up <N>" must survive when no preceding
	// `NAME IMAGE COMMAND` header has anchored a table block.
	input := []string{
		"Up 3 servers are configured.",
		"It took Up 10 seconds total.",
		`unrelated   line   "with"   Up 2 seconds (healthy)   80/tcp`,
		"more prose",
	}
	out := dropDockerComposePsTable(input)
	if strings.Join(out, "|") != strings.Join(input, "|") {
		t.Errorf("expected all lines preserved (no header anchor), got:\n%v", out)
	}
}

func TestDropDockerComposePsTable_EndsOnBlankOrNonRowLine(t *testing.T) {
	// After the table ends, normal lines must resume being kept.
	input := []string{
		"before",
		"NAME       IMAGE       COMMAND   SERVICE   CREATED   STATUS   PORTS",
		`a   img-a   "cmd"   svc-a   1 sec ago   Up 2 seconds (healthy)   80/tcp`,
		`b   img-b   "cmd"   svc-b   1 sec ago   Up 3 seconds (healthy)   81/tcp`,
		"",
		"after the table",
		"and more after",
	}
	out := dropDockerComposePsTable(input)
	want := []string{"before", "", "after the table", "and more after"}
	if strings.Join(out, "|") != strings.Join(want, "|") {
		t.Errorf("expected\n%v\ngot\n%v", want, out)
	}
}
