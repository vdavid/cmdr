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

func TestRunFileLength_AllowlistExceeded(t *testing.T) {
	tmp := t.TempDir()

	// Create a file that exceeds its allowlist
	path := filepath.Join(tmp, "grew.go")
	if err := os.WriteFile(path, []byte(strings.Repeat("line\n", 950)), 0644); err != nil {
		t.Fatal(err)
	}

	// Allowlist it at 900 (but it's 950 now)
	writeAllowlist(t, tmp, map[string]int{"grew.go": 900})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunFileLength(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("expected warning (file exceeded allowlist), got code %d", result.Code)
	}
	if !strings.Contains(result.Message, "grew.go") {
		t.Errorf("expected 'grew.go' in message, got: %s", result.Message)
	}
	if !strings.Contains(result.Message, "allowlist: 900") {
		t.Errorf("expected 'allowlist: 900' in message, got: %s", result.Message)
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
	if result != nil {
		t.Errorf("expected nil for missing allowlist, got %v", result)
	}
}
