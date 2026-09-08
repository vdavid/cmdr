package checks

import (
	"os"
	"path/filepath"
	"testing"
)

// TestReadOpacityFindingCount covers the check-runner side of the
// `CMDR_A11Y_OPACITY_STATUS_FILE` side channel (see `RunA11yContrast`'s doc
// comment): the tool writes a positive count when advisory opacity findings
// remain from an otherwise-clean run, and this reads it back.
func TestReadOpacityFindingCount(t *testing.T) {
	cases := []struct {
		name      string
		fileBody  string // "" means don't create the file
		wantCount int
		wantOk    bool
	}{
		{"missing file (clean run)", "", 0, false},
		{"positive count", "12\n", 12, true},
		{"positive count no trailing newline", "3", 3, true},
		{"zero is treated as none", "0\n", 0, false},
		{"garbage is treated as none", "not-a-number\n", 0, false},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			dir := t.TempDir()
			path := filepath.Join(dir, "count.txt")
			if c.fileBody != "" {
				if err := os.WriteFile(path, []byte(c.fileBody), 0o644); err != nil {
					t.Fatalf("setup: %v", err)
				}
			}

			count, ok := readOpacityFindingCount(path)
			if ok != c.wantOk || count != c.wantCount {
				t.Errorf("readOpacityFindingCount(%q) = (%d, %v), want (%d, %v)", c.fileBody, count, ok, c.wantCount, c.wantOk)
			}
		})
	}
}
