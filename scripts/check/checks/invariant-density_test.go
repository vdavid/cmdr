package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeFixtureFile writes content at dir/relPath, creating parent directories.
func writeFixtureFile(t *testing.T, dir, relPath, content string) {
	t.Helper()
	full := filepath.Join(dir, filepath.FromSlash(relPath))
	if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(full, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

// sourceLines returns a source file body with the given number of lines.
func sourceLines(n int) string {
	return strings.Repeat("let x = 1;\n", n)
}

// rules returns a doc body carrying n ❌ rules and c ⚠️ cautions.
func rules(n, c int) string {
	return strings.Repeat("- ❌ Never do the thing.\n", n) + strings.Repeat("- ⚠️ Watch out.\n", c)
}

// writeInvariantDensityAllowlist writes a complete allowlist JSON for a fixture repo.
func writeInvariantDensityAllowlist(t *testing.T, dir string, subsystems map[string]int) {
	t.Helper()
	checksDir := filepath.Join(dir, "scripts", "check", "checks")
	if err := os.MkdirAll(checksDir, 0755); err != nil {
		t.Fatal(err)
	}
	list := invariantDensityAllowlist{Comment: "test allowlist", Subsystems: subsystems}
	if err := writeJSONAllowlist(invariantDensityAllowlistPath(dir), list); err != nil {
		t.Fatal(err)
	}
}

// twoSubsystemRepo builds a fixture with a JS package that nests a Rust member:
// `apps/desktop` (2 rules / 1,000 lines) and `apps/desktop/src-tauri`
// (5 rules / 1,000 lines).
func twoSubsystemRepo(t *testing.T) string {
	t.Helper()
	tmp := t.TempDir()
	writeFixtureFile(t, tmp, "package.json", "{}")
	writeFixtureFile(t, tmp, "apps/desktop/package.json", "{}")
	writeFixtureFile(t, tmp, "apps/desktop/src/lib/CLAUDE.md", rules(2, 3))
	writeFixtureFile(t, tmp, "apps/desktop/src/lib/pane.ts", sourceLines(1000))
	writeFixtureFile(t, tmp, "apps/desktop/src-tauri/Cargo.toml", "[package]\nname = \"cmdr\"\n")
	writeFixtureFile(t, tmp, "apps/desktop/src-tauri/src/CLAUDE.md", rules(5, 0))
	writeFixtureFile(t, tmp, "apps/desktop/src-tauri/src/main.rs", sourceLines(1000))
	return tmp
}

func TestInvariantSubsystemFor_LongestPrefixWins(t *testing.T) {
	roots := []string{"apps/desktop", "apps/desktop/src-tauri", "crates/cmdr-index"}
	cases := map[string]string{
		"apps/desktop/src/lib/CLAUDE.md":       "apps/desktop",
		"apps/desktop/src-tauri/src/CLAUDE.md": "apps/desktop/src-tauri",
		"apps/desktop/src-tauri/Cargo.toml":    "apps/desktop/src-tauri",
		"crates/cmdr-index/src/lib.rs":         "crates/cmdr-index",
		"docs/architecture.md":                 ".",
		"AGENTS.md":                            ".",
		"apps/desktop-extras/src/CLAUDE.md":    ".", // prefix must stop at a path boundary
	}
	for rel, want := range cases {
		if got := invariantSubsystemFor(rel, roots); got != want {
			t.Errorf("invariantSubsystemFor(%q) = %q, want %q", rel, got, want)
		}
	}
}

func TestMeasureInvariantDensity_AttributesRulesAndLines(t *testing.T) {
	report, err := measureInvariantDensity(twoSubsystemRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	if report.totalRules != 7 {
		t.Errorf("totalRules = %d, want 7", report.totalRules)
	}
	if report.totalCautions != 3 {
		t.Errorf("totalCautions = %d, want 3", report.totalCautions)
	}
	if report.totalDocs != 2 {
		t.Errorf("totalDocs = %d, want 2", report.totalDocs)
	}
	if got := report.rulesByRoot["apps/desktop"]; got != 2 {
		t.Errorf("apps/desktop rules = %d, want 2 (src-tauri must not fold into its parent package)", got)
	}
	if got := report.rulesByRoot["apps/desktop/src-tauri"]; got != 5 {
		t.Errorf("apps/desktop/src-tauri rules = %d, want 5", got)
	}
	// Worst density first: 5 rules per 1,000 lines beats 2 per 1,000.
	if len(report.subsystems) != 2 || report.subsystems[0].root != "apps/desktop/src-tauri" {
		t.Fatalf("expected src-tauri ranked first, got %+v", report.subsystems)
	}
	if got := report.subsystems[0].rulesPerKiloLine(); got != 5 {
		t.Errorf("src-tauri density = %v, want 5", got)
	}
	if got := report.subsystems[1].rulesPerKiloLine(); got != 2 {
		t.Errorf("apps/desktop density = %v, want 2", got)
	}
}

func TestRunInvariantDensity_GreenAtAllowlist(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/desktop": 2, "apps/desktop/src-tauri": 5})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	if result.MadeChanges {
		t.Errorf("expected no allowlist rewrite, got: %s", result.Message)
	}
	// The gauge itself is the message: worst subsystem, its density, the totals.
	for _, want := range []string{"7 ❌", "rules/kloc", "apps/desktop/src-tauri        5.00"} {
		if !strings.Contains(result.Message, want) {
			t.Errorf("expected %q in the gauge, got: %s", want, result.Message)
		}
	}
}

func TestRunInvariantDensity_WarnsOnAnyGrowth(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	// One rule more than allowed: no slack buffer, because a rule count only moves
	// when somebody writes or deletes a rule.
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/desktop": 2, "apps/desktop/src-tauri": 4})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Fatalf("expected warning, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "allowlist: 4") {
		t.Errorf("expected the allowed count in the message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "src-tauri/src/CLAUDE.md") {
		t.Errorf("expected the heaviest doc named so the warning is actionable, got: %s", result.Message)
	}
}

func TestRunInvariantDensity_WarnsOnUnlistedSubsystem(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/desktop": 2})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Fatalf("expected warning for the unlisted subsystem, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "not in the allowlist") {
		t.Errorf("expected 'not in the allowlist', got: %s", result.Message)
	}
}

func TestRunInvariantDensity_CautionsNeverWarn(t *testing.T) {
	tmp := t.TempDir()
	writeFixtureFile(t, tmp, "crates/cmdr-fs/Cargo.toml", "[package]\nname = \"cmdr-fs\"\n")
	writeFixtureFile(t, tmp, "crates/cmdr-fs/src/lib.rs", sourceLines(500))
	writeFixtureFile(t, tmp, "crates/cmdr-fs/src/CLAUDE.md", rules(1, 40))
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"crates/cmdr-fs": 1})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("⚠️ cautions are informational and must never warn, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunInvariantDensity_RatchetsDownLocally(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/desktop": 2, "apps/desktop/src-tauri": 9})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Fatalf("expected the allowlist ratcheted down, got: %+v", result)
	}
	reloaded := loadInvariantDensityAllowlist(tmp)
	if got := reloaded.Subsystems["apps/desktop/src-tauri"]; got != 5 {
		t.Errorf("expected ratchet 9 → 5, got %d", got)
	}
	if reloaded.Comment == "" {
		t.Error("expected $comment preserved across the rewrite")
	}
}

