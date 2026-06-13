package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeDoc writes content to rel under dir, creating parent dirs.
func writeDoc(t *testing.T, dir, rel, content string) {
	t.Helper()
	full := filepath.Join(dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(full, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestScanTwoColTables(t *testing.T) {
	tests := []struct {
		name     string
		content  string
		wantHits int
		wantKind string // substring expected in the first hit's kind (if wantHits > 0)
	}{
		{
			name: "two-column table is flagged",
			content: `# Doc

| Tool | Use for |
| ---- | ------- |
| grep | search  |
| find | locate  |
`,
			wantHits: 1,
			wantKind: "2-column",
		},
		{
			name: "borderless two-column table is flagged",
			content: `# Doc

Tool | Use for
---- | -------
grep | search
`,
			wantHits: 1,
			wantKind: "2-column",
		},
		{
			name: "three-column table with sequential index first column is flagged",
			content: `# Doc

| # | Step      | Detail        |
| - | --------- | ------------- |
| 1 | Checkout  | clone the repo |
| 2 | Build     | run the build  |
| 3 | Test      | run the tests  |
`,
			wantHits: 1,
			wantKind: "index",
		},
		{
			name: "three-column table with empty index cells is flagged",
			content: `# Doc

| Idx | Name | Note |
| --- | ---- | ---- |
|     | foo  | a    |
|     | bar  | b    |
`,
			wantHits: 1,
			wantKind: "index",
		},
		{
			name: "genuine three-column matrix passes",
			content: `# Doc

| Backend | Read | Write |
| ------- | ---- | ----- |
| local   | ✓    | ✓     |
| smb     | ✓    | ✗     |
`,
			wantHits: 0,
		},
		{
			name: "three-column with non-index first column passes",
			content: `# Doc

| Command | Args | Result |
| ------- | ---- | ------ |
| copy    | a b  | done   |
| move    | a b  | done   |
`,
			wantHits: 0,
		},
		{
			name: "four-column matrix passes",
			content: `# Doc

| Op | A | B | C |
| -- | - | - | - |
| x  | 1 | 2 | 3 |
`,
			wantHits: 0,
		},
		{
			name: "converted bullet list passes",
			content: `# Doc

- **grep**: search for text
- **find**: locate files
`,
			wantHits: 0,
		},
		{
			name:     "table-shaped lines inside a fenced code block are ignored",
			content:  "# Doc\n\n```\n| a | b |\n| - | - |\n| 1 | 2 |\n```\n",
			wantHits: 0,
		},
		{
			name: "cell containing an escaped pipe does not inflate the column count",
			content: `# Doc

| Pattern | Meaning      |
| ------- | ------------ |
| a\|b    | a or b       |
`,
			wantHits: 1,
			wantKind: "2-column",
		},
		{
			name:     "no tables means no hits",
			content:  "# Doc\n\nJust some prose, no tables here.\n",
			wantHits: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			hits := scanTwoColTables("test.md", tt.content)
			if len(hits) != tt.wantHits {
				t.Fatalf("got %d hits, want %d: %+v", len(hits), tt.wantHits, hits)
			}
			if tt.wantHits > 0 && !strings.Contains(hits[0].kind, tt.wantKind) {
				t.Errorf("first hit kind = %q, want substring %q", hits[0].kind, tt.wantKind)
			}
		})
	}
}

func TestInAgentFacingScope(t *testing.T) {
	tests := []struct {
		rel  string
		want bool
	}{
		{"AGENTS.md", true},
		{"CLAUDE.md", true},
		{"apps/desktop/src/lib/commands/CLAUDE.md", true},
		{"apps/desktop/src/lib/commands/DETAILS.md", true},
		{"docs/architecture.md", true},
		{"docs/tooling/mcp.md", true},
		{".claude/rules/frontend.md", true},
		// Out of scope:
		{"README.md", false},
		{"CONTRIBUTING.md", false},
		{"CHANGELOG.md", false},
		{"apps/website/CLAUDE.md", false},
		{"apps/website/src/content/blog/post/index.md", false},
		{"brand/CLAUDE.md", false},
		{"apps/desktop/node_modules/foo/CLAUDE.md", false},
		{"apps/desktop/dist/CLAUDE.md", false},
		{"apps/desktop/src/lib/commands/notes.md", false}, // a non-CLAUDE/DETAILS .md outside docs/
		{".github/workflows/README.md", false},
	}
	for _, tt := range tests {
		if got := inAgentFacingScope(tt.rel); got != tt.want {
			t.Errorf("inAgentFacingScope(%q) = %v, want %v", tt.rel, got, tt.want)
		}
	}
}

func TestRunDocsNoTwoColTables(t *testing.T) {
	t.Run("clean tree passes with a stats message", func(t *testing.T) {
		tmp := t.TempDir()
		writeDoc(t, tmp, "CLAUDE.md", "# Root\n\n- **a**: one\n- **b**: two\n")
		writeDoc(t, tmp, "docs/architecture.md", "# Arch\n\nNo tables.\n")

		ctx := &CheckContext{RootDir: tmp}
		result, err := RunDocsNoTwoColTables(ctx)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if result.Code != ResultSuccess {
			t.Fatalf("expected success, got code %d: %s", result.Code, result.Message)
		}
		if !strings.Contains(result.Message, "no 2-column tables") {
			t.Errorf("expected stats message, got: %s", result.Message)
		}
	})

	t.Run("a two-column table fails and cites file and line", func(t *testing.T) {
		tmp := t.TempDir()
		writeDoc(t, tmp, "docs/guide.md", "# Guide\n\n| Key | Value |\n| --- | ----- |\n| a   | b     |\n")

		ctx := &CheckContext{RootDir: tmp}
		_, err := RunDocsNoTwoColTables(ctx)
		if err == nil {
			t.Fatal("expected failure for a two-column table, got success")
		}
		if !strings.Contains(err.Error(), "docs/guide.md:3") {
			t.Errorf("expected error to cite docs/guide.md:3 (the header line), got: %v", err)
		}
	})

	t.Run("a human-facing table outside scope is ignored", func(t *testing.T) {
		tmp := t.TempDir()
		writeDoc(t, tmp, "README.md", "# Readme\n\n| Key | Value |\n| --- | ----- |\n| a   | b     |\n")

		ctx := &CheckContext{RootDir: tmp}
		result, err := RunDocsNoTwoColTables(ctx)
		if err != nil {
			t.Fatalf("expected README to be out of scope (success), got error: %v", err)
		}
		if result.Code != ResultSuccess {
			t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
		}
	})
}
