package checks

import (
	"strings"
	"testing"
	"time"
)

const timedRun = `────────────
 Nextest run ID 8a3f with nextest profile: default
    Starting 3 tests across 2 binaries (1 test skipped)
        PASS [   0.012s] (1/3) cmdr downloads::latest_ring::tests::ring_wraps
        PASS [  14.250s] (2/3) cmdr file_viewer::session_test::test_session_close_stops_watcher
        PASS [   3.400s] (3/3) cmdr-archive watch::host_seam_test::a_rewrite_asks_the_host_to_refresh_the_archive_listings
────────────
     Summary [ 400.200s] 3 tests run: 3 passed, 1 skipped
`

func TestRustTimingProfileSplitsBuildFromRunAndRanksTheSlowest(t *testing.T) {
	profile := RustTimingProfile(timedRun, 664*time.Second, ParseNextestResults(timedRun), 2)

	lines := strings.Split(profile, "\n")
	want := []string{
		"Timing: 11m4s in all: 4m24s building and listing, 6m40s running tests",
		"Tests summed: 0m18s over 3 tests",
		"Slowest 2 tests:",
		"  14.25 s  cmdr::file_viewer::session_test::test_session_close_stops_watcher",
		"   3.40 s  cmdr-archive::watch::host_seam_test::a_rewrite_asks_the_host_to_refresh_the_archive_listings",
	}
	if len(lines) != len(want) {
		t.Fatalf("got %d lines, want %d:\n%s", len(lines), len(want), profile)
	}
	for i := range want {
		if lines[i] != want[i] {
			t.Errorf("line %d:\n got %q\nwant %q", i, lines[i], want[i])
		}
	}
}

func TestRustTimingProfileWithoutASummaryStillRanksTheTests(t *testing.T) {
	output := strings.Replace(timedRun, "     Summary [ 400.200s] 3 tests run: 3 passed, 1 skipped\n", "", 1)
	profile := RustTimingProfile(output, 664*time.Second, ParseNextestResults(output), 1)

	if !strings.HasPrefix(profile, "Timing: 11m4s in all (nextest printed no summary, so no split)\n") {
		t.Errorf("unexpected header:\n%s", profile)
	}
	if !strings.Contains(profile, "test_session_close_stops_watcher") {
		t.Errorf("the slowest test is missing:\n%s", profile)
	}
}
