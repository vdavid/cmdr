package checks

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// initTempGitRepo initializes a git repo in dir and returns a known-good commit SHA
// (the full 40-char hash of the repo's first commit). The test harness uses this
// to drive the happy-path test without depending on any specific SHA.
func initTempGitRepo(t *testing.T, dir string) (fullSHA string) {
	t.Helper()

	// Local identity so `git commit` works in CI/sandboxes without global config.
	runGit := func(args ...string) string {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=Test",
			"GIT_AUTHOR_EMAIL=test@example.com",
			"GIT_COMMITTER_NAME=Test",
			"GIT_COMMITTER_EMAIL=test@example.com",
		)
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v failed: %v\n%s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}

	runGit("init", "-q", "-b", "main")
	// Create a file and commit.
	if err := os.WriteFile(filepath.Join(dir, "seed.txt"), []byte("hello\n"), 0644); err != nil {
		t.Fatal(err)
	}
	runGit("add", "seed.txt")
	runGit("commit", "-q", "-m", "seed")
	return runGit("rev-parse", "HEAD")
}

func writeChangelog(t *testing.T, dir, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, "CHANGELOG.md"), []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestRunChangelogCommitLinks_MissingChangelogIsSuccess(t *testing.T) {
	// Decision: treat missing CHANGELOG.md as success rather than skip. Skipped
	// reads as "something's wrong, can't check" in the runner UI; success is more
	// accurate — there are zero links, so zero bad links.
	tmp := t.TempDir()
	initTempGitRepo(t, tmp)

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunChangelogCommitLinks(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "No CHANGELOG.md") {
		t.Errorf("expected 'No CHANGELOG.md' in message, got: %s", result.Message)
	}
}

func TestRunChangelogCommitLinks_HappyPath(t *testing.T) {
	tmp := t.TempDir()
	fullSHA := initTempGitRepo(t, tmp)
	shortSHA := fullSHA[:7]

	content := `# Changelog

- First thing ([` + shortSHA + `](https://github.com/vdavid/cmdr/commit/` + shortSHA + `))
- Second thing ([` + fullSHA[:10] + `](https://github.com/vdavid/cmdr/commit/` + fullSHA[:10] + `))
- Duplicate link ([` + shortSHA + `](https://github.com/vdavid/cmdr/commit/` + shortSHA + `))
`
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunChangelogCommitLinks(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	// 2 unique SHAs, 3 total links.
	if !strings.Contains(result.Message, "2 unique SHAs") {
		t.Errorf("expected '2 unique SHAs' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "3 links") {
		t.Errorf("expected '3 links' in message, got: %s", result.Message)
	}
}

func TestRunChangelogCommitLinks_BadURLSHA(t *testing.T) {
	tmp := t.TempDir()
	initTempGitRepo(t, tmp)

	// A hex SHA that definitely won't resolve in a fresh one-commit repo.
	badSHA := "deadbeef1234"
	content := "# Changelog\n\n- Bad link ([" + badSHA + "](https://github.com/vdavid/cmdr/commit/" + badSHA + "))\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for non-resolving SHA, got success")
	}
	if !strings.Contains(err.Error(), badSHA) {
		t.Errorf("expected error to mention bad SHA %q, got: %v", badSHA, err)
	}
	if !strings.Contains(err.Error(), "CHANGELOG.md:3") {
		t.Errorf("expected error to cite line 3, got: %v", err)
	}
}

func TestRunChangelogCommitLinks_TextURLMismatch(t *testing.T) {
	tmp := t.TempDir()
	fullSHA := initTempGitRepo(t, tmp)
	// Use the real SHA in the URL so the URL resolves — the failure should come
	// purely from the text/URL mismatch, not from a missing SHA.
	urlSHA := fullSHA[:7]
	textSHA := "def4567" // totally different prefix

	content := "# Changelog\n\n- Mismatch ([" + textSHA + "](https://github.com/vdavid/cmdr/commit/" + urlSHA + "))\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for text/URL mismatch, got success")
	}
	if !strings.Contains(err.Error(), "mismatch") {
		t.Errorf("expected 'mismatch' in error, got: %v", err)
	}
}

func TestRunChangelogCommitLinks_ShortSHAFlagged(t *testing.T) {
	tmp := t.TempDir()
	initTempGitRepo(t, tmp)

	// 5 chars — below the 6-char minimum the regex accepts.
	content := "# Changelog\n\n- Short ([abcde](https://github.com/vdavid/cmdr/commit/abcde))\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for too-short SHA, got success")
	}
	if !strings.Contains(err.Error(), "too short") {
		t.Errorf("expected 'too short' in error, got: %v", err)
	}
}

func TestRunChangelogCommitLinks_NoLinks(t *testing.T) {
	tmp := t.TempDir()
	initTempGitRepo(t, tmp)
	writeChangelog(t, tmp, "# Changelog\n\nNothing here yet.\n")

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunChangelogCommitLinks(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "No commit links") {
		t.Errorf("expected 'No commit links' in message, got: %s", result.Message)
	}
}
