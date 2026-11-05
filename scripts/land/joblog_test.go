package main

import (
	"slices"
	"strings"
	"testing"
)

// GitHub's job log shape: every line timestamped, the tools' colors kept.
func jobLog(lines ...string) string {
	var b strings.Builder
	for _, line := range lines {
		b.WriteString("2026-11-05T01:52:10.1234567Z " + line + "\r\n")
	}
	return b.String()
}

func TestFailureSummary(t *testing.T) {
	cases := []struct {
		name string
		log  string
		want []string
	}{
		{
			// Shapes from run 41949695576's light lane (2026-11-05): a passing check, then a red
			// one whose Go test failure sits inside the runner's block, read once.
			name: "a check runner block, with its nested go test failure",
			log: jobLog(
				"##[group]Run pnpm check --ci",
				"• 📜 Scripts: 🐹 Go / go-vet       ... \x1b[32mOK\x1b[0m (3.1s)",
				"• 📜 Scripts: 🐹 Go / go-tests   ... \x1b[31mFAILED\x1b[0m (1m9s)",
				"      tests failed",
				"            --- FAIL: TestEveryOutOfCrateReadIsClassified (0.61s)",
				"                rust-test-scope_test.go:584: drag.rs reads .git/config",
				"",
				"            FAIL\tcmdr/scripts/check/checks\t14.251s",
				"",
				"• 📜 Scripts: 🐹 Go / staticcheck  ... \x1b[32mOK\x1b[0m (9.0s)",
				"##[error]Process completed with exit code 1.",
			),
			want: []string{
				"• 📜 Scripts: 🐹 Go / go-tests   ... FAILED (1m9s)",
				"      tests failed",
				"            --- FAIL: TestEveryOutOfCrateReadIsClassified (0.61s)",
				"                rust-test-scope_test.go:584: drag.rs reads .git/config",
				"",
				"            FAIL\tcmdr/scripts/check/checks\t14.251s",
			},
		},
		{
			name: "Playwright's numbered failures, up to its tally",
			log: jobLog(
				"Running 40 tests using 2 workers",
				"  ✓  1 [cmdr] › tests/copy.spec.ts:10:5 › copies a file (2.1s)",
				"  1) [cmdr] › tests/viewer.spec.ts:12:5 › opens the viewer ─────",
				"",
				"    Error: expect(locator).toBeVisible() failed",
				"      at tests/viewer.spec.ts:20:7",
				"",
				"  2) [cmdr] › tests/search.spec.ts:8:3 › finds a file ─────",
				"",
				"    Test timeout of 30000ms exceeded.",
				"",
				"  2 failed",
				"    [cmdr] › tests/viewer.spec.ts:12:5 › opens the viewer",
				"  38 passed (1.2m)",
			),
			want: []string{
				"  1) [cmdr] › tests/viewer.spec.ts:12:5 › opens the viewer ─────",
				"",
				"    Error: expect(locator).toBeVisible() failed",
				"      at tests/viewer.spec.ts:20:7",
				"  2) [cmdr] › tests/search.spec.ts:8:3 › finds a file ─────",
				"",
				"    Test timeout of 30000ms exceeded.",
			},
		},
		{
			name: "a bare go test failure, subtests nested",
			log: jobLog(
				"=== RUN   TestA",
				"--- FAIL: TestA (0.00s)",
				"    --- FAIL: TestA/one (0.00s)",
				"        a_test.go:9: got 1, want 2",
				"FAIL",
				"ok  \tother\t0.1s",
			),
			want: []string{
				"--- FAIL: TestA (0.00s)",
				"    --- FAIL: TestA/one (0.00s)",
				"        a_test.go:9: got 1, want 2",
			},
		},
		{
			name: "no failure block: GitHub's error annotations",
			log: jobLog(
				"##[group]Run actions/checkout@v5",
				"##[error]The runner has received a shutdown signal.",
				"##[error]Process completed with exit code 143.",
			),
			want: []string{
				"##[error]The runner has received a shutdown signal.",
				"##[error]Process completed with exit code 143.",
			},
		},
		{
			name: "nothing to say",
			log:  jobLog("all fine", "really"),
			want: nil,
		},
	}
	for _, c := range cases {
		if got := failureSummary(c.log); !slices.Equal(got, c.want) {
			t.Errorf("%s:\ngot  %q\nwant %q", c.name, got, c.want)
		}
	}
}

func TestCIFailurePrintsEachJobsSummaryUnderIt(t *testing.T) {
	run := workflowRun{ID: 7, Status: "in_progress"}
	jobs := []runJob{
		{ID: 1, Name: "Lint", Status: "completed", Conclusion: "failure", URL: "https://x/1"},
		{ID: 2, Name: "Tests", Status: "in_progress"},
	}
	msg := ciFailure(run, jobs, map[int64][]string{1: {"• lint... FAILED (1s)", "", "      bad.go:1: nope"}}, true, "pnpm land").Error()
	want := "  Lint: failure https://x/1\n      • lint... FAILED (1s)\n\n            bad.go:1: nope\n"
	if !strings.Contains(msg, want) {
		t.Errorf("the summary isn't under its job:\n%s", msg)
	}
	if !strings.Contains(msg, "gh run cancel 7 -R "+repo) {
		t.Errorf("an early stop doesn't offer to cancel the rest:\n%s", msg)
	}
}
