package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// Tier 3 a11y coverage check: every .svelte component under apps/desktop/src/lib/
// must either have a colocated *.a11y.test.ts file (that imports the helper) OR
// be listed in the allowlist with a reason.
//
// This guards against new components silently skipping a11y coverage. See
// `docs/design-system.md` § "Automated contrast checks" for the full a11y
// testing strategy.
//
// Mechanics:
//   - Scope: files under `apps/desktop/src/lib/` that git tracks (no untracked
//     / gitignored files).
//   - For each .svelte: either Foo.a11y.test.ts exists alongside AND imports
//     from `$lib/test-a11y`, OR the component's relative path is in the
//     allowlist.
//   - Flags dead allowlist entries (paths pointing to files that no longer
//     exist) — forces cleanup when components move or get deleted.

const a11yCoverageScope = "apps/desktop/src/lib"

// a11yCoverageTestImportMarker is what we look for inside a *.a11y.test.ts file
// to confirm it actually exercises the helper (catches empty files that only
// exist to silence the check).
const a11yCoverageTestImportMarker = "$lib/test-a11y"

type a11yCoverageAllowlist struct {
	// Exempt maps a relative path (from repo root) to a human-readable reason.
	// Example: "apps/desktop/src/lib/file-explorer/pane/FilePane.svelte": "too composed for jsdom — tier 2 covers"
	Exempt map[string]string `json:"exempt"`
}

func loadA11yCoverageAllowlist(rootDir string) (a11yCoverageAllowlist, error) {
	path := filepath.Join(rootDir, "scripts", "check", "checks", "a11y-coverage-allowlist.json")
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return a11yCoverageAllowlist{Exempt: map[string]string{}}, nil
		}
		return a11yCoverageAllowlist{}, err
	}
	var list a11yCoverageAllowlist
	if err := json.Unmarshal(data, &list); err != nil {
		return a11yCoverageAllowlist{}, fmt.Errorf("parse allowlist: %w", err)
	}
	if list.Exempt == nil {
		list.Exempt = map[string]string{}
	}
	return list, nil
}

// listTrackedFiles returns tracked files under the given prefix, relative to rootDir.
// Uses `git ls-files` so untracked / gitignored files are ignored.
func listTrackedFiles(rootDir, prefix string) ([]string, error) {
	cmd := exec.Command("git", "ls-files", "--", prefix)
	cmd.Dir = rootDir
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git ls-files: %w", err)
	}
	raw := strings.Split(strings.TrimSpace(string(out)), "\n")
	files := make([]string, 0, len(raw))
	for _, f := range raw {
		f = strings.TrimSpace(f)
		if f != "" {
			files = append(files, f)
		}
	}
	return files, nil
}

// testFilePathFor returns the expected a11y test file path for a given .svelte
// component. Input: "apps/desktop/src/lib/ui/Button.svelte".
// Output: "apps/desktop/src/lib/ui/Button.a11y.test.ts".
func testFilePathFor(sveltePath string) string {
	return strings.TrimSuffix(sveltePath, ".svelte") + ".a11y.test.ts"
}

// testFileIsValid returns true if the test file exists and contains the helper import.
// Empty test files that exist but don't actually run axe are treated as missing.
func testFileIsValid(rootDir, testRelPath string) bool {
	data, err := os.ReadFile(filepath.Join(rootDir, testRelPath))
	if err != nil {
		return false
	}
	return strings.Contains(string(data), a11yCoverageTestImportMarker)
}

type a11yCoverageResult struct {
	uncoveredFiles   []string          // .svelte files without tests and not allowlisted
	emptyTestFiles   []string          // test files that exist but don't import the helper
	deadAllowlist    []string          // allowlist entries pointing to files that don't exist
	allowlistedCount int               // count of valid allowlist entries
	coveredCount     int               // count of components with valid test files
	allowlistReasons map[string]string // for formatting
}

