package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestNewCoverageRun_IsolatesReportsDir(t *testing.T) {
	desktopDir := t.TempDir()

	a, err := newCoverageRun(desktopDir)
	if err != nil {
		t.Fatalf("newCoverageRun: %v", err)
	}
	defer os.RemoveAll(a.reportsDir)
	b, err := newCoverageRun(desktopDir)
	if err != nil {
		t.Fatalf("newCoverageRun: %v", err)
	}
	defer os.RemoveAll(b.reportsDir)

	// Two invocations must land in distinct directories, or a concurrent run's
	// cleanup of reportsDir/.tmp deletes the other's in-flight v8 worker files.
	if a.reportsDir == b.reportsDir {
		t.Fatalf("expected distinct reports dirs, both were %q", a.reportsDir)
	}
	for _, run := range []*coverageRun{a, b} {
		if !strings.HasPrefix(run.reportsDir, os.TempDir()) {
			t.Errorf("expected reports dir under %q, got %q", os.TempDir(), run.reportsDir)
		}
		if info, err := os.Stat(run.reportsDir); err != nil || !info.IsDir() {
			t.Errorf("expected reports dir to exist, stat err=%v", err)
		}
		if run.summary != filepath.Join(run.reportsDir, "coverage-summary.json") {
			t.Errorf("summary %q not under reports dir %q", run.summary, run.reportsDir)
		}
		if run.cmd.Dir != desktopDir {
			t.Errorf("expected cmd dir %q, got %q", desktopDir, run.cmd.Dir)
		}
		wantEnv := "VITEST_COVERAGE_DIR=" + run.reportsDir
		if !containsEnv(run.cmd.Env, wantEnv) {
			t.Errorf("expected env to contain %q, got %v", wantEnv, run.cmd.Env)
		}
	}
}

func containsEnv(env []string, want string) bool {
	for _, e := range env {
		if e == want {
			return true
		}
	}
	return false
}

func writeCoverageFixture(t *testing.T, desktopDir string, files []string) {
	t.Helper()
	for _, rel := range files {
		full := filepath.Join(desktopDir, "src", "lib", rel)
		if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte("export {}\n"), 0644); err != nil {
			t.Fatal(err)
		}
	}
}

func TestFindStaleCoverageEntries(t *testing.T) {
	desktopDir := t.TempDir()
	writeCoverageFixture(t, desktopDir, []string{"satisfied.ts", "border.ts", "lowcov.ts"})

	allowlist := CoverageAllowlist{Files: map[string]AllowlistEntry{
		"satisfied.ts": {Reason: "depends on Tauri"}, // now at 90% → satisfied
		"border.ts":    {Reason: "depends on Tauri"}, // 72%: above threshold but inside the margin band → keep quiet
		"lowcov.ts":    {Reason: "depends on Tauri"}, // 10% → genuinely needed
		"gone.ts":      {Reason: "stale"},            // file no longer exists → dead
	}}
	srcPrefix := filepath.Join(desktopDir, "src", "lib") + "/"
	pct := func(p float64) FileCoverage {
		return FileCoverage{Lines: CoverageMetric{Pct: p}}
	}
	coverage := map[string]FileCoverage{
		srcPrefix + "satisfied.ts": pct(90.0),
		srcPrefix + "border.ts":    pct(72.0),
		srcPrefix + "lowcov.ts":    pct(10.0),
	}

	stale := findStaleCoverageEntries(desktopDir, allowlist, coverage)

	if len(stale.dead) != 1 || stale.dead[0] != "gone.ts" {
		t.Errorf("expected dead = [gone.ts], got %v", stale.dead)
	}
	if len(stale.satisfied) != 1 || !strings.Contains(stale.satisfied[0], "satisfied.ts") {
		t.Errorf("expected satisfied = [satisfied.ts ...], got %v", stale.satisfied)
	}
	if !strings.Contains(stale.satisfied[0], "90.0%") {
		t.Errorf("expected coverage percentage in satisfied entry, got %v", stale.satisfied[0])
	}
}

func TestApplyCoverageShrinkwrap_CIReportsWithoutRewriting(t *testing.T) {
	desktopDir := t.TempDir()
	allowlist := CoverageAllowlist{
		Comment: "test comment",
		Files: map[string]AllowlistEntry{
			"gone.ts": {Reason: "stale"},
		},
	}
	path := filepath.Join(desktopDir, "coverage-allowlist.json")
	if err := writeJSONAllowlist(path, allowlist); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: desktopDir, CI: true}
	stale := coverageStaleness{dead: []string{"gone.ts"}, satisfied: []string{"covered.ts (90.0%)"}}
	notes, madeChanges, err := applyCoverageShrinkwrap(ctx, desktopDir, &allowlist, stale)
	if err != nil {
		t.Fatal(err)
	}
	if madeChanges {
		t.Error("expected no rewrite in CI mode")
	}
	if len(notes) != 2 {
		t.Fatalf("expected a dead note and a satisfied note, got %d: %v", len(notes), notes)
	}
	if !strings.Contains(notes[0], "gone.ts") || !strings.Contains(notes[0], "local run removes") {
		t.Errorf("expected CI dead-entry note naming gone.ts, got: %s", notes[0])
	}
	if !strings.Contains(notes[1], "covered.ts") {
		t.Errorf("expected satisfied note naming covered.ts, got: %s", notes[1])
	}
	// The allowlist file must be untouched, and the in-memory entry kept.
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "gone.ts") {
		t.Error("expected allowlist file untouched in CI mode")
	}
	if _, ok := allowlist.Files["gone.ts"]; !ok {
		t.Error("expected in-memory allowlist untouched in CI mode")
	}
}

func TestShrinkwrapCoverageAllowlist_RemovesDeadEntries(t *testing.T) {
	desktopDir := t.TempDir()
	allowlist := CoverageAllowlist{
		Comment: "test comment",
		Files: map[string]AllowlistEntry{
			"keep.ts": {Reason: "still needed"},
			"gone.ts": {Reason: "stale"},
		},
	}
	path := filepath.Join(desktopDir, "coverage-allowlist.json")
	if err := writeJSONAllowlist(path, allowlist); err != nil {
		t.Fatal(err)
	}

	if err := shrinkwrapCoverageAllowlist(desktopDir, &allowlist, []string{"gone.ts"}); err != nil {
		t.Fatal(err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	content := string(data)
	if strings.Contains(content, "gone.ts") {
		t.Errorf("expected gone.ts removed from rewritten allowlist, got: %s", content)
	}
	if !strings.Contains(content, "keep.ts") || !strings.Contains(content, "still needed") {
		t.Errorf("expected surviving entry (with reason) preserved, got: %s", content)
	}
	if !strings.Contains(content, "test comment") {
		t.Errorf("expected $comment preserved, got: %s", content)
	}
}
