package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeClaudeMd writes a CLAUDE.md (or the given relPath) with the given word
// count (one word per token) at dir/relPath, creating parent directories.
func writeClaudeMd(t *testing.T, dir, relPath string, words int) {
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

// writeClaudeMdLengthAllowlist writes a complete allowlist JSON and returns its path.
func writeClaudeMdLengthAllowlist(t *testing.T, dir string, files map[string]int) string {
	t.Helper()
	checksDir := filepath.Join(dir, "scripts", "check", "checks")
	if err := os.MkdirAll(checksDir, 0755); err != nil {
		t.Fatal(err)
	}
	list := claudeMdLengthAllowlist{Comment: "test allowlist", Files: files}
	path := filepath.Join(checksDir, "claude-md-length-allowlist.json")
	if err := writeJSONAllowlist(path, list); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestCountWords(t *testing.T) {
	tests := []struct {
		name     string
		content  string
		expected int
	}{
		{"empty", "", 0},
		{"single word", "hello", 1},
		{"trailing newline", "hello\n", 1},
		{"multiple words", "one two three", 3},
		{"mixed whitespace", "a\nb\tc  d", 4},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tmp := t.TempDir()
			path := filepath.Join(tmp, "f.md")
			if err := os.WriteFile(path, []byte(tt.content), 0644); err != nil {
				t.Fatal(err)
			}
			got, err := countWords(path)
			if err != nil {
				t.Fatal(err)
			}
			if got != tt.expected {
				t.Errorf("countWords() = %d, want %d", got, tt.expected)
			}
		})
	}
}

func TestRunClaudeMdLength_NoLongFiles(t *testing.T) {
	tmp := t.TempDir()
	writeClaudeMd(t, tmp, "CLAUDE.md", 100)
	writeClaudeMd(t, tmp, "sub/CLAUDE.md", 600) // exactly 600 passes (only >600 warns)

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunClaudeMdLength_DetectsLongFile(t *testing.T) {
	tmp := t.TempDir()
	writeClaudeMd(t, tmp, "long/CLAUDE.md", 700)
	writeClaudeMd(t, tmp, "short/CLAUDE.md", 100)

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("expected warning, got code %d", result.Code)
	}
	if !strings.Contains(result.Message, filepath.Join("long", "CLAUDE.md")) {
		t.Errorf("expected long/CLAUDE.md in message, got: %s", result.Message)
	}
	if strings.Contains(result.Message, filepath.Join("short", "CLAUDE.md")) {
		t.Errorf("did not expect short/CLAUDE.md in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "700 words") {
		t.Errorf("expected '700 words' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "over 600 words") {
		t.Errorf("expected 'over 600 words' summary, got: %s", result.Message)
	}
}

func TestRunClaudeMdLength_IgnoresDetailsMd(t *testing.T) {
	tmp := t.TempDir()
	// A huge DETAILS.md must never warn: the pull tier is unlimited.
	writeClaudeMd(t, tmp, "DETAILS.md", 5000)

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success (DETAILS.md not scanned), got code %d: %s", result.Code, result.Message)
	}
}

func TestRunClaudeMdLength_AllowlistSuppresses(t *testing.T) {
	tmp := t.TempDir()
	rel := filepath.Join("big", "CLAUDE.md")
	writeClaudeMd(t, tmp, rel, 900)
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 900})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success (allowlisted), got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "1 allowlisted") {
		t.Errorf("expected '1 allowlisted' in message, got: %s", result.Message)
	}
}

func TestRunClaudeMdLength_AllowlistWithinBuffer(t *testing.T) {
	tmp := t.TempDir()
	// 990 words vs allowlist 900: within the 10% growth buffer.
	rel := filepath.Join("grew", "CLAUDE.md")
	writeClaudeMd(t, tmp, rel, 990)
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 900})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success (within 10%% buffer), got code %d: %s", result.Code, result.Message)
	}
}