func scanA11yCoverage(rootDir string, allowlist a11yCoverageAllowlist) (a11yCoverageResult, error) {
	var result a11yCoverageResult
	result.allowlistReasons = allowlist.Exempt

	tracked, err := listTrackedFiles(rootDir, a11yCoverageScope)
	if err != nil {
		return result, err
	}

	// Build a set of all tracked files for dead-allowlist detection.
	trackedSet := make(map[string]bool, len(tracked))
	for _, f := range tracked {
		trackedSet[f] = true
	}

	// Walk every .svelte in scope.
	for _, rel := range tracked {
		if !strings.HasSuffix(rel, ".svelte") {
			continue
		}
		// Route-level files (+layout.svelte, +page.svelte) aren't under src/lib/,
		// but guard anyway in case the scope shifts.
		base := filepath.Base(rel)
		if strings.HasPrefix(base, "+") {
			continue
		}

		if _, exempt := allowlist.Exempt[rel]; exempt {
			result.allowlistedCount++
			continue
		}

		testRel := testFilePathFor(rel)
		if !trackedSet[testRel] {
			result.uncoveredFiles = append(result.uncoveredFiles, rel)
			continue
		}
		if !testFileIsValid(rootDir, testRel) {
			result.emptyTestFiles = append(result.emptyTestFiles, testRel)
			continue
		}
		result.coveredCount++
	}

	// Dead allowlist entries: paths in the allowlist that no longer exist as tracked files.
	for path := range allowlist.Exempt {
		if !trackedSet[path] {
			result.deadAllowlist = append(result.deadAllowlist, path)
		}
	}

	sort.Strings(result.uncoveredFiles)
	sort.Strings(result.emptyTestFiles)
	sort.Strings(result.deadAllowlist)

	return result, nil
}

func formatA11yCoverageFailure(r a11yCoverageResult) string {
	var sb strings.Builder
	sb.WriteString("a11y coverage gaps found. Add a tier-3 test OR allowlist with reason.\n")

	if len(r.uncoveredFiles) > 0 {
		sb.WriteString(fmt.Sprintf("  %d component(s) without a tier-3 a11y test:\n", len(r.uncoveredFiles)))
		for _, f := range r.uncoveredFiles {
			sb.WriteString(fmt.Sprintf("    - %s (expected %s)\n", f, testFilePathFor(f)))
		}
	}
	if len(r.emptyTestFiles) > 0 {
		sb.WriteString(fmt.Sprintf("  %d test file(s) exist but don't import from %q:\n", len(r.emptyTestFiles), a11yCoverageTestImportMarker))
		for _, f := range r.emptyTestFiles {
			sb.WriteString(fmt.Sprintf("    - %s\n", f))
		}
	}
	if len(r.deadAllowlist) > 0 {
		sb.WriteString(fmt.Sprintf("  %d dead allowlist entry/entries (file no longer exists):\n", len(r.deadAllowlist)))
		for _, f := range r.deadAllowlist {
			sb.WriteString(fmt.Sprintf("    - %s — remove from scripts/check/checks/a11y-coverage-allowlist.json\n", f))
		}
	}
	sb.WriteString("\nTemplate for new test: see apps/desktop/src/lib/ui/CLAUDE.md § Adding a component-level a11y test (tier 3).\n")
	sb.WriteString("Allowlist is for components that genuinely can't be tested here (tier 2 covers, too composed, etc.). Include a reason.")
	return strings.TrimRight(sb.String(), "\n")
}

// RunA11yCoverage ensures every tracked .svelte component under src/lib/ has a
// colocated *.a11y.test.ts or is explicitly allowlisted.
func RunA11yCoverage(ctx *CheckContext) (CheckResult, error) {
	allowlist, err := loadA11yCoverageAllowlist(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("load allowlist: %w", err)
	}
	result, err := scanA11yCoverage(ctx.RootDir, allowlist)
	if err != nil {
		return CheckResult{}, fmt.Errorf("scan: %w", err)
	}

	if len(result.uncoveredFiles) == 0 && len(result.emptyTestFiles) == 0 && len(result.deadAllowlist) == 0 {
		suffix := ""
		if result.allowlistedCount > 0 {
			suffix = fmt.Sprintf(" (%d allowlisted)", result.allowlistedCount)
		}
		return Success(fmt.Sprintf("%d component(s) covered%s", result.coveredCount, suffix)), nil
	}

	return CheckResult{}, fmt.Errorf("%s", formatA11yCoverageFailure(result))
}
