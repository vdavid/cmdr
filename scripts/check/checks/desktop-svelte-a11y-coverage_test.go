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
		"apps/desktop/src/lib/ui/Button.a11y.test.ts":       "// empty — doesn't import the helper",
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
		// No component at the allowlisted path — it was deleted/moved.
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

func TestA11yCoverage_IgnoresUntrackedFiles(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":             "<button>Click</button>",
		"apps/desktop/src/lib/ui/Button.a11y.test.ts":       `import { expectNoA11yViolations } from '$lib/test-a11y'`,
		"scripts/check/checks/a11y-coverage-allowlist.json": `{"exempt":{}}`,
	})

	// Create an untracked new svelte file with no test — should be ignored.
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

	// Don't write an allowlist file — should default to empty.
	ctx := &CheckContext{RootDir: tmp}
	result, err := RunA11yCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success (no svelte files in scope), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d", result.Code)
	}
}
