package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"sort"
	"strings"
)

// Tier 3 a11y coverage check: every .svelte component under apps/desktop/src/lib/
// must be exercised by an a11y test that imports the helper, OR be listed in the
// allowlist with a reason.
//
// This guards against new components silently skipping a11y coverage. See
// `docs/design-system.md` § "Automated contrast checks" for the full a11y
// testing strategy.
//
// Mechanics:
//   - Scope: files under `apps/desktop/src/lib/` that git tracks (no untracked
//     / gitignored files).
//   - For each .svelte, one of: Foo.a11y.test.ts exists alongside AND imports
//     from `$lib/test-a11y`; OR some *.a11y.test.ts in the SAME directory
//     imports from `$lib/test-a11y` and imports Foo.svelte itself; OR the
//     component's relative path is in the allowlist.
//   - The directory-level form is what lets one file cover a whole directory.
//     The frontend lane's cost is per test FILE, not per test (`docs/testing.md`
//     § "What a test actually costs"), so 24 one-test files cost ~24× what one
//     24-test file costs. "Imports it" is resolved from parsed import
//     statements (`a11y-test-imports.go`), never a substring search.
//   - Flags dead allowlist entries (paths pointing to files that no longer
//     exist). This forces cleanup when components move or get deleted.

const a11yCoverageScope = "apps/desktop/src/lib"

// a11yCoverageTestImportMarker is what we look for inside a *.a11y.test.ts file
// to confirm it actually exercises the helper (catches empty files that only
// exist to silence the check).
const a11yCoverageTestImportMarker = "$lib/test-a11y"

type a11yCoverageAllowlist struct {
	// Exempt maps a relative path (from repo root) to a human-readable reason.
	// Example: "apps/desktop/src/lib/file-explorer/pane/FilePane.svelte": "too composed for jsdom, tier 2 covers"
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

// componentsCoveredByDirectoryTests maps every component path that some
// *.a11y.test.ts imports to true. Only files that also import the a11y helper
// count, and only imports resolving into the test file's OWN directory: a
// sibling directory's same-named component is a different component.
func componentsCoveredByDirectoryTests(rootDir string, tracked []string) map[string]bool {
	covered := map[string]bool{}
	for _, rel := range tracked {
		if !strings.HasSuffix(rel, ".a11y.test.ts") {
			continue
		}
		data, err := os.ReadFile(filepath.Join(rootDir, rel))
		if err != nil || !strings.Contains(string(data), a11yCoverageTestImportMarker) {
			continue
		}
		dir := path.Dir(rel)
		for imported := range importedPathsIn(rel, string(data)) {
			if strings.HasSuffix(imported, ".svelte") && path.Dir(imported) == dir {
				covered[imported] = true
			}
		}
	}
	return covered
}

type a11yCoverageResult struct {
	uncoveredFiles     []string          // .svelte files without tests and not allowlisted
	emptyTestFiles     []string          // test files that exist but don't import the helper
	deadAllowlist      []string          // allowlist entries pointing to files that don't exist
	redundantAllowlist []string          // allowlist entries whose component has a valid test anyway
	allowlistedCount   int               // count of valid allowlist entries
	coveredCount       int               // count of components with valid test files
	allowlistReasons   map[string]string // for formatting
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

	byDirectoryTest := componentsCoveredByDirectoryTests(rootDir, tracked)

	// covered reports whether a component is exercised at all: by its colocated
	// file, or by any directory-level a11y test that imports it.
	covered := func(rel string) bool {
		if testRel := testFilePathFor(rel); trackedSet[testRel] && testFileIsValid(rootDir, testRel) {
			return true
		}
		return byDirectoryTest[rel]
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
			// An exempt component that has a valid test anyway makes the
			// entry redundant: the "can't be tested" reason no longer holds.
			if covered(rel) {
				result.redundantAllowlist = append(result.redundantAllowlist, rel)
				continue
			}
			result.allowlistedCount++
			continue
		}

		if covered(rel) {
			result.coveredCount++
			continue
		}
		// A colocated file that exists but never imports the helper is a stub:
		// name the file, since deleting or filling it is the fix.
		if testRel := testFilePathFor(rel); trackedSet[testRel] {
			result.emptyTestFiles = append(result.emptyTestFiles, testRel)
			continue
		}
		result.uncoveredFiles = append(result.uncoveredFiles, rel)
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
	sort.Strings(result.redundantAllowlist)

	return result, nil
}

func formatA11yCoverageFailure(r a11yCoverageResult) string {
	var sb strings.Builder
	sb.WriteString("a11y coverage gaps found. Add a tier-3 test OR allowlist with reason.\n")

	if len(r.uncoveredFiles) > 0 {
		sb.WriteString(fmt.Sprintf("  %d component(s) without a tier-3 a11y test:\n", len(r.uncoveredFiles)))
		for _, f := range r.uncoveredFiles {
			sb.WriteString(fmt.Sprintf("    - %s (add %s, or import it from a *.a11y.test.ts in %s/)\n",
				f, testFilePathFor(f), path.Dir(f)))
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
			sb.WriteString(fmt.Sprintf("    - %s: remove from scripts/check/checks/a11y-coverage-allowlist.json\n", f))
		}
	}
	if len(r.redundantAllowlist) > 0 {
		sb.WriteString(fmt.Sprintf("  %d redundant allowlist entry/entries (component has a valid a11y test anyway):\n", len(r.redundantAllowlist)))
		for _, f := range r.redundantAllowlist {
			sb.WriteString(fmt.Sprintf("    - %s: remove from scripts/check/checks/a11y-coverage-allowlist.json\n", f))
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

	if len(result.uncoveredFiles) == 0 && len(result.emptyTestFiles) == 0 && len(result.deadAllowlist) == 0 && len(result.redundantAllowlist) == 0 {
		suffix := ""
		if result.allowlistedCount > 0 {
			suffix = fmt.Sprintf(" (%d allowlisted)", result.allowlistedCount)
		}
		return Success(fmt.Sprintf("%d component(s) covered%s", result.coveredCount, suffix)), nil
	}

	return CheckResult{}, fmt.Errorf("%s", formatA11yCoverageFailure(result))
}
