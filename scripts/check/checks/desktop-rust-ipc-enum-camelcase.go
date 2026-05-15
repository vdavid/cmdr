package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// RunIpcEnumCamelCase verifies that internally-tagged specta::Type enums with `rename_all = "camelCase"`
// and struct variants also have `rename_all_fields = "camelCase"`. Without the latter, field names
// inside struct variants ship as snake_case on the wire even though the variant tags are camelCase.
// See apps/desktop/src/lib/ipc/CLAUDE.md "Type shape constraints" for the rationale.
func RunIpcEnumCamelCase(ctx *CheckContext) (CheckResult, error) {
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")

	violations, scanned, err := scanIpcEnums(rustSrcDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", err)
	}

	if len(violations) > 0 {
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s:%d: enum `%s` has `rename_all = \"camelCase\"` and struct variants but is missing `rename_all_fields = \"camelCase\"`\n",
				v.relPath, v.line, v.enumName))
		}
		return CheckResult{}, fmt.Errorf(
			"found %d IPC %s missing `rename_all_fields = \"camelCase\"`:\n%sfix: add `rename_all_fields = \"camelCase\"` to the `#[serde(...)]` attribute and run `pnpm bindings:regen` (see `apps/desktop/src/lib/ipc/CLAUDE.md` § Type shape constraints)",
			len(violations), Pluralize(len(violations), "enum", "enums"), sb.String(),
		)
	}

	return Success(fmt.Sprintf(
		"%d specta-typed %s scanned, all camelCase-renamed enums with struct variants opt into `rename_all_fields`",
		scanned, Pluralize(scanned, "enum", "enums"),
	)), nil
}

type ipcEnumViolation struct {
	relPath  string
	line     int
	enumName string
}

// Matches enum declarations after we've found a relevant serde attribute.
var enumDeclPattern = regexp.MustCompile(`^\s*(?:pub(?:\s*\([^)]*\))?\s+)?enum\s+(\w+)`)

// Matches struct-variant lines inside an enum body: `VariantName { ... }` (open brace on same line).
var structVariantPattern = regexp.MustCompile(`^\s*\w+\s*\{`)

// Matches a bare variant name on its own line (followed by `{` on the next line).
var bareVariantNamePattern = regexp.MustCompile(`^\s*\w+\s*$`)

// Matches a `derive(...)` list that mentions `specta::Type`.
var deriveSpectaTypePattern = regexp.MustCompile(`derive\s*\([^)]*\bspecta::Type\b`)

// Captures the body of `#[serde(...)]` (single-line attributes only).
var serdeAttrPattern = regexp.MustCompile(`#\[serde\(([^\]]*)\)\]`)

func scanIpcEnums(srcDir string) ([]ipcEnumViolation, int, error) {
	var violations []ipcEnumViolation
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}

		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		lines := strings.Split(string(data), "\n")

		for i := range lines {
			match := enumDeclPattern.FindStringSubmatch(lines[i])
			if match == nil {
				continue
			}
			enumName := match[1]

			// Collect the preceding attribute block (derive + serde).
			attrs := collectPrecedingAttributes(lines, i)
			if !isSpectaType(attrs) {
				continue
			}
			scanned++

			serdeAttr := findSerdeAttribute(attrs)
			if serdeAttr == "" {
				continue
			}
			if !strings.Contains(serdeAttr, `rename_all = "camelCase"`) {
				continue
			}
			if strings.Contains(serdeAttr, `rename_all_fields`) {
				continue
			}
			if !enumHasStructVariant(lines, i) {
				continue
			}

			relPath := relPathFromRoot(srcDir, path)
			violations = append(violations, ipcEnumViolation{
				relPath:  relPath,
				line:     i + 1,
				enumName: enumName,
			})
		}

		return nil
	})

	return violations, scanned, err
}

// collectPrecedingAttributes returns the concatenated text of all `#[...]` attribute blocks
// (single-line) immediately preceding the line at lineIdx. Stops at the first non-attribute,
// non-blank line. Multi-line attributes are not in use anywhere relevant here.
func collectPrecedingAttributes(lines []string, lineIdx int) string {
	var collected []string
	for j := lineIdx - 1; j >= 0; j-- {
		trimmed := strings.TrimSpace(lines[j])
		if trimmed == "" {
			continue
		}
		if !strings.HasPrefix(trimmed, "#[") {
			break
		}
		collected = append([]string{trimmed}, collected...)
	}
	return strings.Join(collected, "\n")
}

// isSpectaType returns true if the attribute block lists `specta::Type` in a `derive(...)`.
func isSpectaType(attrs string) bool {
	return deriveSpectaTypePattern.MatchString(attrs)
}

// findSerdeAttribute extracts the `serde(...)` attribute body from the attribute block, or "".
func findSerdeAttribute(attrs string) string {
	matches := serdeAttrPattern.FindStringSubmatch(attrs)
	if matches == nil {
		return ""
	}
	return matches[1]
}

// enumHasStructVariant scans forward from the enum declaration line, tracking brace depth,
// and returns true if any variant at depth 1 is a struct variant (`Name { ... }`).
func enumHasStructVariant(lines []string, enumLineIdx int) bool {
	// Find the opening brace of the enum body.
	depth := 0
	started := false
	for i := enumLineIdx; i < len(lines); i++ {
		line := lines[i]
		for _, ch := range line {
			if ch == '{' {
				depth++
				started = true
			} else if ch == '}' {
				depth--
				if started && depth == 0 {
					return false
				}
			}
		}
		if !started {
			continue
		}
		// At depth 1 we are between enum variants. Look for struct-variant pattern,
		// but skip lines that are attributes or the enum declaration itself.
		if depth == 1 && i > enumLineIdx {
			trimmed := strings.TrimSpace(line)
			if trimmed == "" || strings.HasPrefix(trimmed, "//") || strings.HasPrefix(trimmed, "#[") {
				continue
			}
			// A struct variant has `Name {` somewhere on its line, OR `Name` followed by `{` on a later line.
			if structVariantPattern.MatchString(line) {
				return true
			}
			// Handle `VariantName` on its own line with `{` on the next.
			if bareVariantNamePattern.MatchString(line) && i+1 < len(lines) {
				next := strings.TrimSpace(lines[i+1])
				if strings.HasPrefix(next, "{") {
					return true
				}
			}
		}
	}
	return false
}

func relPathFromRoot(srcDir, path string) string {
	rootDir := filepath.Dir(filepath.Dir(filepath.Dir(filepath.Dir(srcDir))))
	rel, err := filepath.Rel(rootDir, path)
	if err != nil {
		return path
	}
	return rel
}
