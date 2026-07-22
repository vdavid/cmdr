package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeDialogGalleryRepo writes the given files into a throwaway root. The check
// reads its two source files by path (no `git ls-files`), so no git repo needed.
func writeDialogGalleryRepo(t *testing.T, files map[string]string) string {
	t.Helper()
	tmp := t.TempDir()
	for rel, content := range files {
		full := filepath.Join(tmp, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(content), 0644); err != nil {
			t.Fatal(err)
		}
	}
	return tmp
}

// A registry file shaped like the real `dialog-registry.ts`.
func dialogRegistrySource(ids ...string) string {
	var sb strings.Builder
	sb.WriteString("export const SOFT_DIALOG_REGISTRY = [\n")
	for _, id := range ids {
		sb.WriteString("  { id: '" + id + "', description: 'Some dialog' },\n")
	}
	sb.WriteString("] as const satisfies readonly { id: string; description?: string }[]\n")
	return sb.String()
}

// A gallery file shaped like the real `gallery-registry.ts`: entries carry
// `dialogId`, and their nested states carry `id` (which must NOT be mistaken for
// a dialog id).
func galleryRegistrySource(dialogIds ...string) string {
	var sb strings.Builder
	sb.WriteString("export const DIALOG_GALLERY_ENTRIES: DialogGalleryEntry[] = [\n")
	for _, id := range dialogIds {
		sb.WriteString("  {\n")
		sb.WriteString("    dialogId: '" + id + "',\n")
		sb.WriteString("    label: 'Some dialog',\n")
		sb.WriteString("    hostWindow: 'main',\n")
		sb.WriteString("    status: 'ready',\n")
		sb.WriteString("    states: [{ id: 'default', label: 'Default' }],\n")
		sb.WriteString("  },\n")
	}
	sb.WriteString("]\n")
	return sb.String()
}

func dialogGalleryFiles(registryIds, galleryIds []string) map[string]string {
	return map[string]string{
		"apps/desktop/src/lib/ui/dialog-registry.ts":              dialogRegistrySource(registryIds...),
		"apps/desktop/src/lib/dialog-gallery/gallery-registry.ts": galleryRegistrySource(galleryIds...),
	}
}

func TestDialogGalleryCoverage_Success(t *testing.T) {
	tmp := writeDialogGalleryRepo(t, dialogGalleryFiles(
		[]string{"alert", "about", "feedback"},
		[]string{"alert", "about", "feedback"},
	))

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunDialogGalleryCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "3 registered dialog(s)") {
		t.Errorf("expected the count in the message, got: %s", result.Message)
	}
}

func TestDialogGalleryCoverage_MissingGalleryEntry(t *testing.T) {
	tmp := writeDialogGalleryRepo(t, dialogGalleryFiles(
		[]string{"alert", "about", "feedback"},
		[]string{"alert", "about"},
	))

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunDialogGalleryCoverage(ctx)
	if err == nil {
		t.Fatal("expected an error for a registered dialog with no gallery entry")
	}
	msg := err.Error()
	if !strings.Contains(msg, "feedback") {
		t.Errorf("expected the failure to name the uncovered dialog, got: %s", msg)
	}
	if !strings.Contains(msg, "gallery-registry.ts") {
		t.Errorf("expected the failure to point at the gallery registry, got: %s", msg)
	}
}

func TestDialogGalleryCoverage_StaleGalleryEntry(t *testing.T) {
	tmp := writeDialogGalleryRepo(t, dialogGalleryFiles(
		[]string{"alert", "about"},
		[]string{"alert", "about", "removed-dialog"},
	))

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunDialogGalleryCoverage(ctx)
	if err == nil {
		t.Fatal("expected an error for a gallery entry with no registry id")
	}
	msg := err.Error()
	if !strings.Contains(msg, "removed-dialog") {
		t.Errorf("expected the failure to name the stale entry, got: %s", msg)
	}
}

// The check asserts id PRESENCE only: an entry with no states (a dialog listed
// but not triggerable) still counts as covered. Otherwise the check would fight
// every honest "listed, not wired" row.
func TestDialogGalleryCoverage_IgnoresStateCompleteness(t *testing.T) {
	gallery := "export const DIALOG_GALLERY_ENTRIES: DialogGalleryEntry[] = [\n" +
		"  { dialogId: 'alert', label: 'Alert', hostWindow: 'main', status: 'not-triggerable', reason: 'Needs a live transfer.', states: [] },\n" +
		"]\n"
	tmp := writeDialogGalleryRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/dialog-registry.ts":              dialogRegistrySource("alert"),
		"apps/desktop/src/lib/dialog-gallery/gallery-registry.ts": gallery,
	})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunDialogGalleryCoverage(ctx)
	if err != nil {
		t.Fatalf("expected success for a stateless entry, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

// Unregistered overlays are listed in the gallery under their own const with an
// `overlayId`, deliberately invisible to this check: they aren't soft dialogs.
func TestDialogGalleryCoverage_IgnoresUnregisteredOverlays(t *testing.T) {
	gallery := galleryRegistrySource("alert") +
		"\nexport const UNREGISTERED_OVERLAY_ENTRIES: UnregisteredOverlayEntry[] = [\n" +
		"  { overlayId: 'command-palette', label: 'Command palette', hostWindow: 'main', reason: 'Not in SOFT_DIALOG_REGISTRY; press ⌘K.' },\n" +
		"]\n"
	tmp := writeDialogGalleryRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/dialog-registry.ts":              dialogRegistrySource("alert"),
		"apps/desktop/src/lib/dialog-gallery/gallery-registry.ts": gallery,
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunDialogGalleryCoverage(ctx)
	if err != nil {
		t.Fatalf("expected unregistered overlays to be ignored, got error: %v", err)
	}
}

func TestDialogGalleryCoverage_MissingGalleryFileFails(t *testing.T) {
	tmp := writeDialogGalleryRepo(t, map[string]string{
		"apps/desktop/src/lib/ui/dialog-registry.ts": dialogRegistrySource("alert"),
	})

	ctx := &CheckContext{RootDir: tmp}
	_, err := RunDialogGalleryCoverage(ctx)
	if err == nil {
		t.Fatal("expected an error when the gallery registry is missing")
	}
	if !strings.Contains(err.Error(), "gallery-registry.ts") {
		t.Errorf("expected the failure to name the missing file, got: %s", err.Error())
	}
}

// Outside a cmdr checkout (or in a test root with no frontend), there's nothing
// to compare, so the check skips rather than inventing a failure.
func TestDialogGalleryCoverage_SkipsWithoutDialogRegistry(t *testing.T) {
	tmp := writeDialogGalleryRepo(t, map[string]string{"some-other-file.txt": "unrelated"})

	ctx := &CheckContext{RootDir: tmp}
	result, err := RunDialogGalleryCoverage(ctx)
	if err != nil {
		t.Fatalf("expected a skip, got error: %v", err)
	}
	if result.Code != ResultSkipped {
		t.Errorf("expected skipped, got code %d: %s", result.Code, result.Message)
	}
}
