package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runDiscardedOutcomeOn writes the supplied files into a temp repo layout rooted
// at `apps/desktop/src-tauri/src/` and runs the check over them.
func runDiscardedOutcomeOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	for rel, body := range files {
		full := filepath.Join(srcDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunDiscardedOutcome(&CheckContext{RootDir: root})
}

func expectDiscardedOutcomeClean(t *testing.T, files map[string]string) {
	t.Helper()
	res, err := runDiscardedOutcomeOn(t, files)
	if err != nil {
		t.Fatalf("expected no violation, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

// The three shapes that actually shipped, each reconstructed from the commit
// that fixed it. A check that can't catch the bugs it was written for is
// decoration.
func TestDiscardedOutcome_CatchesTheShapesThatShipped(t *testing.T) {
	cases := map[string]map[string]string{
		// `resolve_write_conflict`: the IPC command dropped the manager's typed
		// verdict, so the frontend couldn't tell a stale answer from a real one.
		"an ipc command drops a typed verdict": {
			"manager.rs": "pub fn resolve_conflict(id: &str) -> ConflictResolutionOutcome { inner(id) }\n",
			"commands.rs": "#[tauri::command]\n" +
				"pub fn resolve_write_conflict(id: String) {\n" +
				"    resolve_conflict(&id);\n" +
				"}\n",
		},
		// `pause_operation`: same shape, one layer down.
		"a thin wrapper drops a typed outcome": {
			"manager.rs": "pub fn set_paused(id: &str, paused: bool) -> PauseOutcome { inner(id, paused) }\n",
			"api.rs":     "pub fn pause_operation(id: &str) {\n    set_paused(id, true);\n}\n",
		},
		// `pause_all`: the sweep swallowed each per-op answer, so the MCP tool
		// above it invented one.
		"a sweep drops every per-item outcome": {
			"api.rs": "pub fn pause_operation(id: &str) -> PauseOutcome { flip(id) }\n" +
				"pub fn pause_all() {\n" +
				"    for id in running_ids() {\n" +
				"        pause_operation(&id);\n" +
				"    }\n" +
				"}\n",
		},
		// A plain `bool` is the case the compiler is blind to.
		"a bool answer": {
			"store.rs": "pub fn did_change(id: &str) -> bool { true }\n" +
				"pub fn apply(id: &str) {\n    did_change(id);\n}\n",
		},
	}
	for name, files := range cases {
		t.Run(name, func(t *testing.T) {
			_, err := runDiscardedOutcomeOn(t, files)
			if err == nil {
				t.Fatal("expected a violation, got success")
			}
		})
	}
}

func TestDiscardedOutcome_ReportsCallerCalleeAndType(t *testing.T) {
	_, err := runDiscardedOutcomeOn(t, map[string]string{
		"api.rs": "pub fn set_paused(id: &str) -> PauseOutcome { flip(id) }\n" +
			"pub fn pause_operation(id: &str) {\n    set_paused(id);\n}\n",
	})
	if err == nil {
		t.Fatal("expected a violation, got success")
	}
	for _, want := range []string{"api.rs", "pause_operation", "set_paused", "PauseOutcome"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected %q in the message, got: %s", want, err.Error())
		}
	}
}

// The false-positive story, which is the check's whole risk. Each of these is a
// discard somebody would rightly refuse to change.
func TestDiscardedOutcome_LeavesTheHonestDiscardsAlone(t *testing.T) {
	cases := map[string]map[string]string{
		// `Result` is `#[must_use]`, so the compiler already warns. Flagging it
		// too would mean two voices for one problem.
		"a Result the compiler already warns about": {
			"api.rs": "pub fn write(p: &Path) -> Result<(), Error> { inner(p) }\n" +
				"pub fn save(p: &Path) {\n    write(p);\n}\n",
		},
		// The map/set idiom: `insert` handing back what it displaced.
		"an Option nobody wants": {
			"api.rs": "pub fn take_previous(k: &str) -> Option<String> { inner(k) }\n" +
				"pub fn put(k: &str) {\n    take_previous(k);\n}\n",
		},
		// The value goes somewhere: bound, propagated, matched, or chained.
		"a value that is actually used": {
			"api.rs": "pub fn verdict(id: &str) -> PauseOutcome { flip(id) }\n" +
				"pub fn a(id: &str) {\n    let outcome = verdict(id);\n    log(outcome);\n}\n" +
				"pub fn b(id: &str) {\n    match verdict(id) {\n        _ => {}\n    }\n}\n" +
				"pub fn c(id: &str) {\n    verdict(id).report();\n}\n",
		},
		// The caller passes the answer on, which is the fix, not the defect.
		"a caller that returns the answer": {
			"api.rs": "pub fn verdict(id: &str) -> PauseOutcome { flip(id) }\n" +
				"pub fn pause(id: &str) -> PauseOutcome {\n    verdict(id)\n}\n",
		},
		// Nothing to lose.
		"a unit-returning callee": {
			"api.rs": "pub fn notify(id: &str) {}\n" +
				"pub fn run(id: &str) {\n    notify(id);\n}\n",
		},
		// Two definitions disagreeing means the name can't be resolved without a
		// compiler, and a guess is how a check earns its reputation for noise.
		"an ambiguous name": {
			"one.rs": "pub fn apply(id: &str) -> bool { true }\n",
			"two.rs": "pub fn apply(id: &str) {}\n",
			"use.rs": "pub fn run(id: &str) {\n    apply(id);\n}\n",
		},
		// A method call is not a free-function call; the index knows nothing
		// about receivers.
		"a method call": {
			"api.rs": "pub fn verdict(id: &str) -> bool { true }\n" +
				"pub fn run(&self, id: &str) {\n    self.verdict(id);\n}\n",
		},
		// Test code discards on purpose, constantly, and misleads nobody.
		"a discard inside a test module": {
			"api.rs": "pub fn verdict(id: &str) -> PauseOutcome { flip(id) }\n" +
				"#[cfg(test)]\nmod tests {\n" +
				"    use super::*;\n" +
				"    #[test]\n    fn t() {\n        verdict(\"x\");\n    }\n" +
				"}\n",
		},
		// A `where` bound carries arrows of its own; reading one as the return
		// type turns every unit function into a `bool` one.
		"a where-clause arrow is not a return type": {
			"api.rs": "pub fn mutate<F>(f: F)\nwhere\n    F: FnOnce(&mut Store) -> bool,\n{\n    inner(f);\n}\n" +
				"pub fn add(id: &str) {\n    mutate(|s| true);\n}\n",
		},
		// Same trap inline: the bound's `(` would otherwise read as the parameter
		// list, and its `>` would close the generics early.
		"an inline bound with its own parens and arrow": {
			"api.rs": "pub fn mutate<F: FnOnce(&mut Store) -> bool>(f: F) {\n    inner(f);\n}\n" +
				"pub fn add(id: &str) {\n    mutate(|s| true);\n}\n",
		},
		// A call named only in prose is not a call.
		"a mention in a comment or a string": {
			"api.rs": "pub fn verdict(id: &str) -> bool { true }\n" +
				"pub fn run(id: &str) {\n" +
				"    // Don't write `verdict(id);` here.\n" +
				"    let s = \"verdict(id);\";\n" +
				"}\n",
		},
	}
	for name, files := range cases {
		t.Run(name, func(t *testing.T) { expectDiscardedOutcomeClean(t, files) })
	}
}

func TestDiscardedOutcome_HonorsTheOptOutDirective(t *testing.T) {
	expectDiscardedOutcomeClean(t, map[string]string{
		"api.rs": "pub fn verdict(id: &str) -> PauseOutcome { flip(id) }\n" +
			"pub fn apply(id: &str) {\n" +
			"    // allowed-discarded-outcome: this applies a decision already reported to whoever asked.\n" +
			"    verdict(id);\n" +
			"}\n",
	})
}

// An opt-out that excuses nothing is a claim about the code that stopped being
// true. Same treatment ESLint gives an unused disable comment.
func TestDiscardedOutcome_ReportsAnOrphanedOptOut(t *testing.T) {
	_, err := runDiscardedOutcomeOn(t, map[string]string{
		"api.rs": "pub fn verdict(id: &str) -> PauseOutcome { flip(id) }\n" +
			"pub fn apply(id: &str) {\n" +
			"    // allowed-discarded-outcome: stale, the discard is long gone.\n" +
			"    let outcome = verdict(id);\n" +
			"    report(outcome);\n" +
			"}\n",
	})
	if err == nil {
		t.Fatal("expected the orphaned opt-out to fail the check, got success")
	}
	if !strings.Contains(err.Error(), "unused") {
		t.Errorf("expected the orphan report, got: %s", err.Error())
	}
}

func TestDiscardedOutcome_FollowsAUseAlias(t *testing.T) {
	_, err := runDiscardedOutcomeOn(t, map[string]string{
		"manager.rs": "pub fn pause_all() -> PauseAllOutcome { sweep() }\n",
		"commands.rs": "use crate::file_system::{pause_all as ops_pause_all};\n" +
			"#[tauri::command]\n" +
			"pub fn pause_all() {\n    ops_pause_all();\n}\n",
	})
	if err == nil {
		t.Fatal("an aliased import must still resolve to the real function, got success")
	}
	if !strings.Contains(err.Error(), "PauseAllOutcome") {
		t.Errorf("expected the resolved return type in the message, got: %s", err.Error())
	}
}
