package checks

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// setupGitRepo initializes a throwaway git repo with the given files staged.
// Returns the repo root. Required because scanA11yCoverage uses `git ls-files`
// to scope its search.
func setupGitRepo(t *testing.T, files map[string]string) string {
	t.Helper()
	tmp := t.TempDir()

	run := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = tmp
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}

	run("init", "--quiet")
	run("config", "user.email", "test@example.com")
	run("config", "user.name", "test")
	run("config", "commit.gpgsign", "false")

	for rel, content := range files {
		full := filepath.Join(tmp, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(content), 0644); err != nil {
			t.Fatal(err)
		}
	}
	run("add", ".")
	run("commit", "-m", "init", "--quiet")

	return tmp
}

func TestA11yCoverage_Success(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":             "<button>Click</button>",
		"apps/desktop/src/lib/ui/Button.a11y.test.ts":       `import { expectNoA11yViolations } from '$lib/test-a11y'`,
		"apps/desktop/src/lib/ui/Dialog.svelte":             "<dialog>hi</dialog>",
		"apps/desktop/src/lib/ui/Dialog.a11y.test.ts":       `import { expectNoA11yViolations } from '$lib/test-a11y'`,
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "2 component(s) covered") {
		t.Errorf("expected '2 component(s) covered', got: %s", result.Message)
	}
}

func TestA11yCoverage_MissingTest(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":             "<button>Click</button>",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for missing test")
	}
	msg := err.Error()
	if !strings.Contains(msg, "apps/desktop/src/lib/ui/Button.svelte") {
		t.Errorf("expected failure to name Button.svelte, got: %s", msg)
	}
	if !strings.Contains(msg, "Button.a11y.test.ts") {
		t.Errorf("expected failure to name expected test path, got: %s", msg)
	}
}

func TestA11yCoverage_EmptyTestFile(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":             "<button>Click</button>",
		"apps/desktop/src/lib/ui/Button.a11y.test.ts":       "// empty, doesn't import the helper",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for empty test file")
	}
	msg := err.Error()
	if !strings.Contains(msg, "don't import") {
		t.Errorf("expected failure to mention missing import, got: %s", msg)
	}
}

func TestA11yCoverage_AllowlistSuppresses(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/huge/Complex.svelte":          "<div />",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{"apps/desktop/src/lib/huge/Complex.svelte":"too composed"}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success with allowlist, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d", result.Code)
	}
	if !strings.Contains(result.Message, "1 allowlisted") {
		t.Errorf("expected '1 allowlisted' in message, got: %s", result.Message)
	}
}

func TestA11yCoverage_DeadAllowlistEntry(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		// No component at the allowlisted path (it was deleted/moved).
		"apps/desktop/src/lib/ui/Other.svelte":              "<div />",
		"apps/desktop/src/lib/ui/Other.a11y.test.ts":        `import { expectNoA11yViolations } from '$lib/test-a11y'`,
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{"apps/desktop/src/lib/deleted/Gone.svelte":"stale entry"}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for dead allowlist entry")
	}
	msg := err.Error()
	if !strings.Contains(msg, "Gone.svelte") {
		t.Errorf("expected failure to name the dead entry, got: %s", msg)
	}
	if !strings.Contains(msg, "dead allowlist entry") {
		t.Errorf("expected 'dead allowlist entry' in message, got: %s", msg)
	}
}

func TestA11yCoverage_RedundantAllowlistEntry(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		// Component is exempt, yet a valid a11y test exists → the entry is redundant.
		"apps/desktop/src/lib/ui/Tested.svelte":             "<div />",
		"apps/desktop/src/lib/ui/Tested.a11y.test.ts":       `import { expectNoA11yViolations } from '$lib/test-a11y'`,
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{"apps/desktop/src/lib/ui/Tested.svelte":"can't be tested (no longer true)"}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for redundant allowlist entry")
	}
	msg := err.Error()
	if !strings.Contains(msg, "Tested.svelte") {
		t.Errorf("expected failure to name the redundant entry, got: %s", msg)
	}
	if !strings.Contains(msg, "redundant") {
		t.Errorf("expected 'redundant' in message, got: %s", msg)
	}
}

