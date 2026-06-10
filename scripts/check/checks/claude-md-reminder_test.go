package checks

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// initReminderRepo seeds a repo with a `main` branch holding one CLAUDE.md and
// some source files, then leaves HEAD on a fresh branch the test can mutate.
func initReminderRepo(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()

	run := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=test@example.com",
			"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=test@example.com",
		)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	write := func(rel, body string) {
		full := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(body), 0644); err != nil {
			t.Fatal(err)
		}
	}

	run("init", "-q", "-b", "main")
	write("apps/desktop/CLAUDE.md", "# Desktop\n")
	write("apps/desktop/src/lib.rs", "fn main() {}\n")
	write("apps/api/server.go", "package api\n")
	run("add", ".")
	run("commit", "-q", "-m", "seed")
	return dir
}

func runReminder(t *testing.T, dir string) CheckResult {
	t.Helper()
	res, err := RunClaudeMdReminder(&CheckContext{RootDir: dir})
	if err != nil {
		t.Fatalf("check returned error: %v", err)
	}
	return res
}

func TestReminder_NoChanges(t *testing.T) {
	dir := initReminderRepo(t)
	res := runReminder(t, dir)
	if res.Code != ResultSuccess {
		t.Fatalf("expected success, got %v: %s", res.Code, res.Message)
	}
}

func TestReminder_SourceChangeWithoutDocUpdate_Warns(t *testing.T) {
	dir := initReminderRepo(t)
	if err := os.WriteFile(filepath.Join(dir, "apps/desktop/src/lib.rs"), []byte("fn changed() {}\n"), 0644); err != nil {
		t.Fatal(err)
	}
	res := runReminder(t, dir)
	if res.Code != ResultWarning {
		t.Fatalf("expected warning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "apps/desktop/") {
		t.Errorf("expected message to mention apps/desktop/, got: %s", res.Message)
	}
	if !strings.Contains(res.Message, "Just a friendly reminder") {
		t.Errorf("expected friendly tone, got: %s", res.Message)
	}
}

func TestReminder_SourceChangeWithDocUpdate_Passes(t *testing.T) {
	dir := initReminderRepo(t)
	if err := os.WriteFile(filepath.Join(dir, "apps/desktop/src/lib.rs"), []byte("fn changed() {}\n"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "apps/desktop/CLAUDE.md"), []byte("# Desktop\n\nUpdated.\n"), 0644); err != nil {
		t.Fatal(err)
	}
	res := runReminder(t, dir)
	if res.Code != ResultSuccess {
		t.Fatalf("expected success when doc was updated alongside source, got %v: %s", res.Code, res.Message)
	}
}

func TestReminder_SourceChangeWithDetailsUpdate_Passes(t *testing.T) {
	// Pre-fix this warned: only a CLAUDE.md touch counted as a doc update, so
	// documenting a change in the pull-tier DETAILS.md still triggered the nag.
	dir := initReminderRepo(t)
	if err := os.WriteFile(filepath.Join(dir, "apps/desktop/src/lib.rs"), []byte("fn changed() {}\n"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "apps/desktop/DETAILS.md"), []byte("# Desktop details\n\nUpdated.\n"), 0644); err != nil {
		t.Fatal(err)
	}
	res := runReminder(t, dir)
	if res.Code != ResultSuccess {
		t.Fatalf("expected success when DETAILS.md was updated alongside source, got %v: %s", res.Code, res.Message)
	}
}

func TestReminder_NonSourceFileChange_Ignored(t *testing.T) {
	dir := initReminderRepo(t)
	// Markdown / json changes shouldn't trigger the reminder.
	if err := os.WriteFile(filepath.Join(dir, "apps/desktop/notes.md"), []byte("hi\n"), 0644); err != nil {
		t.Fatal(err)
	}
	res := runReminder(t, dir)
	if res.Code != ResultSuccess {
		t.Fatalf("expected success for non-source change, got %v: %s", res.Code, res.Message)
	}
}

func TestReminder_FileOutsideAnyClaudeMd_Ignored(t *testing.T) {
	dir := initReminderRepo(t)
	// apps/api has no CLAUDE.md anywhere on the path; changes there don't trigger.
	if err := os.WriteFile(filepath.Join(dir, "apps/api/server.go"), []byte("package api\n\nvar X = 1\n"), 0644); err != nil {
		t.Fatal(err)
	}
	res := runReminder(t, dir)
	if res.Code != ResultSuccess {
		t.Fatalf("expected success when changed file has no enclosing CLAUDE.md, got %v: %s", res.Code, res.Message)
	}
}

func TestReminder_NearestClaudeWins(t *testing.T) {
	// A file under a deeper CLAUDE.md should attribute to the deeper one,
	// not the ancestor.
	dir := initReminderRepo(t)
	run := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=test@example.com",
			"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=test@example.com",
		)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}

	// Commit the deeper CLAUDE.md and a baseline source file so they're not
	// counted as "untracked changes" by the next run.
	deepDir := filepath.Join(dir, "apps/desktop/src/feature")
	if err := os.MkdirAll(deepDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(deepDir, "CLAUDE.md"), []byte("# Feature\n"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(deepDir, "code.rs"), []byte("fn x() {}\n"), 0644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-q", "-m", "add deeper feature module")

	// Now mutate only the source file. The doc is unchanged.
	if err := os.WriteFile(filepath.Join(deepDir, "code.rs"), []byte("fn changed() {}\n"), 0644); err != nil {
		t.Fatal(err)
	}

	res := runReminder(t, dir)
	if res.Code != ResultWarning {
		t.Fatalf("expected warning, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "apps/desktop/src/feature/") {
		t.Errorf("expected attribution to the deeper CLAUDE.md, got: %s", res.Message)
	}
	// The ancestor `apps/desktop/` CLAUDE.md should not be reported as a miss
	// since no source file directly under it (excluding the deeper-CLAUDE-covered
	// subtree) was changed. Match on the bullet form to avoid catching the
	// substring inside the deeper path.
	if strings.Contains(res.Message, "- apps/desktop/ ") {
		t.Errorf("ancestor CLAUDE.md should not be reported, got: %s", res.Message)
	}
}

func TestReminder_BranchVsMain_DetectsCommittedChanges(t *testing.T) {
	dir := initReminderRepo(t)
	run := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=test@example.com",
			"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=test@example.com",
		)
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	run("checkout", "-q", "-b", "feature")
	if err := os.WriteFile(filepath.Join(dir, "apps/desktop/src/lib.rs"), []byte("fn branched() {}\n"), 0644); err != nil {
		t.Fatal(err)
	}
	run("add", "apps/desktop/src/lib.rs")
	run("commit", "-q", "-m", "branched change")

	// Working tree is clean now, but the diff vs main should still trigger.
	res := runReminder(t, dir)
	if res.Code != ResultWarning {
		t.Fatalf("expected warning from branch-vs-main diff, got %v: %s", res.Code, res.Message)
	}
}

func TestParsePorcelainZ_RenameKeepsBothPaths(t *testing.T) {
	// Format: "R  new\x00orig\x00"
	in := "R  new/path.rs\x00orig/path.rs\x00 M other.go\x00"
	got := parsePorcelainZ(in)
	want := []string{"new/path.rs", "orig/path.rs", "other.go"}
	if len(got) != len(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
	for i, p := range got {
		if p != want[i] {
			t.Errorf("path %d: got %q, want %q", i, p, want[i])
		}
	}
}
