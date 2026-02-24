package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/BurntSushi/toml"
)

// RunCfgGate verifies that Rust code properly gates macOS-only crate imports with #[cfg(target_os = "macos")].
func RunCfgGate(ctx *CheckContext) (CheckResult, error) {
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")
	cargoPath := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "Cargo.toml")

	// Step 1: Parse Cargo.toml and extract macOS-only crate names
	macOSModules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to parse Cargo.toml: %w", err)
	}
	if len(macOSModules) == 0 {
		return Success("No macOS-only dependencies found"), nil
	}

	// Step 2: Build set of module-gated files (files inside cfg(target_os = "macos") modules)
	gatedFiles, err := buildModuleGatedFileSet(rustSrcDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to build module-gated file set: %w", err)
	}

	// Step 3 & 4: Scan remaining .rs files for ungated uses of macOS-only crates
	violations, gatedUseCount, err := scanForUngatedUses(rustSrcDir, macOSModules, gatedFiles)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", err)
	}

	// Step 5: Report violations
	if len(violations) > 0 {
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s:%d: use of macOS-only crate '%s' without #[cfg(target_os = \"macos\")]\n",
				v.relPath, v.line, v.crateName))
		}
		return CheckResult{}, fmt.Errorf(
			"found %d ungated %s of macOS-only crates:\n%s",
			len(violations), Pluralize(len(violations), "use", "uses"), sb.String(),
		)
	}

	// Step 6: Success
	return Success(fmt.Sprintf(
		"%d gated %s of %d macOS-only %s verified (%d %s skipped via module-level gating)",
		gatedUseCount, Pluralize(gatedUseCount, "use", "uses"),
		len(macOSModules), Pluralize(len(macOSModules), "crate", "crates"),
		len(gatedFiles), Pluralize(len(gatedFiles), "file", "files"),
	)), nil
}

// violation records a single ungated use of a macOS-only crate.
type violation struct {
	relPath   string
	line      int
	crateName string
}

// extractMacOSCrateModules parses Cargo.toml and returns the set of Rust module names
// (hyphens converted to underscores) for crates declared under [target.'cfg(target_os = "macos")'.dependencies].
func extractMacOSCrateModules(cargoPath string) (map[string]bool, error) {
	var cargo map[string]any
	if _, err := toml.DecodeFile(cargoPath, &cargo); err != nil {
		return nil, err
	}

	// Navigate: target -> cfg(target_os = "macos") -> dependencies
	targetSection, ok := cargo["target"]
	if !ok {
		return nil, nil
	}
	targetMap, ok := targetSection.(map[string]any)
	if !ok {
		return nil, nil
	}

	cfgSection, ok := targetMap[`cfg(target_os = "macos")`]
	if !ok {
		return nil, nil
	}
	cfgMap, ok := cfgSection.(map[string]any)
	if !ok {
		return nil, nil
	}

	depsSection, ok := cfgMap["dependencies"]
	if !ok {
		return nil, nil
	}
	depsMap, ok := depsSection.(map[string]any)
	if !ok {
		return nil, nil
	}

	modules := make(map[string]bool, len(depsMap))
	for crateName := range depsMap {
		moduleName := strings.ReplaceAll(crateName, "-", "_")
		modules[moduleName] = true
	}
	return modules, nil
}

// modDeclPattern matches cfg-gated module declarations: optional visibility, then mod <name>;
var modDeclPattern = regexp.MustCompile(`^(?:pub(?:\s*\((?:crate|super)\))?\s+)?mod\s+(\w+)\s*;`)

