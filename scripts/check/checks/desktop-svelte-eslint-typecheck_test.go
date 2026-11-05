package checks

import (
	"errors"
	"maps"
	"testing"
)

// A lane's snapshot covers only the files its own fix pass can rewrite: the Svelte and
// TypeScript lanes fix in parallel, so the other lane's rewrite must not count as a change.
func TestLintScopeSnapshotSeesOnlyTheLanesFiles(t *testing.T) {
	dir := initFpGitRepo(t, map[string]string{
		"apps/desktop/src/a.svelte": "<p>a</p>",
		"apps/desktop/src/b.ts":     "export const b = 1",
		"apps/website/c.ts":         "export const c = 1",
	})
	snap := func(scope []string) map[string]string {
		t.Helper()
		s, err := lintScopeSnapshot(dir, scope)
		if err != nil {
			t.Fatalf("lintScopeSnapshot: %v", err)
		}
		return s
	}
	svelteBefore, tsBefore := snap(svelteLintScope), snap(typescriptLintScope)

	writeFiles(t, dir, map[string]string{"apps/desktop/src/b.ts": "export const b = 2", "apps/website/c.ts": "x"})
	if !maps.Equal(svelteBefore, snap(svelteLintScope)) {
		t.Error("a .ts edit must not change the Svelte lane's snapshot")
	}
	if maps.Equal(tsBefore, snap(typescriptLintScope)) {
		t.Error("a .ts edit must change the TypeScript lane's snapshot")
	}

	svelteBefore, tsBefore = snap(svelteLintScope), snap(typescriptLintScope)
	writeFiles(t, dir, map[string]string{"apps/desktop/src/a.svelte": "<p>fixed</p>"})
	if maps.Equal(svelteBefore, snap(svelteLintScope)) {
		t.Error("a .svelte edit must change the Svelte lane's snapshot")
	}
	if !maps.Equal(tsBefore, snap(typescriptLintScope)) {
		t.Error("a .svelte edit must not change the TypeScript lane's snapshot")
	}

	// A file that was already dirty and gets rewritten again still counts.
	svelteBefore = snap(svelteLintScope)
	writeFiles(t, dir, map[string]string{"apps/desktop/src/a.svelte": "<p>fixed twice</p>"})
	if maps.Equal(svelteBefore, snap(svelteLintScope)) {
		t.Error("rewriting an already-dirty file must change the snapshot")
	}
}

// fakeLintRun records how often the lane's verdict and fix pass ran, and lets a test decide
// whether the fix pass rewrote anything.
type fakeLintRun struct {
	verdicts, fixes int
	fixChanges      bool
	snapshotErr     error
	state           string
}

func (f *fakeLintRun) verdict() lintOutcome {
	f.verdicts++
	return lintOutcome{output: "second verdict", err: errors.New("still red")}
}

func (f *fakeLintRun) fix() {
	f.fixes++
	if f.fixChanges {
		f.state = "fixed"
	}
}

func (f *fakeLintRun) snapshot() (map[string]string, error) {
	return map[string]string{"apps/desktop/src/a.svelte": f.state}, f.snapshotErr
}

var firstRedVerdict = lintOutcome{output: "first verdict", err: errors.New("red")}

// Type-aware findings never auto-fix, so on most red runs the fix pass rewrites nothing and a
// second verdict would lint the same content again for the same answer.
func TestFixThenReverdictSkipsTheSecondVerdictWhenTheFixChangedNothing(t *testing.T) {
	f := &fakeLintRun{}
	got, reran := fixThenReverdict(firstRedVerdict, f.fix, f.verdict, f.snapshot)
	if f.fixes != 1 || f.verdicts != 0 || reran {
		t.Fatalf("want the fix pass and no second verdict, got fixes=%d verdicts=%d reran=%v", f.fixes, f.verdicts, reran)
	}
	if got != firstRedVerdict {
		t.Errorf("want the first verdict back, got %+v", got)
	}
}

func TestFixThenReverdictRerunsTheVerdictWhenTheFixChangedAFile(t *testing.T) {
	f := &fakeLintRun{fixChanges: true}
	got, reran := fixThenReverdict(firstRedVerdict, f.fix, f.verdict, f.snapshot)
	if f.verdicts != 1 || !reran || got.output != "second verdict" {
		t.Fatalf("want one second verdict, got verdicts=%d reran=%v out=%q", f.verdicts, reran, got.output)
	}
}

// Fail safe: when the snapshot can't be taken, nobody knows whether the fix pass changed
// anything, so the verdict runs again.
func TestFixThenReverdictRerunsWhenTheSnapshotFails(t *testing.T) {
	f := &fakeLintRun{snapshotErr: errors.New("git broke")}
	if _, reran := fixThenReverdict(firstRedVerdict, f.fix, f.verdict, f.snapshot); !reran || f.verdicts != 1 {
		t.Fatalf("a failed snapshot must re-run the verdict, got verdicts=%d reran=%v", f.verdicts, reran)
	}
}
