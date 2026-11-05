package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

// relatedSourceDirs are the trees, relative to the repo root, that hold the
// desktop vitest suite's code: the same set `vitest.config.ts` includes tests
// from. A change anywhere else can't change what those tests import.
var relatedSourceDirs = []string{
	"apps/desktop/src/",
	"apps/desktop/scripts/",
	"apps/desktop/eslint-plugins/",
	"apps/desktop/test/",
	"eslint-plugins/",
}

var relatedSourceExts = map[string]bool{".ts": true, ".js": true, ".mjs": true, ".svelte": true}

// relatedTestTargets turns the branch's changed repo paths into the module paths
// `vitest related` takes, relative to `apps/desktop`, keeping only code the
// desktop suite can import. Order follows the input; duplicates drop.
func relatedTestTargets(changed []string) []string {
	var targets []string
	seen := map[string]bool{}
	for _, path := range changed {
		if path == "" || seen[path] || !relatedSourceExts[filepath.Ext(path)] {
			continue
		}
		for _, dir := range relatedSourceDirs {
			if !strings.HasPrefix(path, dir) {
				continue
			}
			seen[path] = true
			if rel, ok := strings.CutPrefix(path, "apps/desktop/"); ok {
				targets = append(targets, rel)
			} else {
				targets = append(targets, "../../"+path)
			}
			break
		}
	}
	return targets
}

// catalogDir holds the app's message catalogs, one JSON per area per locale.
const catalogDir = "apps/desktop/src/lib/intl/messages/"

// changedCatalogs says whether the branch touched a message catalog.
func changedCatalogs(changed []string) bool {
	for _, path := range changed {
		if strings.HasPrefix(path, catalogDir) && filepath.Ext(path) == ".json" {
			return true
		}
	}
	return false
}

// catalogReaderRe spots a test that reads catalogs: by path (`messages/en`,
// `join(…, 'messages', …)`), through the plugin's virtual modules, or through the two
// loaders (`vite-locale-catalogs.ts`, `i18n-catalog-lib.ts`). ❌ Never pass a loader itself
// to `vitest related`: `vitest.config.ts` imports both, and a config dependency selects the
// whole suite. `intl/messages.svelte`, the runtime every component reaches, deliberately
// doesn't match.
var catalogReaderRe = regexp.MustCompile(`messages/en|intl/messages['/]|virtual:cmdr-(?:catalog|locale-manifest)|'messages'|vite-locale-catalogs|i18n-catalog-lib`)

// catalogReaderTargets lists, relative to `desktopDir`, the test files an edit to a catalog
// JSON should select. A catalog reaches the runtime through the `virtual:cmdr-catalog`
// plugin, which Vitest's graph walk can't see, so without this a copy edit selected no test
// at all. The set: every test of the runtime (`src/lib/intl/`) and every test whose source
// reads catalogs. A component test asserting English copy isn't in it; the full suite in CI
// catches that.
func catalogReaderTargets(desktopDir string) ([]string, error) {
	repoRoot := filepath.Join(desktopDir, "..", "..")
	var tests []string
	for _, dir := range relatedSourceDirs {
		root := filepath.Join(repoRoot, dir)
		if rel, ok := strings.CutPrefix(dir, "apps/desktop/"); ok {
			root = filepath.Join(desktopDir, rel)
		}
		err := filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				if os.IsNotExist(err) {
					return filepath.SkipDir
				}
				return err
			}
			if entry.IsDir() {
				if entry.Name() == "node_modules" {
					return filepath.SkipDir
				}
				return nil
			}
			name := entry.Name()
			if !strings.HasSuffix(name, ".test.ts") && !strings.HasSuffix(name, ".test.js") {
				return nil
			}
			rel, relErr := filepath.Rel(desktopDir, path)
			if relErr != nil {
				return relErr
			}
			rel = filepath.ToSlash(rel)
			if strings.HasPrefix(rel, "src/lib/intl/") {
				tests = append(tests, rel)
				return nil
			}
			source, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			if catalogReaderRe.Match(source) {
				tests = append(tests, rel)
			}
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	sort.Strings(tests)
	return tests, nil
}

// branchChangedFiles lists what this branch changed against `origin/main`:
// committed, staged, unstaged, and untracked alike, so the lane works before a
// commit and after one. Without an `origin/main` (a fresh clone with no fetch),
// it falls back to the uncommitted changes alone. The base comes from
// `BranchBase`, which this lane's fingerprint also carries
// (`KeyedOnBranchBase`), so a named run's cache hit can't be from another base.
func branchChangedFiles(rootDir string) ([]string, error) {
	base, err := BranchBase(rootDir)
	if err != nil {
		return nil, err
	}
	diff, err := runGit(rootDir, "diff", "--name-only", "--diff-filter=d", base)
	if err != nil {
		return nil, err
	}
	untracked, err := runGit(rootDir, "ls-files", "--others", "--exclude-standard")
	if err != nil {
		return nil, err
	}
	return strings.Split(diff+"\n"+untracked, "\n"), nil
}

var vitestPassedRe = regexp.MustCompile(`Tests\s+(\d+) passed`)

// RunSvelteTestsRelated runs only the desktop vitest files whose imports reach
// what this branch changed, a seconds-to-a-minute subset of `svelte-tests` for
// an agent to run beside `--fast`. Named-only: the full suite gates in CI.
func RunSvelteTestsRelated(ctx *CheckContext) (CheckResult, error) {
	changed, err := branchChangedFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read what this branch changed: %w", err)
	}
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	targets := relatedTestTargets(changed)
	sourceCount := len(targets)
	catalogsChanged := changedCatalogs(changed)
	if catalogsChanged {
		readers, err := catalogReaderTargets(desktopDir)
		if err != nil {
			return CheckResult{}, fmt.Errorf("couldn't list the tests that read message catalogs: %w", err)
		}
		targets = append(targets, readers...)
	}
	if len(targets) == 0 {
		return Skipped("no desktop frontend code changed"), nil
	}

	reportDir, err := os.MkdirTemp("", "cmdr-svelte-related-*")
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to create the report dir: %w", err)
	}
	defer os.RemoveAll(reportDir)
	jsonReport := filepath.Join(reportDir, "test-results.json")

	args := append([]string{"exec", "vitest", "related", "--run", "--passWithNoTests"}, targets...)
	cmd := exec.Command("pnpm", args...)
	cmd.Dir = desktopDir
	cmd.Env = append(os.Environ(), "VITEST_JSON_REPORT="+jsonReport)
	output, err := RunCommand(cmd, true)
	recordVitestTests(ctx, jsonReport, desktopDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("related svelte tests failed: %s", diagnoseVitestFailure(jsonReport, desktopDir, output))
	}

	files := fmt.Sprintf("%d changed %s", sourceCount, Pluralize(sourceCount, "file", "files"))
	if catalogsChanged {
		files += " and the message catalogs"
	}
	match := vitestPassedRe.FindStringSubmatch(StripANSI(output))
	if match == nil {
		return Success("no tests reach the " + files), nil
	}
	count, _ := strconv.Atoi(match[1])
	result := Success(fmt.Sprintf("%d %s reaching the %s passed", count, Pluralize(count, "test", "tests"), files))
	result.Total = count
	return result, nil
}