// buildModuleGatedFileSet scans lib.rs and mod.rs files to find modules gated behind
// #[cfg(target_os = "macos")], then resolves them to actual file paths.
// Returns a set of absolute file paths that are inherently gated.
func buildModuleGatedFileSet(srcDir string) (map[string]bool, error) {
	gatedFiles := make(map[string]bool)

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		name := d.Name()
		if name != "lib.rs" && name != "mod.rs" {
			return nil
		}

		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}

		dir := filepath.Dir(path)
		lines := strings.Split(string(data), "\n")
		gatedModNames := findCfgGatedModules(lines)

		for _, modName := range gatedModNames {
			// Resolve to <dir>/<name>.rs or <dir>/<name>/mod.rs
			singleFile := filepath.Join(dir, modName+".rs")
			dirModule := filepath.Join(dir, modName, "mod.rs")

			if info, err := os.Stat(filepath.Join(dir, modName)); err == nil && info.IsDir() {
				// Directory module: add mod.rs and recursively add all .rs files
				gatedFiles[dirModule] = true
				subDir := filepath.Join(dir, modName)
				_ = filepath.WalkDir(subDir, func(subPath string, subD os.DirEntry, subErr error) error {
					if subErr != nil {
						return subErr
					}
					if !subD.IsDir() && strings.HasSuffix(subD.Name(), ".rs") {
						gatedFiles[subPath] = true
					}
					return nil
				})
			} else if _, err := os.Stat(singleFile); err == nil {
				// Single file module
				gatedFiles[singleFile] = true
			}
		}

		return nil
	})

	return gatedFiles, err
}

// findCfgGatedModules finds module names that are preceded by #[cfg(target_os = "macos")]
// in the given lines. Handles blank lines and other attributes between the cfg and the mod.
func findCfgGatedModules(lines []string) []string {
	var result []string

	for i, line := range lines {
		trimmed := strings.TrimSpace(line)

		// Check if this line is a mod declaration
		matches := modDeclPattern.FindStringSubmatch(trimmed)
		if matches == nil {
			continue
		}
		modName := matches[1]

		// Walk backwards to see if there's a #[cfg(target_os = "macos")] attribute
		if hasMacOSCfgAttribute(lines, i) {
			result = append(result, modName)
		}
	}

	return result
}

// usePattern matches `use <ident>::` with optional visibility and leading whitespace.
var usePattern = regexp.MustCompile(`^\s*(?:pub(?:\s*\((?:crate|super)\))?\s+)?use\s+(\w+)::`)

// scanForUngatedUses walks all .rs files, skipping gated files, and checks that
// uses of macOS-only crates are properly gated. Returns violations and the count of
// properly gated uses found.
func scanForUngatedUses(srcDir string, macOSModules map[string]bool, gatedFiles map[string]bool) ([]violation, int, error) {
	var violations []violation
	gatedUseCount := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}

		// Skip files that are inside cfg-gated modules
		if gatedFiles[path] {
			return nil
		}

		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}

		lines := strings.Split(string(data), "\n")
		for i, line := range lines {
			matches := usePattern.FindStringSubmatch(line)
			if matches == nil {
				continue
			}
			crateName := matches[1]
			if !macOSModules[crateName] {
				continue
			}

			// Found a use of a macOS-only crate. Check if it's properly gated.
			if hasMacOSCfgAttribute(lines, i) {
				gatedUseCount++
			} else {
				// Compute relative path from the repo root's grandparent for display
				// We want paths like apps/desktop/src-tauri/src/foo.rs
				// srcDir is <root>/apps/desktop/src-tauri/src, so go up 4 levels to get root
				rootDir := filepath.Dir(filepath.Dir(filepath.Dir(filepath.Dir(srcDir))))
				relPath, relErr := filepath.Rel(rootDir, path)
				if relErr != nil {
					relPath = path
				}
				violations = append(violations, violation{
					relPath:   relPath,
					line:      i + 1, // 1-indexed
					crateName: crateName,
				})
			}
		}

		return nil
	})

	return violations, gatedUseCount, err
}

// attrLinePattern matches lines that look like attributes: #[...] or continuation of multi-line attributes.
var attrLinePattern = regexp.MustCompile(`^\s*#\[`)

