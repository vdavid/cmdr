package checks

import (
	"fmt"
	"strings"
	"testing"
)

// shardInstanceID stamps the CMDR_INSTANCE_ID env var the binary picks up at launch. The
// format is load-bearing on two fronts: (1) instance-id.ts productName() expects exactly
// `e2e-<short>-<pid>` to reshape the Dock label into `Cmdr (E2E <short>)`, and (2) the
// macOS Keychain backend suffixes its SERVICE_NAME with the same string. Drift would
// quietly turn the Dock label into "Cmdr (e2e-mtp-12345)" (still works, ugly) and break a
// future `pgrep -f 'Cmdr (E2E '` cleanup script.
func TestShardInstanceIDFormat(t *testing.T) {
	t.Parallel()

	cases := []struct {
		short string
		pid   int
		want  string
	}{
		{"mtp", 12345, "e2e-mtp-12345"},
		{"nonmtp1", 99999, "e2e-nonmtp1-99999"},
		{"nonmtp2", 1, "e2e-nonmtp2-1"},
	}
	for _, tc := range cases {
		got := shardInstanceID(tc.short, tc.pid)
		if got != tc.want {
			t.Errorf("shardInstanceID(%q, %d) = %q, want %q", tc.short, tc.pid, got, tc.want)
		}
	}
}

// planShards must compose the per-shard CMDR_INSTANCE_ID alongside the data dir and ports,
// and the MTP lane is shard 0 (sequential). Pinning both invariants here catches a future
// refactor that drops the instance ID or reshuffles the MTP lane out of position 0 (the MTP
// shard MUST run alone because the run's MTP backing dir is shared across its instances).
func TestPlanShardsAssignsInstanceIDs(t *testing.T) {
	t.Parallel()

	const pid = 4242
	shards := planShards("", 1700000000, pid, []int{40001, 40002, 40003})

	if len(shards) != nonMtpShards+1 {
		t.Fatalf("planShards returned %d shards, want %d", len(shards), nonMtpShards+1)
	}
	if shards[0].kind != "mtp" {
		t.Errorf("shards[0].kind = %q, want %q", shards[0].kind, "mtp")
	}
	if shards[0].instanceID != "e2e-mtp-4242" {
		t.Errorf("shards[0].instanceID = %q, want %q", shards[0].instanceID, "e2e-mtp-4242")
	}

	// Non-MTP shards use the `nonmtp<i>` short name (no dash between "nonmtp" and the index)
	// to match the productName regex in instance-id.ts.
	for i := 1; i <= nonMtpShards; i++ {
		shard := shards[i]
		wantInstance := fmt.Sprintf("e2e-nonmtp%d-%d", i, pid)
		if shard.instanceID != wantInstance {
			t.Errorf("shards[%d].instanceID = %q, want %q", i, shard.instanceID, wantInstance)
		}
		if !strings.HasPrefix(shard.instanceID, "e2e-nonmtp") {
			t.Errorf("shards[%d].instanceID = %q, want e2e-nonmtp prefix", i, shard.instanceID)
		}
	}
}

// Two suites run at once whenever two worktrees are busy, which is most of the time.
// Everything a shard OWNS therefore has to be scoped to its run: a resource two runs
// share is one run reaching into the other's. That is not theoretical — a fixed MCP
// port once let a starting run SIGTERM a running one's app mid-test, and the 38
// failures that followed read exactly like a product bug.
//
// The two plans share a timestamp on purpose. Only the pid separates one run from
// another: two suites starting in the same second is a coin flip nobody should have to
// win, so a path whose only variable part is the clock counts as shared.
func TestPlanShardsSharesNothingBetweenConcurrentRuns(t *testing.T) {
	t.Parallel()

	const sameSecond = int64(1700000000)
	first := planShards("", sameSecond, 4242, []int{40001, 40002, 40003})
	second := planShards("", sameSecond, 4343, []int{40004, 40005, 40006})

	for i := range first {
		a, b := first[i], second[i]
		owned := map[string][2]string{
			"instanceID": {a.instanceID, b.instanceID},
			"socketPath": {a.socketPath, b.socketPath},
			"dataDir":    {a.dataDir, b.dataDir},
			"logFile":    {a.logFile, b.logFile},
			"outputDir":  {a.outputDir, b.outputDir},
			// The report is the run's evidence: the per-test log, the duration
			// allowlist, and the flake warning are all read back out of it. A shared
			// one means a run can be judged on another run's results.
			"jsonReport": {a.jsonReport, b.jsonReport},
			// Wiped and recreated at MTP-shard startup. Shared, a starting run
			// deletes the tree a running one's MTP specs are asserting against.
			"mtpFixtureRoot": {a.mtpFixtureRoot, b.mtpFixtureRoot},
		}
		for field, pair := range owned {
			if pair[0] == pair[1] {
				t.Errorf("shard %q: both runs got %s = %q; a concurrent run would clobber it", a.name, field, pair[0])
			}
		}
		if a.mcpPort == b.mcpPort {
			t.Errorf("shard %q: both runs got mcpPort = %d", a.name, a.mcpPort)
		}
	}
}