func TestA11yCoverage_IgnoresUntrackedFiles(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":             "<button>Click</button>",
		"apps/desktop/src/lib/ui/Button.a11y.test.ts":       `import { expectNoA11yViolations } from '$lib/test-a11y'`,
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	// Create an untracked new svelte file with no test; should be ignored.
	untracked := filepath.Join(tmp, "apps/desktop/src/lib/ui/Untracked.svelte")
	if err := os.WriteFile(untracked, []byte("<div />"), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success (untracked ignored), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestA11yCoverage_SkipsRouteFiles(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/routes/+layout.svelte":        "<div />",
		"apps/desktop/src/lib/routes/+page.svelte":          "<div />",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success (route files skipped), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestA11yCoverage_MissingAllowlistIsOkWhenNoScope(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"some-other-file.txt": "unrelated",
	})

	// Don't write an allowlist file; should default to empty.
	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success (no svelte files in scope), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d", result.Code)
	}
}

// --- Directory-level a11y test files ---------------------------------------
//
// A component is also covered by any *.a11y.test.ts in its own directory that
// imports it. These tests pin the "imports it" detection: it must be an actual
// import statement resolving to that exact file, never a name that merely
// appears in the text.

const a11yHelperImport = `import { expectNoA11yViolations } from '$lib/test-a11y'` + "\n"

func TestA11yCoverage_DirectoryFileCoversImportedComponent(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Alpha.svelte": "<div />",
		"apps/desktop/src/lib/settings/Beta.svelte":  "<div />",
		"apps/desktop/src/lib/settings/sections.a11y.test.ts": a11yHelperImport +
			`import Alpha from './Alpha.svelte'` + "\n" +
			`import Beta from './Beta.svelte'` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if !strings.Contains(result.Message, "2 component(s) covered") {
		t.Errorf("expected both components covered, got: %s", result.Message)
	}
}

func TestA11yCoverage_DirectoryFileDoesNotCoverUnimportedComponent(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Alpha.svelte": "<div />",
		"apps/desktop/src/lib/settings/Beta.svelte":  "<div />",
		"apps/desktop/src/lib/settings/sections.a11y.test.ts": a11yHelperImport +
			`import Alpha from './Alpha.svelte'` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error: Beta.svelte is never imported")
	}
	if !strings.Contains(err.Error(), "Beta.svelte") {
		t.Errorf("expected failure to name Beta.svelte, got: %s", err.Error())
	}
	if strings.Contains(err.Error(), "Alpha.svelte") {
		t.Errorf("Alpha.svelte is imported and must not be reported: %s", err.Error())
	}
}

// The prefix trap: a substring match on "Section.svelte" would be satisfied by
// an import of "SearchSection.svelte", silently dropping the guarantee.
func TestA11yCoverage_PrefixIsNotAMatch(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Section.svelte":       "<div />",
		"apps/desktop/src/lib/settings/SearchSection.svelte": "<div />",
		"apps/desktop/src/lib/settings/all.a11y.test.ts": a11yHelperImport +
			`import SearchSection from './SearchSection.svelte'` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error: Section.svelte is only a suffix of the imported SearchSection.svelte")
	}
	if !strings.Contains(err.Error(), "lib/settings/Section.svelte") {
		t.Errorf("expected failure to name Section.svelte, got: %s", err.Error())
	}
	if strings.Contains(err.Error(), "SearchSection.svelte") {
		t.Errorf("SearchSection.svelte is imported and must not be reported: %s", err.Error())
	}
}

