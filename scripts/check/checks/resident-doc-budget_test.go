package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeWords(t *testing.T, dir, relPath string, words int) {
	t.Helper()
	full := filepath.Join(dir, relPath)
	if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
		t.Fatal(err)
	}
	content := strings.TrimSpace(strings.Repeat("word ", words))
	if err := os.WriteFile(full, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestCollectResidentDocs_FollowsImportsAndRules(t *testing.T) {
	tmp := t.TempDir()
	// Root imports AGENTS.md and docs/architecture.md; noise tokens must be ignored.
	writeWords(t, tmp, "CLAUDE.md", 0)
	if err := os.WriteFile(filepath.Join(tmp, "CLAUDE.md"),
		[]byte("@AGENTS.md @docs/architecture.md @iconify-json/lucide @param"), 0644); err != nil {
		t.Fatal(err)
	}
	writeWords(t, tmp, "AGENTS.md", 50)
	writeWords(t, tmp, "docs/architecture.md", 30)
	writeWords(t, tmp, ".claude/rules/a.md", 10)
	writeWords(t, tmp, ".claude/rules/b.md", 20)
	// A nested CLAUDE.md must NOT be part of the resident bundle (not imported).
	writeWords(t, tmp, "lib/CLAUDE.md", 999)

	docs, err := collectResidentDocs(tmp)
	if err != nil {
		t.Fatal(err)
	}
	got := map[string]bool{}
	for _, d := range docs {
		got[d] = true
	}
	for _, want := range []string{"CLAUDE.md", "AGENTS.md", "docs/architecture.md", ".claude/rules/a.md", ".claude/rules/b.md"} {
		if !got[want] {
			t.Errorf("expected %s in resident bundle, got %v", want, docs)
		}
	}
	if got["lib/CLAUDE.md"] {
		t.Errorf("non-imported nested CLAUDE.md must not be resident, got %v", docs)
	}
	if got["iconify-json/lucide"] || got["param"] {
		t.Errorf("noise @-tokens must not resolve as imports, got %v", docs)
	}
}

func TestRunResidentDocBudget_UnderCap(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "CLAUDE.md"), []byte("@AGENTS.md"), 0644); err != nil {
		t.Fatal(err)
	}
	writeWords(t, tmp, "AGENTS.md", 100)
	writeWords(t, tmp, ".claude/rules/r.md", 50)

	result, err := RunResidentDocBudget(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success well under cap, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunResidentDocBudget_OverCapWarns(t *testing.T) {
	tmp := t.TempDir()
	if err := os.WriteFile(filepath.Join(tmp, "CLAUDE.md"), []byte("@AGENTS.md"), 0644); err != nil {
		t.Fatal(err)
	}
	// One word past the cap (root is 1 "word": "@AGENTS.md").
	writeWords(t, tmp, "AGENTS.md", residentDocBudgetWords)

	result, err := RunResidentDocBudget(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("expected warning over cap, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "over the") {
		t.Errorf("expected over-cap message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "AGENTS.md") {
		t.Errorf("expected the largest file named in the breakdown, got: %s", result.Message)
	}
}

func TestRunResidentDocBudget_TransitiveImport(t *testing.T) {
	tmp := t.TempDir()
	// CLAUDE.md -> AGENTS.md -> docs/deep.md (transitive).
	if err := os.WriteFile(filepath.Join(tmp, "CLAUDE.md"), []byte("@AGENTS.md"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(tmp, "AGENTS.md"), []byte("text @docs/deep.md"), 0644); err != nil {
		t.Fatal(err)
	}
	writeWords(t, tmp, "docs/deep.md", 5)

	docs, err := collectResidentDocs(tmp)
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, d := range docs {
		if d == "docs/deep.md" {
			found = true
		}
	}
	if !found {
		t.Errorf("expected transitive import docs/deep.md in bundle, got %v", docs)
	}
}
