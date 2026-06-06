package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestCountLines(t *testing.T) {
	tests := []struct {
		name     string
		content  string
		expected int
	}{
		{"empty file", "", 0},
		{"single line", "hello", 1},
		{"single line with newline", "hello\n", 1},
		{"multiple lines", "a\nb\nc", 3},
		{"multiple lines with trailing newline", "a\nb\nc\n", 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tmp := t.TempDir()
			path := filepath.Join(tmp, "test.txt")
			if err := os.WriteFile(path, []byte(tt.content), 0644); err != nil {
				t.Fatal(err)
			}
			got, err := countLines(path)
			if err != nil {
				t.Fatal(err)
			}
			if got != tt.expected {
				t.Errorf("countLines() = %d, want %d", got, tt.expected)
			}
		})
	}
}

func TestFormatTokenCount(t *testing.T) {
	tests := []struct {
		tokens   int64
		expected string
	}{
		{0, "0"},
		{500, "500"},
		{999, "999"},
		{1000, "1k"},
		{1500, "1k"},
		{108000, "108k"},
		{1500000, "1500k"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			if got := formatTokenCount(tt.tokens); got != tt.expected {
				t.Errorf("formatTokenCount(%d) = %q, want %q", tt.tokens, got, tt.expected)
			}
		})
	}
}

func TestRunFileLength_NoLongFiles(t *testing.T) {
	tmp := t.TempDir()

	path := filepath.Join(tmp, "short.go")
	content := strings.Repeat("line\n", 100)
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d", result.Code)
	}
}

