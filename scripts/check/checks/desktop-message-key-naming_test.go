package checks

import (
	"os"
	"path/filepath"
	"testing"
)

func TestValidateMessageKey_AcceptsWellFormedKeys(t *testing.T) {
	good := []string{
		"common.cancel",
		"transfer.trash",
		"settings.fsWatch.title",
		"settings.fsWatch.clearIndex",
		"common.openSystemSettings",
		"transfer.fileOnly.allDone",
	}
	for _, key := range good {
		if reason := validateMessageKey(key); reason != "" {
			t.Errorf("validateMessageKey(%q) = %q, want valid", key, reason)
		}
	}
}

func TestValidateMessageKey_RejectsBadShapes(t *testing.T) {
	bad := []string{
		"transfer",            // single segment, not scoped
		"Transfer.trash",      // segment must start lowercase
		"transfer.Trash",      // leaf must start lowercase
		"transfer.trash-item", // hyphen not allowed
		"transfer..trash",     // empty segment
		"transfer.trash.",     // trailing dot
		"1transfer.trash",     // segment can't start with a digit
		"transfer.0count",     // leaf can't start with a digit
	}
	for _, key := range bad {
		if reason := validateMessageKey(key); reason == "" {
			t.Errorf("validateMessageKey(%q) = valid, want a shape violation", key)
		}
	}
}

func TestValidateMessageKey_RejectsUnknownArea(t *testing.T) {
	if reason := validateMessageKey("bogusarea.title"); reason == "" {
		t.Error("validateMessageKey(bogusarea.title) = valid, want an unknown-area violation")
	}
	// Shape is fine, only the area is unknown — the reason should say so.
	if reason := validateMessageKey("widgets.foo"); reason == "" {
		t.Error("validateMessageKey(widgets.foo) = valid, want an unknown-area violation")
	}
}

func TestScanMessageKeyNaming_FlagsBadKeyAndValidatesMetadataTwin(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "transfer.json")
	content := `{
  "transfer.trash": "ok",
  "@transfer.trash": { "description": "fine" },
  "transfer.Bad-Key": "malformed",
  "@bogusarea.thing": { "description": "metadata for a bad key" }
}`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	violations, count, err := scanMessageKeyNaming(path, "transfer.json")
	if err != nil {
		t.Fatalf("scanMessageKeyNaming: %v", err)
	}
	// Two message keys counted (the `@` twins aren't separate messages).
	if count != 2 {
		t.Errorf("count = %d, want 2", count)
	}
	// `transfer.Bad-Key` (malformed) and `@bogusarea.thing` (metadata for an
	// unknown area) both flag; `transfer.trash` + its `@` twin pass.
	if len(violations) != 2 {
		t.Fatalf("got %d violations, want 2: %#v", len(violations), violations)
	}
}

func TestScanMessageKeyNaming_CleanFilePasses(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "common.json")
	content := `{
  "common.cancel": "Cancel",
  "@common.cancel": { "description": "fine" }
}`
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
	violations, count, err := scanMessageKeyNaming(path, "common.json")
	if err != nil {
		t.Fatalf("scanMessageKeyNaming: %v", err)
	}
	if len(violations) != 0 {
		t.Errorf("got %d violations, want 0: %#v", len(violations), violations)
	}
	if count != 1 {
		t.Errorf("count = %d, want 1", count)
	}
}
