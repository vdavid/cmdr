package checks

import (
	"os"
	"path/filepath"
	"strings"
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

// TestExtractOpacitySection covers the warn-message trimming: the tool's full
// stdout carries the WCAG report, the APCA advisory distribution, and the
// opacity block, but the check-runner warning should surface only the
// opacity block (findings plus its one "fix:" footer), not the rest.
func TestExtractOpacitySection(t *testing.T) {
	output := "APCA (advisory): 1362 pairs, 196 in the Lc 45-60 muted band\n" +
		"✅ APCA Lc-45 floor: every text pair ≥ Lc 45\n" +
		"=== Unmodeled opacity (contrast not verified) ===\n" +
		"  DragOverlay.svelte:79  .drag-overlay.cannot-drop  opacity=0.5\n" +
		"  VolumeBreadcrumb.svelte:1320  .favorite-item.is-dragging  opacity=0.5\n" +
		"  fix: express as a color token (e.g. --color-text-quiet) so the walker sees it, or hand-model it in a synthesizer (dropdown_states.go / query_dialog_states.go)\n" +
		"\n" +
		"⚠️  277 files, 4692 rules checked, 1362 pairs evaluated, 0 violations, 12 unmodeled opacity dims (advisory)\n"

	section := extractOpacitySection(output)
	if !strings.Contains(section, "=== Unmodeled opacity") {
		t.Errorf("expected section to start at the opacity header, got:\n%s", section)
	}
	if !strings.Contains(section, "DragOverlay.svelte:79") || !strings.Contains(section, "fix: express as a color token") {
		t.Errorf("expected section to include findings and the fix footer, got:\n%s", section)
	}
	if strings.Contains(section, "APCA") || strings.Contains(section, "unmodeled opacity dims (advisory)") {
		t.Errorf("expected section to exclude the APCA report and the final summary line, got:\n%s", section)
	}

	if got := lastNonEmptyLine(output); !strings.Contains(got, "277 files") {
		t.Errorf("lastNonEmptyLine() = %q, want the final stats line", got)
	}

	if got := extractOpacitySection("no marker here"); got != "" {
		t.Errorf("extractOpacitySection with no marker = %q, want empty", got)
	}
}
