package checks

import (
	"slices"
	"testing"
)

// The flags a contention re-run may hand nextest. Every one of them is about WHICH tests run
// and HOW, none about what cargo builds. A re-run that carries a cargo option asks cargo
// again, and on CI that question recompiled `cmdr` (1m37s on Linux, 12–15 min on macOS) to
// re-run one test for one second (#619).
var rerunAllowedFlags = map[string]bool{
	"--profile":           true,
	"--binaries-metadata": true,
	"--run-ignored":       true,
	"-E":                  true,
}

func assertRunsWithoutCargo(t *testing.T, argv []string) {
	t.Helper()
	if len(argv) < 2 || argv[0] != "nextest" || argv[1] != "run" {
		t.Fatalf("expected a `nextest run`, got %q", argv)
	}
	if !slices.Contains(argv, "--binaries-metadata") {
		t.Errorf("a run must execute the binaries the lane already built (--binaries-metadata), got %q", argv)
	}
	for i := 2; i < len(argv); i++ {
		flag := argv[i]
		if !rerunAllowedFlags[flag] {
			t.Errorf("%q in %q is not a run-only flag: it reaches cargo and can rebuild", flag, argv)
			continue
		}
		i++ // every allowed flag takes one value
	}
}

// recordingLane is a built lane whose cargo calls are recorded instead of run.
func recordingLane(opts nextestRunOptions) (*nextestLane, *[][]string) {
	var calls [][]string
	lane := &nextestLane{
		binariesMetadata: "/tmp/the-main-runs-binaries.json",
		opts:             opts,
		exec: func(argv []string) (string, error) {
			calls = append(calls, argv)
			return "     Summary [   1.000s] 1 test run: 1 passed, 0 skipped\n", nil
		},
	}
	return lane, &calls
}

func valueAfter(argv []string, flag string) string {
	if i := slices.Index(argv, flag); i >= 0 && i+1 < len(argv) {
		return argv[i+1]
	}
	return ""
}

func TestAContentionRerunExecutesTheMainRunsBinariesWithoutAskingCargo(t *testing.T) {
	for _, opts := range []nextestRunOptions{{}, {RunIgnoredOnly: true}} {
		lane, calls := recordingLane(opts)
		if _, err := lane.run("test(fixture_)"); err != nil {
			t.Fatal(err)
		}
		rerun := lane.contentionRunner()
		for _, profile := range []string{ContentionProbeProfile, ContentionRetryProfile} {
			if _, err := rerun(profile, []string{"a::b"}); err != nil {
				t.Fatal(err)
			}
		}

		if len(*calls) != 3 {
			t.Fatalf("want the main run plus two re-runs, got %q", *calls)
		}
		main := (*calls)[0]
		for i, argv := range *calls {
			assertRunsWithoutCargo(t, argv)
			if got := valueAfter(argv, "--binaries-metadata"); got != lane.binariesMetadata {
				t.Errorf("call %d runs %q, not the binaries the lane built (%q)", i, got, lane.binariesMetadata)
			}
			// The re-run must select from the same population as the main run: dropping
			// `--run-ignored only` would select nothing and read as "passed alone".
			if slices.Contains(argv, "--run-ignored") != slices.Contains(main, "--run-ignored") {
				t.Errorf("call %d disagrees with the main run on --run-ignored: %q vs %q", i, argv, main)
			}
		}
		if valueAfter((*calls)[1], "--profile") != ContentionProbeProfile ||
			valueAfter((*calls)[2], "--profile") != ContentionRetryProfile {
			t.Errorf("re-runs must name their contention profiles, got %q", (*calls)[1:])
		}
	}
}

// The build is the lane's one question to cargo, and it's the same question for every lane.
func TestTheLaneBuildIsTheOnlyCargoQuestionAndListsBinariesOnly(t *testing.T) {
	laneArgs := []string{"--workspace", "--features", "cmdr/virtual-mtp"}
	argv := nextestBuildArgv(laneArgs)
	want := []string{"nextest", "list", "--list-type", "binaries-only", "--message-format", "json", "--locked",
		"--workspace", "--features", "cmdr/virtual-mtp"}
	if !slices.Equal(argv, want) {
		t.Errorf("build argv = %q, want %q", argv, want)
	}
}
