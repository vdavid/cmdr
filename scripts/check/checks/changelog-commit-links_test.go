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
	// accurate: there are zero refs, so zero bad refs.
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
	longSHA := fullSHA[:10]

	content := `# Changelog

- First thing (` + shortSHA + `)
- Second thing (` + longSHA + `)
- Duplicate ref (` + shortSHA + `)
- Two refs on one entry (` + shortSHA + `, ` + longSHA + `)
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
	// 2 unique SHAs, 5 total references.
	if !strings.Contains(result.Message, "2 unique SHAs") {
		t.Errorf("expected '2 unique SHAs' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "5 references") {
		t.Errorf("expected '5 references' in message, got: %s", result.Message)
	}
}

func TestRunChangelogCommitLinks_WrappedGroupIsRecognized(t *testing.T) {
	// A long entry wraps its ref group across source lines. The group must still
	// be recognized, and each SHA reported at the line it actually appears on.
	tmp := t.TempDir()
	fullSHA := initTempGitRepo(t, tmp)
	good := fullSHA[:8]
	bad := "deadbeef"

	content := "# Changelog\n\n" +
		"- An entry long enough that its commit refs wrap onto the next source line (" + good + ",\n" +
		"  " + bad + ")\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for the non-resolving wrapped SHA, got success")
	}
	if !strings.Contains(err.Error(), bad) {
		t.Errorf("expected error to mention %q, got: %v", bad, err)
	}
	// The bad SHA sits on line 4, the continuation line, not on the entry's first line.
	if !strings.Contains(err.Error(), "CHANGELOG.md:4") {
		t.Errorf("expected error to cite line 4, got: %v", err)
	}
}

func TestRunChangelogCommitLinks_BadSHA(t *testing.T) {
	tmp := t.TempDir()
	initTempGitRepo(t, tmp)

	// A hex SHA that definitely won't resolve in a fresh one-commit repo.
	badSHA := "deadbeef1234"
	content := "# Changelog\n\n- Bad ref (" + badSHA + ")\n"
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

func TestRunChangelogCommitLinks_RejectsLinkedForm(t *testing.T) {
	// The changelog carries bare hashes; the renderers (website, in-app What's new)
	// linkify them. A hand-written markdown link is the deprecated form and must fail
	// loudly rather than bloat the file again.
	tmp := t.TempDir()
	fullSHA := initTempGitRepo(t, tmp)
	sha := fullSHA[:8]

	content := "# Changelog\n\n- Linked ref ([" + sha + "](https://github.com/vdavid/cmdr/commit/" + sha + "))\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for a linked commit ref, got success")
	}
	if !strings.Contains(err.Error(), "bare hash") {
		t.Errorf("expected the error to point at the bare-hash form, got: %v", err)
	}
	if !strings.Contains(err.Error(), "CHANGELOG.md:3") {
		t.Errorf("expected error to cite line 3, got: %v", err)
	}
}

func TestRunChangelogCommitLinks_UnreachableFromHEAD(t *testing.T) {
	// Regression for the v0.13.0 CI-red incident: a SHA that resolves locally
	// (in the object DB via reflog) but is NOT reachable from HEAD must be
	// flagged. Otherwise CI's fresh clone (no reflog) fails on the same ref
	// while local runs pass.
	tmp := t.TempDir()
	seedSHA := initTempGitRepo(t, tmp)

	runGit := func(args ...string) string {
		cmd := exec.Command("git", args...)
		cmd.Dir = tmp
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=test@example.com",
			"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=test@example.com",
		)
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v failed: %v\n%s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}

	// Add a second commit, then hard-reset back to the seed. The second
	// commit's object stays in the DB (reflog + 30-day GC), but HEAD no
	// longer reaches it.
	if err := os.WriteFile(filepath.Join(tmp, "doomed.txt"), []byte("x\n"), 0644); err != nil {
		t.Fatal(err)
	}
	runGit("add", "doomed.txt")
	runGit("commit", "-q", "-m", "doomed")
	doomedSHA := runGit("rev-parse", "HEAD")
	runGit("reset", "--hard", seedSHA)

	// Sanity-check the setup: doomed exists in the object DB but isn't an
	// ancestor. If either assumption changes, this test's premise breaks.
	if err := exec.Command("git", "-C", tmp, "cat-file", "-e", doomedSHA).Run(); err != nil {
		t.Fatalf("doomed commit should still exist in object DB: %v", err)
	}
	if err := exec.Command("git", "-C", tmp, "merge-base", "--is-ancestor", doomedSHA, "HEAD").Run(); err == nil {
		t.Fatal("doomed commit should NOT be ancestor of HEAD")
	}

	content := "# Changelog\n\n- Dangling ref (" + doomedSHA[:7] + ")\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for unreachable-from-HEAD SHA, got success")
	}
	if !strings.Contains(err.Error(), "not reachable from HEAD") {
		t.Errorf("expected 'not reachable from HEAD' in error, got: %v", err)
	}
	if !strings.Contains(err.Error(), doomedSHA[:7]) {
		t.Errorf("expected error to mention dangling SHA %q, got: %v", doomedSHA[:7], err)
	}
}

func TestRunChangelogCommitLinks_IgnoresProse(t *testing.T) {
	// Trailing parentheticals that aren't a pure hash list are ordinary prose.
	// These are all real shapes from the changelog; none may be read as a SHA.
	tmp := t.TempDir()
	initTempGitRepo(t, tmp)

	content := `# Changelog

- Speed up the scan (~40x speed-up!)
- Rename in place (photo.JPG to photo.jpg)
- Bind two shortcuts (⌘⌥R, ⌘⌥C)
- Ship the thing (alpha version!)
- Bump the dep (smb2 0.8.0)
- Flag a hex-looking word mid-sentence (added) and keep going
- Shorten below the floor (abcde)
`
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunChangelogCommitLinks(ctx)
	if err != nil {
		t.Fatalf("expected prose to be ignored, got error: %v", err)
	}
	if !strings.Contains(result.Message, "No commit refs") {
		t.Errorf("expected 'No commit refs' in message, got: %s", result.Message)
	}
}

func TestRunChangelogCommitLinks_NoRefs(t *testing.T) {
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
	if !strings.Contains(result.Message, "No commit refs") {
		t.Errorf("expected 'No commit refs' in message, got: %s", result.Message)
	}
}
