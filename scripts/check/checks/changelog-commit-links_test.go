package checks

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// runGitIn runs git in dir with a local identity, so `git commit` works in CI and
// sandboxes that have no global config.
func runGitIn(t *testing.T, dir string, args ...string) string {
	t.Helper()
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

// initTempGitRepo initializes a git repo in dir and returns a known-good commit SHA
// (the full 40-char hash of the repo's first commit). The test harness uses this
// to drive the happy-path test without depending on any specific SHA.
func initTempGitRepo(t *testing.T, dir string) (fullSHA string) {
	t.Helper()
	runGitIn(t, dir, "init", "-q", "-b", "main")
	return addTempCommit(t, dir, "seed.txt")
}

// addTempCommit writes a file and commits it, returning the new commit's full SHA.
func addTempCommit(t *testing.T, dir, name string) (fullSHA string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(name+"\n"), 0644); err != nil {
		t.Fatal(err)
	}
	runGitIn(t, dir, "add", name)
	runGitIn(t, dir, "commit", "-q", "-m", name)
	return runGitIn(t, dir, "rev-parse", "HEAD")
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
	// A one-commit repo can't produce two distinct 8-char refs, so the second ref is
	// a second commit. Both are 8 characters, the only length the check accepts.
	otherSHA := addTempCommit(t, tmp, "second.txt")
	refA := fullSHA[:8]
	refB := otherSHA[:8]

	content := `# Changelog

- First thing (` + refA + `)
- Second thing (` + refB + `)
- Duplicate ref (` + refA + `)
- Two refs on one entry (` + refA + `, ` + refB + `)
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

func TestRunChangelogCommitLinks_RejectsShortRef(t *testing.T) {
	// The convention is exactly 8. A 7-char ref resolves fine, so only the length
	// rule catches it; recognition deliberately stays loose so it can be caught at
	// all instead of being silently read as prose.
	tmp := t.TempDir()
	fullSHA := initTempGitRepo(t, tmp)
	short := fullSHA[:7]

	writeChangelog(t, tmp, "# Changelog\n\n- Short ref ("+short+")\n")

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for a 7-character ref, got success")
	}
	if !strings.Contains(err.Error(), short) {
		t.Errorf("expected error to mention %q, got: %v", short, err)
	}
	if !strings.Contains(err.Error(), "8 characters") {
		t.Errorf("expected error to state the 8-character rule, got: %v", err)
	}
	if !strings.Contains(err.Error(), "CHANGELOG.md:3") {
		t.Errorf("expected error to cite line 3, got: %v", err)
	}
}

func TestRunChangelogCommitLinks_RejectsLongRef(t *testing.T) {
	// A 9-char ref also resolves, and is just as much a convention break as a
	// 7-char one: the plugin's `{8}` pattern renders neither.
	tmp := t.TempDir()
	fullSHA := initTempGitRepo(t, tmp)
	long := fullSHA[:9]

	writeChangelog(t, tmp, "# Changelog\n\n- Long ref ("+long+")\n")

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for a 9-character ref, got success")
	}
	if !strings.Contains(err.Error(), long) {
		t.Errorf("expected error to mention %q, got: %v", long, err)
	}
	if !strings.Contains(err.Error(), "8 characters") {
		t.Errorf("expected error to state the 8-character rule, got: %v", err)
	}
}

func TestRunChangelogCommitLinks_LengthFindingCitesWrappedLine(t *testing.T) {
	// A wrong-length ref in a wrapped group is reported at the continuation line it
	// actually sits on, the same as a non-resolving one.
	tmp := t.TempDir()
	fullSHA := initTempGitRepo(t, tmp)

	content := "# Changelog\n\n" +
		"- An entry long enough that its commit refs wrap onto the next source line (" + fullSHA[:8] + ",\n" +
		"  " + fullSHA[:6] + ")\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for the 6-character wrapped ref, got success")
	}
	if !strings.Contains(err.Error(), "CHANGELOG.md:4") {
		t.Errorf("expected error to cite line 4, got: %v", err)
	}
	if strings.Contains(err.Error(), "CHANGELOG.md:3") {
		t.Errorf("the 8-character ref on line 3 must not be flagged, got: %v", err)
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

	// A hex SHA that definitely won't resolve in a fresh one-commit repo. Kept at 8
	// characters so the failure is unambiguously about resolution, not length.
	badSHA := "deadbeef"
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

	// Add a second commit, then hard-reset back to the seed. The second
	// commit's object stays in the DB (reflog + 30-day GC), but HEAD no
	// longer reaches it.
	doomedSHA := addTempCommit(t, tmp, "doomed.txt")
	runGitIn(t, tmp, "reset", "--hard", seedSHA)

	// Sanity-check the setup: doomed exists in the object DB but isn't an
	// ancestor. If either assumption changes, this test's premise breaks.
	if err := exec.Command("git", "-C", tmp, "cat-file", "-e", doomedSHA).Run(); err != nil {
		t.Fatalf("doomed commit should still exist in object DB: %v", err)
	}
	if err := exec.Command("git", "-C", tmp, "merge-base", "--is-ancestor", doomedSHA, "HEAD").Run(); err == nil {
		t.Fatal("doomed commit should NOT be ancestor of HEAD")
	}

	content := "# Changelog\n\n- Dangling ref (" + doomedSHA[:8] + ")\n"
	writeChangelog(t, tmp, content)

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunChangelogCommitLinks(ctx)
	if err == nil {
		t.Fatal("expected failure for unreachable-from-HEAD SHA, got success")
	}
	if !strings.Contains(err.Error(), "not reachable from HEAD") {
		t.Errorf("expected 'not reachable from HEAD' in error, got: %v", err)
	}
	if !strings.Contains(err.Error(), doomedSHA[:8]) {
		t.Errorf("expected error to mention dangling SHA %q, got: %v", doomedSHA[:8], err)
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
- Shorten below the recognition floor (abcde)
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