func TestRunFileLength_DetectsLongFiles(t *testing.T) {
	tmp := t.TempDir()

	longPath := filepath.Join(tmp, "long.go")
	content := strings.Repeat("line\n", 850)
	if err := os.WriteFile(longPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	shortPath := filepath.Join(tmp, "short.go")
	shortContent := strings.Repeat("line\n", 100)
	if err := os.WriteFile(shortPath, []byte(shortContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Non-source extension over threshold should be ignored
	txtPath := filepath.Join(tmp, "long.txt")
	if err := os.WriteFile(txtPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("expected warning, got code %d", result.Code)
	}
	if !strings.Contains(result.Message, "long.go") {
		t.Errorf("expected message to contain 'long.go', got: %s", result.Message)
	}
	if strings.Contains(result.Message, "short.go") {
		t.Errorf("expected message to NOT contain 'short.go', got: %s", result.Message)
	}
	if strings.Contains(result.Message, "long.txt") {
		t.Errorf("expected message to NOT contain 'long.txt' (non-source), got: %s", result.Message)
	}
}

func TestRunFileLength_SkipsExcludedDirs(t *testing.T) {
	tmp := t.TempDir()

	nmDir := filepath.Join(tmp, "node_modules")
	if err := os.MkdirAll(nmDir, 0755); err != nil {
		t.Fatal(err)
	}
	longPath := filepath.Join(nmDir, "long.go")
	content := strings.Repeat("line\n", 1000)
	if err := os.WriteFile(longPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success (node_modules should be skipped), got code %d", result.Code)
	}
}

func TestRunFileLength_ColorsYellowAndRed(t *testing.T) {
	tmp := t.TempDir()

	// Over warn threshold (800-1199) → yellow
	warnPath := filepath.Join(tmp, "warn.go")
	if err := os.WriteFile(warnPath, []byte(strings.Repeat("line\n", 900)), 0644); err != nil {
		t.Fatal(err)
	}

	// Over critical threshold (1200+) → red
	critPath := filepath.Join(tmp, "critical.go")
	if err := os.WriteFile(critPath, []byte(strings.Repeat("line\n", 1300)), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}

	if !strings.Contains(result.Message, ansiYellow+"(900 lines") {
		t.Errorf("expected yellow color for 900-line file, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, ansiRed+"(1300 lines") {
		t.Errorf("expected red color for 1300-line file, got: %s", result.Message)
	}
}

func TestRunFileLength_SortedAlphabetically(t *testing.T) {
	tmp := t.TempDir()

	for _, name := range []string{"c.go", "a.go", "b.go"} {
		path := filepath.Join(tmp, name)
		if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 900)), 0644); err != nil {
			t.Fatal(err)
		}
	}

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}

	aIdx := strings.Index(result.Message, "a.go")
	bIdx := strings.Index(result.Message, "b.go")
	cIdx := strings.Index(result.Message, "c.go")

	if aIdx == -1 || bIdx == -1 || cIdx == -1 {
		t.Fatalf("expected all files in message, got: %s", result.Message)
	}
	if !(aIdx < bIdx && bIdx < cIdx) {
		t.Errorf("expected alphabetical order (a < b < c), got a=%d, b=%d, c=%d", aIdx, bIdx, cIdx)
	}
}

func TestRunFileLength_MessageFormat(t *testing.T) {
	tmp := t.TempDir()

	// Create a file with known size to verify kB and token formatting
	path := filepath.Join(tmp, "test.go")
	// 850 lines of "line\n" = 850 * 5 bytes = 4250 bytes
	content := strings.Repeat("line\n", 850)
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}

	// 4250 bytes → 4 kB (4250/1000), ~1k tokens (4250/4=1062, 1062/1000=1k)
	if !strings.Contains(result.Message, "850 lines") {
		t.Errorf("expected '850 lines' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "4 kB") {
		t.Errorf("expected '4 kB' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "~1k tokens") {
		t.Errorf("expected '~1k tokens' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "1 new file over 800 lines") {
		t.Errorf("expected '1 new file over 800 lines' in summary, got: %s", result.Message)
	}
}

func writeAllowlist(t *testing.T, dir string, files map[string]int) {
	t.Helper()
	checksDir := filepath.Join(dir, "scripts", "check", "checks")
	if err := os.MkdirAll(checksDir, 0755); err != nil {
		t.Fatal(err)
	}
	// Build JSON manually to keep it simple
	var sb strings.Builder
	sb.WriteString(`{"files":{`)
	first := true
	for path, lines := range files {
		if !first {
			sb.WriteString(",")
		}
		sb.WriteString(fmt.Sprintf(`"%s":%d`, path, lines))
		first = false
	}
	sb.WriteString("}}")
	if err := os.WriteFile(filepath.Join(checksDir, "file-length-allowlist.json"), []byte(sb.String()), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestRunFileLength_AllowlistSuppresses(t *testing.T) {
	tmp := t.TempDir()

	// Create a long file
	path := filepath.Join(tmp, "big.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 900)), 0644); err != nil {
		t.Fatal(err)
	}

	// Allowlist it at 900 lines
	writeAllowlist(t, tmp, map[string]int{"big.go": 900})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success (file is allowlisted), got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "1 allowlisted") {
		t.Errorf("expected '1 allowlisted' in message, got: %s", result.Message)
	}
}

func TestRunFileLength_AllowlistWithinBuffer(t *testing.T) {
	tmp := t.TempDir()

	// File at 990 lines vs allowlist 900; within the 10% growth buffer
	path := filepath.Join(tmp, "grew.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 990)), 0644); err != nil {
		t.Fatal(err)
	}
	writeAllowlist(t, tmp, map[string]int{"grew.go": 900})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success (within 10%% buffer), got code %d: %s", result.Code, result.Message)
	}
}

func TestRunFileLength_AllowlistExceeded(t *testing.T) {
	tmp := t.TempDir()

	// File at 1035 lines vs allowlist 900; 15% over, outside the 10% buffer
	path := filepath.Join(tmp, "grew.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 1035)), 0644); err != nil {
		t.Fatal(err)
	}
	writeAllowlist(t, tmp, map[string]int{"grew.go": 900})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("expected warning (file exceeded allowlist + buffer), got code %d", result.Code)
	}
	if !strings.Contains(result.Message, "grew.go") {
		t.Errorf("expected 'grew.go' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "allowlist: 900") {
		t.Errorf("expected 'allowlist: 900' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "+15% growth") {
		t.Errorf("expected '+15%% growth' in message, got: %s", result.Message)
	}
}

func TestRunFileLength_NewFileNotInAllowlist(t *testing.T) {
	tmp := t.TempDir()

	// Create a long file NOT in the allowlist
	path := filepath.Join(tmp, "new.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 850)), 0644); err != nil {
		t.Fatal(err)
	}

	// Empty allowlist
	writeAllowlist(t, tmp, map[string]int{})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("expected warning (new file not allowlisted), got code %d", result.Code)
	}
	if !strings.Contains(result.Message, "new.go") {
		t.Errorf("expected 'new.go' in message, got: %s", result.Message)
	}
}

func TestLoadFileLengthAllowlist_Missing(t *testing.T) {
	tmp := t.TempDir()
	result := loadFileLengthAllowlist(tmp)
	if len(result.Files) != 0 || len(result.Exempt) != 0 {
		t.Errorf("expected empty allowlist for missing file, got %+v", result)
	}
}

// writeAllowlistFull writes a complete allowlist JSON (comment, exempt, files)
// and returns its path.
func writeAllowlistFull(t *testing.T, dir string, exempt map[string]string, files map[string]int) string {
	t.Helper()
	checksDir := filepath.Join(dir, "scripts", "check", "checks")
	if err := os.MkdirAll(checksDir, 0755); err != nil {
		t.Fatal(err)
	}
	list := fileLengthAllowlist{Comment: "test allowlist", Exempt: exempt, Files: files}
	path := filepath.Join(checksDir, "file-length-allowlist.json")
	if err := writeJSONAllowlist(path, list); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestRunFileLength_ExemptFileNeverWarns(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "generated.ts")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 5000)), 0644); err != nil {
		t.Fatal(err)
	}
	writeAllowlistFull(t, tmp, map[string]string{"generated.ts": "generated file, length not actionable"}, nil)

	result, err := RunFileLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success for exempt file, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunFileLength_RemovesDeadEntryLocally(t *testing.T) {
	tmp := t.TempDir()
	allowlistPath := writeAllowlistFull(t, tmp, nil, map[string]int{"gone.go": 900})

	result, err := RunFileLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after dead-entry removal, got: %+v", result)
	}
	if !strings.Contains(result.Message, "gone.go") {
		t.Errorf("expected removal mention of gone.go, got: %s", result.Message)
	}
	reloaded := loadFileLengthAllowlist(tmp)
	if _, ok := reloaded.Files["gone.go"]; ok {
		t.Errorf("expected dead entry removed from %s, still present", allowlistPath)
	}
	if reloaded.Comment == "" {
		t.Error("expected $comment preserved across rewrite")
	}
}

