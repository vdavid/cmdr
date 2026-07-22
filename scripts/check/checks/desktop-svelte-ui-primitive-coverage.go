package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// UI primitive coverage check: every top-level primitive under
// apps/desktop/src/lib/ui/*.svelte must either be rendered by a section in the
// Debug > Components catalog (some file under routes/dev/components/sections/
// imports it) OR be listed in the allowlist with a reason.
//
// This mirrors `a11y-coverage` (every primitive needs an a11y test) for the
// catalog: it guards against a new primitive silently never showing up in the
// dev catalog, so every component type stays discoverable in Debug > Components.
// See `apps/desktop/src/lib/ui/CLAUDE.md` § "When adding a primitive".
//
// Mechanics:
//   - Primitives: git-tracked `apps/desktop/src/lib/ui/*.svelte` (top level
//     only; subdir files like `toast/`, `icons/` are sub-parts, out of scope).
//   - Covered: some tracked file under `routes/dev/components/sections/` imports
//     the primitive via `$lib/ui/<Name>.svelte`.
//   - Allowlist: primitives that don't belong in the Components catalog (e.g.
//     pure-visual atoms demoed in the sibling Graphics catalog). Each entry
//     carries a reason.
//   - Flags dead allowlist entries (primitive no longer exists) and redundant
//     ones (an allowlisted primitive that a section imports anyway), forcing
//     cleanup when primitives move or gain a catalog section.

const uiPrimitiveScope = "apps/desktop/src/lib/ui"
const uiPrimitiveCatalogSectionsDir = "apps/desktop/src/routes/dev/components/sections"

// uiPrimitiveImportPattern matches an import of a top-level ui primitive, e.g.
// `import Button from '$lib/ui/Button.svelte'`. The name has no slash, so
// subpath imports (`$lib/ui/toast/...`) don't match — those aren't primitives.
var uiPrimitiveImportPattern = regexp.MustCompile(`\$lib/ui/([A-Za-z0-9_]+)\.svelte`)

type uiPrimitiveCoverageAllowlist struct {
	// Exempt maps a primitive's relative path (from repo root) to a
	// human-readable reason. Example:
	// "apps/desktop/src/lib/ui/Icon.svelte": "pure glyph atom, demoed in the Graphics catalog"
	Exempt map[string]string `json:"exempt"`
}

func loadUiPrimitiveCoverageAllowlist(rootDir string) (uiPrimitiveCoverageAllowlist, error) {
	path := filepath.Join(rootDir, "scripts", "check", "checks", "ui-primitive-coverage-allowlist.json")
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return uiPrimitiveCoverageAllowlist{Exempt: map[string]string{}}, nil
		}
		return uiPrimitiveCoverageAllowlist{}, err
	}
	var list uiPrimitiveCoverageAllowlist
	if err := json.Unmarshal(data, &list); err != nil {
		return uiPrimitiveCoverageAllowlist{}, fmt.Errorf("parse allowlist: %w", err)
	}
	if list.Exempt == nil {
		list.Exempt = map[string]string{}
	}
	return list, nil
}

// isTopLevelUiPrimitive reports whether a tracked path is a top-level
// `lib/ui/*.svelte` file (no further subdir, so `toast/ToastItem.svelte` and
// `icons/EjectIcon.svelte` are excluded as sub-parts).
func isTopLevelUiPrimitive(rel string) bool {
	if !strings.HasSuffix(rel, ".svelte") {
		return false
	}
	rest, ok := strings.CutPrefix(rel, uiPrimitiveScope+"/")
	if !ok {
		return false
	}
	return !strings.Contains(rest, "/")
}

// primitiveName maps a primitive path to its component name (the import token):
// "apps/desktop/src/lib/ui/Button.svelte" -> "Button".
func primitiveName(rel string) string {
	return strings.TrimSuffix(filepath.Base(rel), ".svelte")
}

// collectCatalogImports returns the set of primitive names imported by any
// tracked catalog section file.
func collectCatalogImports(rootDir string) (map[string]bool, error) {
	sections, err := listTrackedFiles(rootDir, uiPrimitiveCatalogSectionsDir)
	if err != nil {
		return nil, err
	}
	imported := make(map[string]bool)
	for _, rel := range sections {
		if !strings.HasSuffix(rel, ".svelte") {
			continue
		}
		data, err := os.ReadFile(filepath.Join(rootDir, rel))
		if err != nil {
			return nil, fmt.Errorf("read %s: %w", rel, err)
		}
		for _, m := range uiPrimitiveImportPattern.FindAllStringSubmatch(string(data), -1) {
			imported[m[1]] = true
		}
	}
	return imported, nil
}

type uiPrimitiveCoverageResult struct {
	uncovered          []string          // primitives with no catalog section and not allowlisted
	deadAllowlist      []string          // allowlist entries whose primitive no longer exists
	redundantAllowlist []string          // allowlisted primitives that a section imports anyway
	allowlistedCount   int               // count of valid allowlist entries
	coveredCount       int               // count of primitives with a catalog section
	allowlistReasons   map[string]string // for formatting
}

