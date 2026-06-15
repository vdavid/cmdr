package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeFile writes content at dir/relPath, creating parent directories.
func writeDetailsSiblingFile(t *testing.T, dir, relPath, content string) {
	t.Helper()
	full := filepath.Join(dir, relPath)
	if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(full, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestRunClaudeMdDetailsSibling_RootExempt(t *testing.T) {
	tmp := t.TempDir()
	// Root CLAUDE.md with no sibling DETAILS.md is fine: it's the @-import manifest.
	writeDetailsSiblingFile(t, tmp, "CLAUDE.md", "@AGENTS.md")

	result, err := RunClaudeMdDetailsSibling(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("root CLAUDE.md should be exempt: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunClaudeMdDetailsSibling_PairedAndLinked(t *testing.T) {
	tmp := t.TempDir()
	writeDetailsSiblingFile(t, tmp, "lib/foo/CLAUDE.md", "Module foo. See [DETAILS.md](DETAILS.md) for depth.")
	writeDetailsSiblingFile(t, tmp, "lib/foo/DETAILS.md", "Depth here.")

	result, err := RunClaudeMdDetailsSibling(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunClaudeMdDetailsSibling_MissingSibling(t *testing.T) {
	tmp := t.TempDir()
	writeDetailsSiblingFile(t, tmp, "lib/bar/CLAUDE.md", "Module bar. See [DETAILS.md](DETAILS.md).")
	// No DETAILS.md sibling.

	_, err := RunClaudeMdDetailsSibling(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for a CLAUDE.md with no sibling DETAILS.md")
	}
	if !strings.Contains(err.Error(), filepath.Join("lib", "bar", "CLAUDE.md")) {
		t.Errorf("expected the offending path in the error, got: %v", err)
	}
	if !strings.Contains(err.Error(), "no sibling DETAILS.md") {
		t.Errorf("expected 'no sibling DETAILS.md' reason, got: %v", err)
	}
}

func TestRunClaudeMdDetailsSibling_PresentButNotReferenced(t *testing.T) {
	tmp := t.TempDir()
	writeDetailsSiblingFile(t, tmp, "lib/baz/CLAUDE.md", "Module baz. No pointer to the depth doc here.")
	writeDetailsSiblingFile(t, tmp, "lib/baz/DETAILS.md", "Depth here.")

	_, err := RunClaudeMdDetailsSibling(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for a CLAUDE.md that doesn't reference its DETAILS.md")
	}
	if !strings.Contains(err.Error(), "doesn't reference") {
		t.Errorf("expected 'doesn't reference' reason, got: %v", err)
	}
}

func TestRunClaudeMdDetailsSibling_ReferenceVariants(t *testing.T) {
	// Both Markdown links and backtick paths count, syntax-agnostic (like
	// docs-reachable), and a reference to any DETAILS.md path counts (not strictly
	// the sibling): the sibling-exists half is the structural guarantee.
	for _, ref := range []string{
		"[DETAILS.md](DETAILS.md)",
		"[the details](./DETAILS.md)",
		"[anchor](DETAILS.md#section-name)",
		"see `DETAILS.md` for depth",
		"see `./DETAILS.md` for depth",
		"see `../sibling-area/DETAILS.md` for the related flow",
		"[api](apps/api-server/DETAILS.md)",
	} {
		tmp := t.TempDir()
		writeDetailsSiblingFile(t, tmp, "a/CLAUDE.md", "Doc. "+ref)
		writeDetailsSiblingFile(t, tmp, "a/DETAILS.md", "Depth.")
		result, err := RunClaudeMdDetailsSibling(&CheckContext{RootDir: tmp})
		if err != nil {
			t.Fatalf("ref %q should count: %v", ref, err)
		}
		if result.Code != ResultSuccess {
			t.Errorf("ref %q: expected success, got %s", ref, result.Message)
		}
	}
}

func TestRunClaudeMdDetailsSibling_NoDetailsMentionFails(t *testing.T) {
	// A CLAUDE.md with a sibling DETAILS.md but no DETAILS.md mention at all fails.
	tmp := t.TempDir()
	writeDetailsSiblingFile(t, tmp, "a/CLAUDE.md", "Doc with no pointer to any depth doc.")
	writeDetailsSiblingFile(t, tmp, "a/DETAILS.md", "Depth.")

	_, err := RunClaudeMdDetailsSibling(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error: no DETAILS.md reference at all")
	}
	if !strings.Contains(err.Error(), "doesn't reference") {
		t.Errorf("expected 'doesn't reference' reason, got: %v", err)
	}
}