// hasMacOSCfgAttribute walks backwards from lineIdx, skipping blank lines and attribute lines,
// to check if any preceding attribute contains target_os = "macos" (and not negated with not(...)).
// Also handles `use` statements inside cfg-gated blocks (e.g., inside a #[cfg(target_os = "macos")] fn).
func hasMacOSCfgAttribute(lines []string, lineIdx int) bool {
	for j := lineIdx - 1; j >= 0; j-- {
		trimmed := strings.TrimSpace(lines[j])

		// Skip blank lines
		if trimmed == "" {
			continue
		}

		// If this is an attribute line (starts with #[), check it
		if attrLinePattern.MatchString(lines[j]) {
			// This attribute might be multi-line. Collect the full attribute text.
			attrText := collectAttribute(lines, j)
			if isMacOSGateAttribute(attrText) {
				return true
			}
			// It's an attribute but not a macOS gate — keep walking (there could be stacked attributes)
			continue
		}

		// If we hit a line that's part of a multi-line attribute (doesn't start with #[
		// but looks like attribute content — e.g., ends with )] or contains attribute-like content),
		// skip it. We handle multi-line attributes by collecting from the #[ opener.
		if isAttributeContinuation(trimmed) {
			continue
		}

		// Hit a non-blank, non-attribute line. If it ends with '{', it could be a
		// function/block/impl opening that's itself cfg-gated (e.g., #[cfg(target_os = "macos")] fn foo() {).
		// Recursively check the attributes above this enclosing block.
		if strings.HasSuffix(trimmed, "{") {
			if hasMacOSCfgAttribute(lines, j) {
				return true
			}
		}

		// Stop walking — this is a regular code line
		break
	}
	return false
}

// collectAttribute collects the full text of an attribute starting at the given line index.
// Handles multi-line attributes by reading forward until the closing `]`.
func collectAttribute(lines []string, startIdx int) string {
	var sb strings.Builder
	for i := startIdx; i < len(lines); i++ {
		sb.WriteString(lines[i])
		sb.WriteString(" ")
		trimmed := strings.TrimSpace(lines[i])
		// Count brackets to determine if the attribute is complete
		if strings.Contains(trimmed, "]") {
			openCount := strings.Count(sb.String(), "[")
			closeCount := strings.Count(sb.String(), "]")
			if closeCount >= openCount {
				break
			}
		}
	}
	return sb.String()
}

// isMacOSGateAttribute checks whether an attribute text contains a macOS cfg gate.
// Returns true for #[cfg(target_os = "macos")] and compound forms like
// #[cfg(all(test, target_os = "macos"))], but false for #[cfg(not(target_os = "macos"))].
func isMacOSGateAttribute(attrText string) bool {
	if !strings.Contains(attrText, `target_os = "macos"`) {
		return false
	}

	// Check for negation: not(...target_os = "macos"...)
	// Find the position of target_os = "macos" and walk backwards to see if it's inside a not()
	idx := strings.Index(attrText, `target_os = "macos"`)
	prefix := attrText[:idx]

	// Check if 'not(' appears after the last closing ')' before our match
	// Simple heuristic: count unmatched not( before the target_os
	// Walk backwards from the target_os position looking for not(
	lastNotIdx := strings.LastIndex(prefix, "not(")
	if lastNotIdx == -1 {
		return true // No negation
	}

	// Check if the not( is still "open" (more opens than closes between not( and target_os)
	between := prefix[lastNotIdx+4:] // after "not("
	openParens := strings.Count(between, "(")
	closeParens := strings.Count(between, ")")
	// If closeParens > openParens, the not() was already closed before target_os
	return closeParens > openParens
}

// isAttributeContinuation returns true if a line looks like it's a continuation of a
// multi-line attribute (inside #[...] but not starting with #[).
func isAttributeContinuation(trimmed string) bool {
	// Common patterns for attribute continuations:
	// - Lines ending with ) or )] or ],
	// - Lines starting with content that looks like inside an attribute (e.g., "NSURL", feature lists)
	// - Lines that are just "]" or ")]"
	if trimmed == "]" || trimmed == ")]" || trimmed == ")," || trimmed == "]," {
		return true
	}
	// Lines that look like they're inside a feature array or attribute arguments
	// (start with a quote or identifier followed by comma)
	if strings.HasPrefix(trimmed, "\"") || strings.HasSuffix(trimmed, ",") || strings.HasSuffix(trimmed, "),") {
		return true
	}
	return false
}
