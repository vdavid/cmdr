package checks

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"cmdr/scripts/check/targetsweep"
)

// fakeCargo stands in for a cargo run: JSON on stdout, progress on stderr, and an exit
// code. `finished` adds cargo's own verdict line. A subshell, because cargo is its own
// process: an `exit` in braces would end the script under test.
func fakeCargo(finished bool, code int) string {
	out := `printf '%s\n' '{"reason":"compiler-artifact","filenames":["/t/debug/deps/liba-1111111111111111.rlib"]}'`
	if finished {
		out += `; printf '%s\n' '{"reason":"build-finished","success":true}'`
	}
	return "( " + out + "; echo '    Checking a v0.1.0' >&2; exit " + strconv.Itoa(code) + " )"
}

func runRecorded(t *testing.T, target, lane string, stream cargoStream, command string) (string, int) {
	t.Helper()
	cmd := exec.Command("sh", "-c", recordedCargoScriptIn(target, lane, stream, command))
	out, err := cmd.CombinedOutput()
	code := 0
	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		code = exitErr.ExitCode()
	} else if err != nil {
		t.Fatal(err)
	}
	return string(out), code
}

func liveFiles(t *testing.T, target string) []string {
	t.Helper()
	entries, err := os.ReadDir(filepath.Join(target, targetsweep.LiveDir))
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, e := range entries {
		names = append(names, e.Name())
	}
	return names
}

// A finished build becomes the lane's complete stream; the output is cargo's stderr, the
// same thing a text-format run printed; the exit code is cargo's.
func TestRecordedCargoScriptSavesAFinishedBuildAsTheLanesStream(t *testing.T) {
	target := t.TempDir()
	out, code := runRecorded(t, target, targetsweep.LaneClippy, stdoutToRecord, fakeCargo(true, 0))
	if code != 0 {
		t.Fatalf("exit %d, want 0; output:\n%s", code, out)
	}
	if strings.TrimSpace(out) != "Checking a v0.1.0" {
		t.Errorf("output should be stderr only, got:\n%s", out)
	}
	if got := liveFiles(t, target); len(got) != 1 || got[0] != "clippy.jsonl" {
		t.Errorf("live files = %v, want just clippy.jsonl", got)
	}
	data, err := os.ReadFile(filepath.Join(target, targetsweep.LiveDir, "clippy.jsonl"))
	if err != nil || !strings.Contains(string(data), "liba-1111111111111111.rlib") {
		t.Errorf("the record should hold cargo's JSON, got %q (%v)", data, err)
	}
	if _, err := os.Stat(filepath.Join(target, targetsweep.StampFile)); err != nil {
		t.Errorf("an unseeded volume should get a stamp: %v", err)
	}
}

// A red TEST run still finished its build, so it records; a build that stopped (a compile
// error) leaves its partial stream for the sweep to count, and the lane's last complete
// stream stays.
func TestRecordedCargoScriptKeepsAStoppedBuildsPartialStream(t *testing.T) {
	target := t.TempDir()
	if _, code := runRecorded(t, target, targetsweep.LaneTests, stdoutToRecord, fakeCargo(true, 4)); code != 4 {
		t.Fatalf("exit %d, want nextest's 4", code)
	}
	if got := liveFiles(t, target); len(got) != 1 || got[0] != "tests.jsonl" {
		t.Fatalf("a red test run whose build finished should record, got %v", got)
	}

	if _, code := runRecorded(t, target, targetsweep.LaneTests, stdoutToRecord, fakeCargo(false, 7)); code != 7 {
		t.Fatalf("exit %d, want cargo's 7", code)
	}
	got := liveFiles(t, target)
	if len(got) != 2 || got[0] != "tests.jsonl" || !strings.HasPrefix(got[1], "tests.partial-") {
		t.Errorf("live files = %v, want tests.jsonl plus one tests.partial-*", got)
	}
}

// The doc lane reads cargo's JSON out of its own output, so its record takes both streams
// and hands all of it back.
func TestRecordedCargoScriptBothStreamsModeHandsEverythingBack(t *testing.T) {
	target := t.TempDir()
	out, code := runRecorded(t, target, targetsweep.LaneDoc, bothToRecord, fakeCargo(true, 0))
	if code != 0 {
		t.Fatalf("exit %d", code)
	}
	for _, want := range []string{`"reason":"compiler-artifact"`, "Checking a v0.1.0"} {
		if !strings.Contains(out, want) {
			t.Errorf("output should contain %q, got:\n%s", want, out)
		}
	}
	if got := liveFiles(t, target); len(got) != 1 || got[0] != "doc.jsonl" {
		t.Errorf("live files = %v, want just doc.jsonl", got)
	}
}

// A stamp already there (a seeded volume's) is never moved: it's the seed boundary.
func TestRecordedCargoScriptKeepsAnExistingStamp(t *testing.T) {
	target := t.TempDir()
	stamp := filepath.Join(target, targetsweep.StampFile)
	if err := os.WriteFile(stamp, []byte("12345\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runRecorded(t, target, targetsweep.LaneClippy, stdoutToRecord, fakeCargo(true, 0))
	if data, _ := os.ReadFile(stamp); string(data) != "12345\n" {
		t.Errorf("stamp = %q, want it untouched", data)
	}
}

func TestWithCargoFlagGoesBeforeTheToolsArguments(t *testing.T) {
	got := strings.Join(withCargoFlag([]string{"clippy", "--locked", "--", "-D", "warnings"}, "--message-format=json-render-diagnostics"), " ")
	if got != "clippy --locked --message-format=json-render-diagnostics -- -D warnings" {
		t.Errorf("got %q", got)
	}
	if got := strings.Join(withCargoFlag([]string{"doc"}, "-x"), " "); got != "doc -x" {
		t.Errorf("got %q", got)
	}
}