// A name mentioned in a doc comment, a describe() title, or a commented-out
// import is text, not coverage.
func TestA11yCoverage_MentionInCommentOrStringIsNotAMatch(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Alpha.svelte": "<div />",
		"apps/desktop/src/lib/settings/Ghost.svelte": "<div />",
		"apps/desktop/src/lib/settings/all.a11y.test.ts": "/** Covers Ghost.svelte too, honest. */\n" +
			a11yHelperImport +
			`import Alpha from './Alpha.svelte'` + "\n" +
			`// import Ghost from './Ghost.svelte'` + "\n" +
			`describe('Ghost.svelte a11y', () => {})` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error: Ghost.svelte is only mentioned, never imported")
	}
	if !strings.Contains(err.Error(), "Ghost.svelte") {
		t.Errorf("expected failure to name Ghost.svelte, got: %s", err.Error())
	}
}

// An import from a sibling directory happens to share the basename; it must not
// satisfy the component that lives here.
func TestA11yCoverage_ImportFromAnotherDirectoryIsNotAMatch(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Row.svelte":          "<div />",
		"apps/desktop/src/lib/settings/Anchor.svelte":       "<div />",
		"apps/desktop/src/lib/settings/Anchor.a11y.test.ts": a11yHelperImport,
		"apps/desktop/src/lib/shared/Row.svelte":            "<div />",
		"apps/desktop/src/lib/shared/Row.a11y.test.ts":      a11yHelperImport,
		"apps/desktop/src/lib/settings/all.a11y.test.ts": a11yHelperImport +
			`import Row from '../shared/Row.svelte'` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error: settings/Row.svelte is not the imported shared/Row.svelte")
	}
	if !strings.Contains(err.Error(), "lib/settings/Row.svelte") {
		t.Errorf("expected failure to name settings/Row.svelte, got: %s", err.Error())
	}
}

// A $lib-absolute specifier resolving to this directory counts, same as './'.
func TestA11yCoverage_LibAliasSpecifierCounts(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Alpha.svelte": "<div />",
		"apps/desktop/src/lib/settings/all.a11y.test.ts": a11yHelperImport +
			`import Alpha from '$lib/settings/Alpha.svelte'` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if !strings.Contains(result.Message, "1 component(s) covered") {
		t.Errorf("expected 1 covered, got: %s", result.Message)
	}
}

// Importing the component is not enough: the file must exercise the a11y helper.
func TestA11yCoverage_DirectoryFileWithoutHelperImportDoesNotCount(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Alpha.svelte": "<div />",
		"apps/desktop/src/lib/settings/all.a11y.test.ts": `import Alpha from './Alpha.svelte'` + "\n" +
			`describe('Alpha', () => {})` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error: the directory test file never imports $lib/test-a11y")
	}
	if !strings.Contains(err.Error(), "Alpha.svelte") {
		t.Errorf("expected failure to name Alpha.svelte, got: %s", err.Error())
	}
}

// Side-effect and namespace import forms resolve like any other.
func TestA11yCoverage_ImportFormsAreParsed(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Alpha.svelte": "<div />",
		"apps/desktop/src/lib/settings/Beta.svelte":  "<div />",
		"apps/desktop/src/lib/settings/Gamma.svelte": "<div />",
		"apps/desktop/src/lib/settings/all.a11y.test.ts": a11yHelperImport +
			`import { mount, tick } from 'svelte'` + "\n" +
			`import './Alpha.svelte'` + "\n" +
			`import * as Beta from "./Beta.svelte"` + "\n" +
			`const Gamma = await import('./Gamma.svelte')` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if !strings.Contains(result.Message, "3 component(s) covered") {
		t.Errorf("expected 3 covered, got: %s", result.Message)
	}
}

// An exempt component that a directory-level file now covers makes the
// allowlist entry redundant, same as a colocated file would.
func TestA11yCoverage_DirectoryFileMakesAllowlistEntryRedundant(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/settings/Alpha.svelte": "<div />",
		"apps/desktop/src/lib/settings/all.a11y.test.ts": a11yHelperImport +
			`import Alpha from './Alpha.svelte'` + "\n",
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{"apps/desktop/src/lib/settings/Alpha.svelte":"too composed"}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunA11yCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for redundant allowlist entry")
	}
	if !strings.Contains(err.Error(), "redundant") {
		t.Errorf("expected 'redundant' in message, got: %s", err.Error())
	}
}
