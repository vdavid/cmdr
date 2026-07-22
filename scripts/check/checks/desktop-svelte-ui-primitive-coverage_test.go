package checks

import (
	"strings"
	"testing"
)

// A catalog section that imports the given primitive. The check only looks for
// the `$lib/ui/<Name>.svelte` import token, so the surrounding markup is minimal.
func sectionImporting(name string) string {
	return "<script lang=\"ts\">\n  import " + name + " from '$lib/ui/" + name + ".svelte'\n</script>\n<" + name + " />\n"
}

func TestUiPrimitiveCoverage_Success(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":                              "<button>Click</button>",
		"apps/desktop/src/lib/ui/Chip.svelte":                                "<span>chip</span>",
		"apps/desktop/src/routes/dev/components/sections/Buttons.svelte":     sectionImporting("Button"),
		"apps/desktop/src/routes/dev/components/sections/ChipSection.svelte": sectionImporting("Chip"),
		"scripts/check/checks/ui-primitive-coverage-allowlist.json":          `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunUiPrimitiveCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "2 primitive(s) in the catalog") {
		t.Errorf("expected '2 primitive(s) in the catalog', got: %s", result.Message)
	}
}

func TestUiPrimitiveCoverage_MissingSection(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":                          "<button>Click</button>",
		"apps/desktop/src/routes/dev/components/sections/Buttons.svelte": sectionImporting("Button"),
		// Widget has no catalog section and isn't allowlisted.
		"apps/desktop/src/lib/ui/Widget.svelte":                     "<div>widget</div>",
		"scripts/check/checks/ui-primitive-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunUiPrimitiveCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for missing catalog section")
	}
	msg := err.Error()
	if !strings.Contains(msg, "apps/desktop/src/lib/ui/Widget.svelte") {
		t.Errorf("expected failure to name Widget.svelte, got: %s", msg)
	}
	if !strings.Contains(msg, "WidgetSection.svelte") {
		t.Errorf("expected failure to name the expected section path, got: %s", msg)
	}
	if !strings.Contains(msg, "routes/dev/components/+page.svelte") {
		t.Errorf("expected failure to point at the catalog page, got: %s", msg)
	}
}

func TestUiPrimitiveCoverage_AllowlistSuppresses(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Icon.svelte":                       "<svg />",
		"scripts/check/checks/ui-primitive-coverage-allowlist.json": `{"exempt":{"apps/desktop/src/lib/ui/Icon.svelte":"pure glyph atom, demoed in the Graphics catalog"}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunUiPrimitiveCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success with allowlist, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d", result.Code)
	}
	if !strings.Contains(result.Message, "1 allowlisted") {
		t.Errorf("expected '1 allowlisted' in message, got: %s", result.Message)
	}
}

func TestUiPrimitiveCoverage_DeadAllowlistEntry(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":                          "<button>Click</button>",
		"apps/desktop/src/routes/dev/components/sections/Buttons.svelte": sectionImporting("Button"),
		"scripts/check/checks/ui-primitive-coverage-allowlist.json":      `{"exempt":{"apps/desktop/src/lib/ui/Gone.svelte":"stale entry"}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunUiPrimitiveCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for dead allowlist entry")
	}
	msg := err.Error()
	if !strings.Contains(msg, "Gone.svelte") {
		t.Errorf("expected failure to name the dead entry, got: %s", msg)
	}
	if !strings.Contains(msg, "dead allowlist entry") {
		t.Errorf("expected 'dead allowlist entry' in message, got: %s", msg)
	}
}

func TestUiPrimitiveCoverage_RedundantAllowlistEntry(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		// Button is exempt, yet a catalog section imports it → the entry is redundant.
		"apps/desktop/src/lib/ui/Button.svelte":                          "<button>Click</button>",
		"apps/desktop/src/routes/dev/components/sections/Buttons.svelte": sectionImporting("Button"),
		"scripts/check/checks/ui-primitive-coverage-allowlist.json":      `{"exempt":{"apps/desktop/src/lib/ui/Button.svelte":"not a catalog component (no longer true)"}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunUiPrimitiveCoverage(ctx)
	if err == nil {
		t.Fatal("expected error for redundant allowlist entry")
	}
	msg := err.Error()
	if !strings.Contains(msg, "Button.svelte") {
		t.Errorf("expected failure to name the redundant entry, got: %s", msg)
	}
	if !strings.Contains(msg, "redundant") {
		t.Errorf("expected 'redundant' in message, got: %s", msg)
	}
}

func TestUiPrimitiveCoverage_IgnoresSubdirParts(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/Button.svelte":                          "<button>Click</button>",
		"apps/desktop/src/routes/dev/components/sections/Buttons.svelte": sectionImporting("Button"),
		// Sub-parts under subdirs are NOT primitives and must be ignored even
		// with no catalog section.
		"apps/desktop/src/lib/ui/toast/ToastItem.svelte":            "<div>toast</div>",
		"apps/desktop/src/lib/ui/icons/EjectIcon.svelte":            "<svg />",
		"scripts/check/checks/ui-primitive-coverage-allowlist.json": `{"exempt":{}}`,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunUiPrimitiveCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success (subdir parts ignored), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestUiPrimitiveCoverage_MissingAllowlistIsOkWhenNoScope(t *testing.T) {
	tmp := setupGitRepo(t, map[string]string{
		"some-other-file.txt": "unrelated",
	})

	// Don't write an allowlist file; should default to empty.
	ctx := &CheckContext{RootDir: tmp}
	result, err := RunUiPrimitiveCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success (no primitives in scope), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d", result.Code)
	}
}