func scanUiPrimitiveCoverage(rootDir string, allowlist uiPrimitiveCoverageAllowlist) (uiPrimitiveCoverageResult, error) {
	var result uiPrimitiveCoverageResult
	result.allowlistReasons = allowlist.Exempt

	tracked, err := listTrackedFiles(rootDir, uiPrimitiveScope)
	if err != nil {
		return result, err
	}

	imported, err := collectCatalogImports(rootDir)
	if err != nil {
		return result, err
	}

	primitiveSet := make(map[string]bool)
	for _, rel := range tracked {
		if !isTopLevelUiPrimitive(rel) {
			continue
		}
		primitiveSet[rel] = true

		covered := imported[primitiveName(rel)]

		if _, exempt := allowlist.Exempt[rel]; exempt {
			// An exempt primitive that has a catalog section anyway makes the
			// entry redundant: the "not a catalog component" reason no longer holds.
			if covered {
				result.redundantAllowlist = append(result.redundantAllowlist, rel)
				continue
			}
			result.allowlistedCount++
			continue
		}

		if covered {
			result.coveredCount++
			continue
		}
		result.uncovered = append(result.uncovered, rel)
	}

	// Dead allowlist entries: paths in the allowlist that no longer exist as
	// tracked top-level primitives.
	for path := range allowlist.Exempt {
		if !primitiveSet[path] {
			result.deadAllowlist = append(result.deadAllowlist, path)
		}
	}

	sort.Strings(result.uncovered)
	sort.Strings(result.deadAllowlist)
	sort.Strings(result.redundantAllowlist)

	return result, nil
}

func formatUiPrimitiveCoverageFailure(r uiPrimitiveCoverageResult) string {
	var sb strings.Builder
	sb.WriteString("UI primitive catalog gaps found. Add a Components-catalog section OR allowlist with a reason.\n")

	if len(r.uncovered) > 0 {
		sb.WriteString(fmt.Sprintf("  %d primitive(s) with no Debug > Components catalog section:\n", len(r.uncovered)))
		for _, f := range r.uncovered {
			name := primitiveName(f)
			sb.WriteString(fmt.Sprintf("    - %s\n", f))
			sb.WriteString(fmt.Sprintf("        Add apps/desktop/src/routes/dev/components/sections/%sSection.svelte that imports %q,\n", name, "$lib/ui/"+name+".svelte"))
			sb.WriteString("        then wire it into routes/dev/components/+page.svelte and the Debug sidebar (routes/debug/+page.svelte).\n")
		}
	}
	if len(r.deadAllowlist) > 0 {
		sb.WriteString(fmt.Sprintf("  %d dead allowlist entry/entries (primitive no longer exists):\n", len(r.deadAllowlist)))
		for _, f := range r.deadAllowlist {
			sb.WriteString(fmt.Sprintf("    - %s: remove from scripts/check/checks/ui-primitive-coverage-allowlist.json\n", f))
		}
	}
	if len(r.redundantAllowlist) > 0 {
		sb.WriteString(fmt.Sprintf("  %d redundant allowlist entry/entries (primitive has a catalog section anyway):\n", len(r.redundantAllowlist)))
		for _, f := range r.redundantAllowlist {
			sb.WriteString(fmt.Sprintf("    - %s: remove from scripts/check/checks/ui-primitive-coverage-allowlist.json\n", f))
		}
	}
	sb.WriteString("\nSee apps/desktop/src/lib/ui/CLAUDE.md § \"When adding a primitive\".\n")
	sb.WriteString("Allowlist is for primitives that don't belong in the Components catalog (e.g. pure-visual atoms demoed in the Graphics catalog). Include a reason.")
	return strings.TrimRight(sb.String(), "\n")
}

// RunUiPrimitiveCoverage ensures every tracked top-level primitive under
// lib/ui/ has a Debug > Components catalog section or is explicitly allowlisted.
func RunUiPrimitiveCoverage(ctx *CheckContext) (CheckResult, error) {
	allowlist, err := loadUiPrimitiveCoverageAllowlist(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("load allowlist: %w", err)
	}
	result, err := scanUiPrimitiveCoverage(ctx.RootDir, allowlist)
	if err != nil {
		return CheckResult{}, fmt.Errorf("scan: %w", err)
	}

	if len(result.uncovered) == 0 && len(result.deadAllowlist) == 0 && len(result.redundantAllowlist) == 0 {
		suffix := ""
		if result.allowlistedCount > 0 {
			suffix = fmt.Sprintf(" (%d allowlisted)", result.allowlistedCount)
		}
		return Success(fmt.Sprintf("%d primitive(s) in the catalog%s", result.coveredCount, suffix)), nil
	}

	return CheckResult{}, fmt.Errorf("%s", formatUiPrimitiveCoverageFailure(result))
}