// Every shard of ONE run backs onto the same MTP root (the MTP shard owns it and the
// others are told to keep their hands off it), so run-scoping must not accidentally
// hand each shard its own.
func TestPlanShardsGivesOneRunOneMtpRoot(t *testing.T) {
	t.Parallel()

	shards := planShards("", 1700000000, 4242, []int{40001, 40002, 40003})
	for _, s := range shards[1:] {
		if s.mtpFixtureRoot != shards[0].mtpFixtureRoot {
			t.Errorf("shard %q backs onto %q, want the run's single root %q",
				s.name, s.mtpFixtureRoot, shards[0].mtpFixtureRoot)
		}
	}
	// mtp-fixtures.ts refuses to delete a root outside this prefix, so a value it
	// would reject leaves the MTP shard unable to reset between tests.
	if !strings.HasPrefix(shards[0].mtpFixtureRoot, "/tmp/cmdr-mtp-") {
		t.Errorf("mtpFixtureRoot = %q, want a /tmp/cmdr-mtp- prefix", shards[0].mtpFixtureRoot)
	}
}

// The ports have to be distinct WITHIN a run too, and the only way to get that from
// the OS is to hold every listener open while asking for the next one: close each in
// turn and the kernel is free to hand the same port back.
func TestReserveMcpPortsAreDistinctAndUsable(t *testing.T) {
	t.Parallel()

	const count = 3
	ports, err := reserveMcpPorts(count)
	if err != nil {
		t.Fatalf("reserveMcpPorts(%d) failed: %v", count, err)
	}
	if len(ports) != count {
		t.Fatalf("reserveMcpPorts(%d) returned %d ports", count, len(ports))
	}
	seen := map[int]bool{}
	for _, p := range ports {
		if p <= 0 {
			t.Errorf("port %d isn't usable", p)
		}
		if seen[p] {
			t.Errorf("port %d handed out twice", p)
		}
		seen[p] = true
	}
}

func TestExtractE2ETestOutput_PreTestSetupFailure(t *testing.T) {
	// Captured output when SMB containers came up but the test runner never
	// reached `Starting Tauri app...` (silent setup failure exiting 2). No
	// Tauri marker, no failure block, no Playwright tally: pre-test hint
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
Use './apps/desktop/test/smb-servers/stop.sh' to stop all containers.
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

func TestExtractE2ETestOutput_PreservesSMBPreFlightBanner(t *testing.T) {
	// SMB pre-flight banner lives in §1 (before "Starting Tauri app..."),
	// which the filter trims. The extractor must preserve it explicitly.
	input := `[INFO] Starting SMB containers (e2e)...
[INFO] SMB e2e stack ready: all 4 containers accepting TCP on :445
[INFO] Running E2E tests in Docker...
Starting Tauri app...
   ✘ test/example.spec.ts:1:1 › fails

  1) [tauri] › test/example.spec.ts:1:1 › fails

     Error: boom

  1 failed
[ELIFECYCLE] Command failed with exit code 1.
`
	out := extractE2ETestOutput(input)
	if !strings.Contains(out, "[SMB] SMB e2e stack ready: all 4 containers accepting TCP on :445") {
		t.Errorf("expected pre-flight banner preserved with [SMB] prefix, got:\n%s", out)
	}
	if !strings.HasPrefix(out, "[SMB]") {
		t.Errorf("expected output to start with the SMB banner, got:\n%s", out)
	}
	if !strings.Contains(out, "Error: boom") {
		t.Errorf("test failure body must still be present, got:\n%s", out)
	}
}

func TestExtractE2ETestOutput_PreservesBothPreAndPostFlightBanners(t *testing.T) {
	// Post-flight runs after the test phase exits, so a healthy run emits
	// both banners. Both must surface in the output.
	input := `[INFO] SMB e2e stack ready: all 4 containers accepting TCP on :445
Starting Tauri app...
   ✘ test/foo.spec.ts:1:1 › x

  1) [tauri] › test/foo.spec.ts:1:1 › x

     Error: oops

  1 failed
[WARN] SMB post-flight: at least one container is no longer accepting TCP, likely died mid-run
[ELIFECYCLE] Command failed with exit code 1.
`
	out := extractE2ETestOutput(input)
	if !strings.Contains(out, "[SMB] SMB e2e stack ready: all 4 containers") {
		t.Errorf("expected pre-flight banner preserved, got:\n%s", out)
	}
	if !strings.Contains(out, "[SMB] SMB post-flight: at least one container") {
		t.Errorf("expected post-flight banner preserved, got:\n%s", out)
	}
}

func TestExtractE2ETestOutput_NoSMBBannerForMacOSRuns(t *testing.T) {
	// On macOS the desktop-e2e-playwright check doesn't emit SMB banners
	// (no SMB containers involved). Filter must not add stray [SMB] lines.
	input := `Starting Tauri app...
   ✘ test/bar.spec.ts:1:1 › y

  1) [tauri] › test/bar.spec.ts:1:1 › y

     Error: nope

  1 failed
[ELIFECYCLE] Command failed with exit code 1.
`
	out := extractE2ETestOutput(input)
	if strings.Contains(out, "[SMB]") {
		t.Errorf("did not expect [SMB] banner on macOS-style run, got:\n%s", out)
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