func TestRunFileLength_RemovesUnderThresholdEntryLocally(t *testing.T) {
	tmp := t.TempDir()
	path := filepath.Join(tmp, "shrunk.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 500)), 0644); err != nil {
		t.Fatal(err)
	}
	writeAllowlistFull(t, tmp, nil, map[string]int{"shrunk.go": 900})

	result, err := RunFileLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after under-threshold removal, got: %+v", result)
	}
	reloaded := loadFileLengthAllowlist(tmp)
	if _, ok := reloaded.Files["shrunk.go"]; ok {
		t.Error("expected under-threshold entry removed")
	}
}

func TestRunFileLength_RatchetsSlackEntryLocally(t *testing.T) {
	tmp := t.TempDir()
	// File at 900 lines, allowed 1200: more than 10% slack → ratchet to 900.
	path := filepath.Join(tmp, "slack.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 900)), 0644); err != nil {
		t.Fatal(err)
	}
	writeAllowlistFull(t, tmp, nil, map[string]int{"slack.go": 1200})

	result, err := RunFileLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after ratchet, got: %+v", result)
	}
	reloaded := loadFileLengthAllowlist(tmp)
	if got := reloaded.Files["slack.go"]; got != 900 {
		t.Errorf("expected slack.go ratcheted to 900, got %d", got)
	}
}

func TestRunFileLength_LeavesSmallSlackAlone(t *testing.T) {
	tmp := t.TempDir()
	// File at 870 lines, allowed 900: within the 10% slack buffer → no churn.
	path := filepath.Join(tmp, "stable.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 870)), 0644); err != nil {
		t.Fatal(err)
	}
	writeAllowlistFull(t, tmp, nil, map[string]int{"stable.go": 900})

	result, err := RunFileLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if result.MadeChanges {
		t.Errorf("expected no rewrite for small slack, got changes: %s", result.Message)
	}
	reloaded := loadFileLengthAllowlist(tmp)
	if got := reloaded.Files["stable.go"]; got != 900 {
		t.Errorf("expected stable.go untouched at 900, got %d", got)
	}
}

func TestRunFileLength_RemovesDeadExemptEntryLocally(t *testing.T) {
	tmp := t.TempDir()
	writeAllowlistFull(t, tmp, map[string]string{"gone.ts": "stale reason"}, nil)

	result, err := RunFileLength(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after dead exempt removal, got: %+v", result)
	}
	reloaded := loadFileLengthAllowlist(tmp)
	if _, ok := reloaded.Exempt["gone.ts"]; ok {
		t.Error("expected dead exempt entry removed")
	}
}

func TestRunFileLength_CIReportsStaleWithoutRewriting(t *testing.T) {
	tmp := t.TempDir()
	writeAllowlistFull(t, tmp, nil, map[string]int{"gone.go": 900})

	result, err := RunFileLength(&CheckContext{RootDir: tmp, CI: true})
	if err != nil {
		t.Fatal(err)
	}
	if result.MadeChanges {
		t.Error("expected no rewrite in CI mode")
	}
	if !strings.Contains(result.Message, "gone.go") {
		t.Errorf("expected stale entry reported in CI, got: %s", result.Message)
	}
	reloaded := loadFileLengthAllowlist(tmp)
	if _, ok := reloaded.Files["gone.go"]; !ok {
		t.Error("expected allowlist untouched in CI mode")
	}
}