func TestRunClaudeMdLength_AllowlistExceeded(t *testing.T) {
	tmp := t.TempDir()
	// 1035 words vs allowlist 900: 15% over, outside the buffer.
	rel := filepath.Join("grew", "CLAUDE.md")
	writeClaudeMd(t, tmp, rel, 1035)
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 900})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("expected warning, got code %d", result.Code)
	}
	if !strings.Contains(result.Message, "allowlist: 900") {
		t.Errorf("expected 'allowlist: 900' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "+15% growth") {
		t.Errorf("expected '+15%% growth' in message, got: %s", result.Message)
	}
}

func TestLoadClaudeMdLengthAllowlist_Missing(t *testing.T) {
	tmp := t.TempDir()
	result := loadClaudeMdLengthAllowlist(tmp)
	if len(result.Files) != 0 {
		t.Errorf("expected empty allowlist for missing file, got %+v", result)
	}
}

func TestRunClaudeMdLength_RemovesDeadEntryLocally(t *testing.T) {
	tmp := t.TempDir()
	rel := filepath.Join("gone", "CLAUDE.md")
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 900})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after dead-entry removal, got: %+v", result)
	}
	reloaded := loadClaudeMdLengthAllowlist(tmp)
	if _, ok := reloaded.Files[rel]; ok {
		t.Error("expected dead entry removed")
	}
	if reloaded.Comment == "" {
		t.Error("expected $comment preserved across rewrite")
	}
}

func TestRunClaudeMdLength_RemovesUnderThresholdEntryLocally(t *testing.T) {
	tmp := t.TempDir()
	rel := filepath.Join("shrunk", "CLAUDE.md")
	writeClaudeMd(t, tmp, rel, 400)
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 900})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after under-threshold removal, got: %+v", result)
	}
	reloaded := loadClaudeMdLengthAllowlist(tmp)
	if _, ok := reloaded.Files[rel]; ok {
		t.Error("expected under-threshold entry removed")
	}
}

func TestRunClaudeMdLength_RatchetsSlackEntryLocally(t *testing.T) {
	tmp := t.TempDir()
	// 700 words, allowed 1000: more than 10% slack → ratchet to 700.
	rel := filepath.Join("slack", "CLAUDE.md")
	writeClaudeMd(t, tmp, rel, 700)
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 1000})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after ratchet, got: %+v", result)
	}
	reloaded := loadClaudeMdLengthAllowlist(tmp)
	if got := reloaded.Files[rel]; got != 700 {
		t.Errorf("expected ratchet to 700, got %d", got)
	}
}

func TestRunClaudeMdLength_LeavesSmallSlackAlone(t *testing.T) {
	tmp := t.TempDir()
	// 870 words, allowed 900: within the 10% slack buffer → no churn.
	rel := filepath.Join("stable", "CLAUDE.md")
	writeClaudeMd(t, tmp, rel, 870)
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 900})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.MadeChanges {
		t.Errorf("expected no rewrite for small slack, got: %s", result.Message)
	}
	reloaded := loadClaudeMdLengthAllowlist(tmp)
	if got := reloaded.Files[rel]; got != 900 {
		t.Errorf("expected stable entry untouched at 900, got %d", got)
	}
}

func TestRunClaudeMdLength_CIReportsStaleWithoutRewriting(t *testing.T) {
	tmp := t.TempDir()
	rel := filepath.Join("gone", "CLAUDE.md")
	writeClaudeMdLengthAllowlist(t, tmp, map[string]int{rel: 900})

	result, err := RunClaudeMdLength(&CheckContext{RootDir: tmp, CI: true})
	if err != nil {
		t.Fatal(err)
	}
	if result.MadeChanges {
		t.Error("expected no rewrite in CI mode")
	}
	if !strings.Contains(result.Message, "CLAUDE.md") {
		t.Errorf("expected stale entry reported in CI, got: %s", result.Message)
	}
	reloaded := loadClaudeMdLengthAllowlist(tmp)
	if _, ok := reloaded.Files[rel]; !ok {
		t.Error("expected allowlist untouched in CI mode")
	}
}
