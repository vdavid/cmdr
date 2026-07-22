package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Dialog gallery coverage check: every id in `SOFT_DIALOG_REGISTRY` has a row in
// the dev-only dialog gallery, and every gallery row names a registered id.
//
// The gallery (Debug > Soft dialogs) claims to be a complete inventory of the
// app's registered soft dialogs. Without this check that claim decays the moment
// someone adds dialog #33: the gallery silently stops being an inventory and a
// design review that trusts it reviews the wrong set.
//
// Mechanics:
//   - Registered ids: `{ id: '…'` entries in `lib/ui/dialog-registry.ts`.
//   - Gallery ids: `dialogId: '…'` fields in `lib/dialog-gallery/gallery-registry.ts`.
//     Nested state objects use `id`, not `dialogId`, so they can't be confused
//     for dialog ids; unregistered overlays use `overlayId` and are deliberately
//     invisible here (they aren't soft dialogs).
//   - Asserts id PRESENCE only, never state completeness. A row that's listed
//     with an honest "not triggerable" reason and no states still counts: the
//     alternative would pressure agents into faking states to appease a check.

const (
	dialogRegistryPath  = "apps/desktop/src/lib/ui/dialog-registry.ts"
	galleryRegistryPath = "apps/desktop/src/lib/dialog-gallery/gallery-registry.ts"
)

// softDialogIdPattern matches a registry entry's id: a line whose first token is
// `{ id: '…'`. Anchoring on the opening brace keeps it off the `description`
// values that follow on the same line.
var softDialogIdPattern = regexp.MustCompile(`(?m)^\s*\{\s*id:\s*'([^']+)'`)

// galleryDialogIdPattern matches a gallery entry's `dialogId: '…'` field.
var galleryDialogIdPattern = regexp.MustCompile(`dialogId:\s*'([^']+)'`)

// extractIDs returns the unique first-capture-group matches, in file order.
func extractIDs(pattern *regexp.Regexp, source string) []string {
	seen := make(map[string]bool)
	var ids []string
	for _, m := range pattern.FindAllStringSubmatch(source, -1) {
		if seen[m[1]] {
			continue
		}
		seen[m[1]] = true
		ids = append(ids, m[1])
	}
	return ids
}

// missingFrom returns the ids present in `want` but absent from `have`, sorted.
func missingFrom(want []string, have []string) []string {
	haveSet := make(map[string]bool, len(have))
	for _, id := range have {
		haveSet[id] = true
	}
	var missing []string
	for _, id := range want {
		if !haveSet[id] {
			missing = append(missing, id)
		}
	}
	sort.Strings(missing)
	return missing
}

func formatDialogGalleryCoverageFailure(uncovered, stale []string) string {
	var sb strings.Builder
	sb.WriteString("Dialog gallery is out of sync with SOFT_DIALOG_REGISTRY.\n")

	if len(uncovered) > 0 {
		sb.WriteString(fmt.Sprintf("  %d registered dialog(s) with no gallery entry:\n", len(uncovered)))
		for _, id := range uncovered {
			sb.WriteString(fmt.Sprintf("    - %s\n", id))
		}
		sb.WriteString(fmt.Sprintf("    Add a row with `dialogId: '<id>'` to %s.\n", galleryRegistryPath))
		sb.WriteString("    A dialog you can't preview still gets a row, with `status: 'not-triggerable'` and an honest reason.\n")
	}
	if len(stale) > 0 {
		sb.WriteString(fmt.Sprintf("  %d gallery entry/entries naming an id that isn't registered:\n", len(stale)))
		for _, id := range stale {
			sb.WriteString(fmt.Sprintf("    - %s\n", id))
		}
		sb.WriteString(fmt.Sprintf("    Either add the id to %s, or drop the row from %s.\n", dialogRegistryPath, galleryRegistryPath))
		sb.WriteString("    An overlay that isn't a registered soft dialog belongs in UNREGISTERED_OVERLAY_ENTRIES instead.\n")
	}

	sb.WriteString("\nSee apps/desktop/src/lib/dialog-gallery/DETAILS.md § Adding an entry.")
	return sb.String()
}

// RunDialogGalleryCoverage ensures the Debug > Soft dialogs gallery lists every
// registered soft dialog, and nothing that isn't one.
func RunDialogGalleryCoverage(ctx *CheckContext) (CheckResult, error) {
	registrySource, err := os.ReadFile(filepath.Join(ctx.RootDir, dialogRegistryPath))
	if err != nil {
		if os.IsNotExist(err) {
			return Skipped(fmt.Sprintf("%s not found", dialogRegistryPath)), nil
		}
		return CheckResult{}, fmt.Errorf("read %s: %w", dialogRegistryPath, err)
	}

	gallerySource, err := os.ReadFile(filepath.Join(ctx.RootDir, galleryRegistryPath))
	if err != nil {
		if os.IsNotExist(err) {
			return CheckResult{}, fmt.Errorf("%s is missing, so no soft dialog has a gallery entry", galleryRegistryPath)
		}
		return CheckResult{}, fmt.Errorf("read %s: %w", galleryRegistryPath, err)
	}

	registered := extractIDs(softDialogIdPattern, string(registrySource))
	inGallery := extractIDs(galleryDialogIdPattern, string(gallerySource))

	uncovered := missingFrom(registered, inGallery)
	stale := missingFrom(inGallery, registered)

	if len(uncovered) > 0 || len(stale) > 0 {
		return CheckResult{}, fmt.Errorf("%s", formatDialogGalleryCoverageFailure(uncovered, stale))
	}

	return Success(fmt.Sprintf("%d registered dialog(s) have a gallery entry", len(registered))), nil
}
