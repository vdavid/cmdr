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
// `apps/viewer` (2 rules / 1,000 lines) and `apps/viewer/src-tauri`
// (5 rules / 1,000 lines).
func twoSubsystemRepo(t *testing.T) string {
	t.Helper()
	tmp := t.TempDir()
	writeFixtureFile(t, tmp, "package.json", "{}")
	writeFixtureFile(t, tmp, "apps/viewer/package.json", "{}")
	writeFixtureFile(t, tmp, "apps/viewer/src/lib/CLAUDE.md", rules(2, 3))
	writeFixtureFile(t, tmp, "apps/viewer/src/lib/pane.ts", sourceLines(1000))
	writeFixtureFile(t, tmp, "apps/viewer/src-tauri/Cargo.toml", "[package]\nname = \"cmdr\"\n")
	writeFixtureFile(t, tmp, "apps/viewer/src-tauri/src/CLAUDE.md", rules(5, 0))
	writeFixtureFile(t, tmp, "apps/viewer/src-tauri/src/main.rs", sourceLines(1000))
	return tmp
}

// desktopSplitRepo builds a fixture in the shape the extra sub-roots exist to break
// apart: one `package.json` covering the Svelte frontend (6 rules / 1,000 lines),
// the test harness (3 / 800), and the build scripts plus the app-level doc
// (1 / 200).
func desktopSplitRepo(t *testing.T) string {
	t.Helper()
	tmp := t.TempDir()
	writeFixtureFile(t, tmp, "package.json", "{}")
	writeFixtureFile(t, tmp, "apps/desktop/package.json", "{}")
	writeFixtureFile(t, tmp, "apps/desktop/CLAUDE.md", rules(1, 0))
	writeFixtureFile(t, tmp, "apps/desktop/scripts/build.ts", sourceLines(200))
	writeFixtureFile(t, tmp, "apps/desktop/src/lib/CLAUDE.md", rules(6, 0))
	writeFixtureFile(t, tmp, "apps/desktop/src/lib/pane.ts", sourceLines(1000))
	writeFixtureFile(t, tmp, "apps/desktop/test/CLAUDE.md", rules(3, 0))
	writeFixtureFile(t, tmp, "apps/desktop/test/e2e.ts", sourceLines(800))
	return tmp
}

func TestInvariantSubsystemRoots_PromotesEveryConfiguredSubRoot(t *testing.T) {
	roots := invariantSubsystemRoots([]string{"package.json", "apps/desktop/package.json"})
	have := make(map[string]bool, len(roots))
	for _, root := range roots {
		have[root] = true
	}
	if !have["apps/desktop"] {
		t.Errorf("expected the manifest root kept, got %v", roots)
	}
	for _, subRoot := range invariantExtraSubsystemRoots {
		if !have[subRoot] {
			t.Errorf("expected the configured sub-root %q promoted to a root, got %v", subRoot, roots)
		}
	}
}

func TestInvariantSubsystemFor_SubRootBeatsThePackageAroundIt(t *testing.T) {
	roots := invariantSubsystemRoots([]string{"apps/desktop/package.json"})
	cases := map[string]string{
		"apps/desktop/src/lib/pane.ts":  "apps/desktop/src",
		"apps/desktop/test/CLAUDE.md":   "apps/desktop/test",
		"apps/desktop/scripts/build.ts": "apps/desktop",
		"apps/desktop/CLAUDE.md":        "apps/desktop",
		"apps/desktop/src-extras/x.ts":  "apps/desktop", // prefix must stop at a path boundary
	}
	for rel, want := range cases {
		if got := invariantSubsystemFor(rel, roots); got != want {
			t.Errorf("invariantSubsystemFor(%q) = %q, want %q", rel, got, want)
		}
	}
}