func TestRunInvariantDensity_RemovesDeadAndEmptyEntriesLocally(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	writeFixtureFile(t, tmp, "crates/cmdr-archive/Cargo.toml", "[package]\nname = \"cmdr-archive\"\n")
	writeFixtureFile(t, tmp, "crates/cmdr-archive/src/lib.rs", sourceLines(100))
	writeInvariantDensityAllowlist(t, tmp, map[string]int{
		"apps/desktop":           2,
		"apps/desktop/src-tauri": 5,
		"crates/cmdr-archive":    3, // subsystem exists but carries no rules any more
		"crates/gone":            4, // subsystem itself is gone
	})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Fatalf("expected stale entries dropped, got: %+v", result)
	}
	reloaded := loadInvariantDensityAllowlist(tmp)
	for _, gone := range []string{"crates/cmdr-archive", "crates/gone"} {
		if _, ok := reloaded.Subsystems[gone]; ok {
			t.Errorf("expected %q dropped from the allowlist", gone)
		}
	}
	if len(reloaded.Subsystems) != 2 {
		t.Errorf("expected the two live entries kept, got %+v", reloaded.Subsystems)
	}
}

func TestRunInvariantDensity_CIReportsStaleWithoutRewriting(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/desktop": 2, "apps/desktop/src-tauri": 9})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp, CI: true})
	if err != nil {
		t.Fatal(err)
	}
	if result.MadeChanges {
		t.Error("expected no rewrite in CI mode")
	}
	if !strings.Contains(result.Message, "9 → 5") {
		t.Errorf("expected the ratchet reported in CI, got: %s", result.Message)
	}
	if got := loadInvariantDensityAllowlist(tmp).Subsystems["apps/desktop/src-tauri"]; got != 9 {
		t.Errorf("expected the allowlist untouched in CI mode, got %d", got)
	}
}

func TestLoadInvariantDensityAllowlist_Missing(t *testing.T) {
	if list := loadInvariantDensityAllowlist(t.TempDir()); len(list.Subsystems) != 0 {
		t.Errorf("expected an empty allowlist for a missing file, got %+v", list)
	}
}