func TestMeasureInvariantDensity_SplitsFrontendAndTestsFromTheirPackage(t *testing.T) {
	report, err := measureInvariantDensity(desktopSplitRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	wantRules := map[string]int{"apps/desktop/src": 6, "apps/desktop/test": 3, "apps/desktop": 1}
	for root, want := range wantRules {
		if got := report.rulesByRoot[root]; got != want {
			t.Errorf("%s rules = %d, want %d", root, got, want)
		}
	}
	// The split may not invent or lose anything: the three buckets reconcile to what
	// the single `apps/desktop` bucket carried before.
	rules, lines := 0, 0
	for _, subsystem := range report.subsystems {
		rules += subsystem.rules
		lines += subsystem.sourceLines
	}
	if rules != report.totalRules || rules != 10 {
		t.Errorf("bucket rules sum to %d, want 10 and the report total %d", rules, report.totalRules)
	}
	if lines != report.totalLines || lines != 2000 {
		t.Errorf("bucket source lines sum to %d, want 2,000 and the report total %d", lines, report.totalLines)
	}
}

func TestInvariantSubsystemFor_LongestPrefixWins(t *testing.T) {
	roots := []string{"apps/viewer", "apps/viewer/src-tauri", "crates/cmdr-index"}
	cases := map[string]string{
		"apps/viewer/src/lib/CLAUDE.md":       "apps/viewer",
		"apps/viewer/src-tauri/src/CLAUDE.md": "apps/viewer/src-tauri",
		"apps/viewer/src-tauri/Cargo.toml":    "apps/viewer/src-tauri",
		"crates/cmdr-index/src/lib.rs":        "crates/cmdr-index",
		"docs/architecture.md":                ".",
		"AGENTS.md":                           ".",
		"apps/viewer-extras/src/CLAUDE.md":    ".", // prefix must stop at a path boundary
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
	if got := report.rulesByRoot["apps/viewer"]; got != 2 {
		t.Errorf("apps/viewer rules = %d, want 2 (src-tauri must not fold into its parent package)", got)
	}
	if got := report.rulesByRoot["apps/viewer/src-tauri"]; got != 5 {
		t.Errorf("apps/viewer/src-tauri rules = %d, want 5", got)
	}
	// Worst density first: 5 rules per 1,000 lines beats 2 per 1,000.
	if len(report.subsystems) != 2 || report.subsystems[0].root != "apps/viewer/src-tauri" {
		t.Fatalf("expected src-tauri ranked first, got %+v", report.subsystems)
	}
	if got := report.subsystems[0].rulesPerKiloLine(); got != 5 {
		t.Errorf("src-tauri density = %v, want 5", got)
	}
	if got := report.subsystems[1].rulesPerKiloLine(); got != 2 {
		t.Errorf("apps/viewer density = %v, want 2", got)
	}
}

func TestCountInvariantMarkers_IgnoresMentionedMarkers(t *testing.T) {
	tmp := t.TempDir()
	writeFixtureFile(t, tmp, "CLAUDE.md", strings.Join([]string{
		"- ❌ Never do the thing.",
		"The `❌` marker means \"never do X\", and `⚠️` means watch out.",
		"```md",
		"- ❌ Never do the example thing.",
		"```",
		"- ⚠️ Watch out for real.",
	}, "\n"))

	ruleCount, cautionCount, err := countInvariantMarkers(filepath.Join(tmp, "CLAUDE.md"))
	if err != nil {
		t.Fatal(err)
	}
	if ruleCount != 1 {
		t.Errorf("rules = %d, want 1 (only the prose rule counts)", ruleCount)
	}
	if cautionCount != 1 {
		t.Errorf("cautions = %d, want 1 (only the prose caution counts)", cautionCount)
	}
}

func TestRunInvariantDensity_RulesWithNoSourceRankFirst(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	// A build unit carrying rules but no source at all: no denominator, so it can't
	// report a flattering 0.00 and sink to the bottom of the table.
	writeFixtureFile(t, tmp, "crates/docs-only/Cargo.toml", "[package]\nname = \"docs-only\"\n")
	writeFixtureFile(t, tmp, "crates/docs-only/CLAUDE.md", rules(3, 0))
	writeInvariantDensityAllowlist(t, tmp, map[string]int{
		"apps/viewer": 2, "apps/viewer/src-tauri": 5, "crates/docs-only": 3,
	})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	rows := strings.Split(result.Message, "\n")
	firstRow := rows[2] // headline, header, then the worst subsystem
	if !strings.Contains(firstRow, "crates/docs-only") || !strings.Contains(firstRow, "n/a") {
		t.Errorf("expected the no-denominator subsystem ranked first and marked n/a, got: %s", result.Message)
	}
}

func TestRunInvariantDensity_GreenAtAllowlist(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/viewer": 2, "apps/viewer/src-tauri": 5})

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
	for _, want := range []string{"7 ❌", "rules/kloc", "apps/viewer/src-tauri        5.00"} {
		if !strings.Contains(result.Message, want) {
			t.Errorf("expected %q in the gauge, got: %s", want, result.Message)
		}
	}
}

func TestRunInvariantDensity_WarnsOnAnyGrowth(t *testing.T) {
	tmp := twoSubsystemRepo(t)
	// One rule more than allowed: no slack buffer, because a rule count only moves
	// when somebody writes or deletes a rule.
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/viewer": 2, "apps/viewer/src-tauri": 4})

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
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/viewer": 2})

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
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/viewer": 2, "apps/viewer/src-tauri": 9})

	result, err := RunInvariantDensity(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Fatalf("expected the allowlist ratcheted down, got: %+v", result)
	}
	reloaded := loadInvariantDensityAllowlist(tmp)
	if got := reloaded.Subsystems["apps/viewer/src-tauri"]; got != 5 {
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
		"apps/viewer":           2,
		"apps/viewer/src-tauri": 5,
		"crates/cmdr-archive":   3, // subsystem exists but carries no rules any more
		"crates/gone":           4, // subsystem itself is gone
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
	writeInvariantDensityAllowlist(t, tmp, map[string]int{"apps/viewer": 2, "apps/viewer/src-tauri": 9})

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
	if got := loadInvariantDensityAllowlist(tmp).Subsystems["apps/viewer/src-tauri"]; got != 9 {
		t.Errorf("expected the allowlist untouched in CI mode, got %d", got)
	}
}

func TestLoadInvariantDensityAllowlist_Missing(t *testing.T) {
	if list := loadInvariantDensityAllowlist(t.TempDir()); len(list.Subsystems) != 0 {
		t.Errorf("expected an empty allowlist for a missing file, got %+v", list)
	}
}
