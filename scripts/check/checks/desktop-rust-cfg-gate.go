package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// RunCfgGate verifies that Rust code properly gates macOS-only imports with
// #[cfg(target_os = "macos")]. Two name sets are watched: macOS-only DEPENDENCY crates
// declared in the manifest, and the crate's own macOS-only MODULES (`#[cfg(target_os =
// "macos")] mod x;`). Both break the Linux build with nothing red on a Mac, and the
// second is the easier one to reach by accident: an insert directly under an existing
// `#[cfg]` line steals that attribute from the `use` it used to gate.
//
// It works in (manifest, source root) PAIRS, one per workspace member, because a
// macOS-only dependency is declared in the manifest of the crate that uses it.
// Reading one manifest while scanning several trees would miss every macOS-only dep
// a crate declares for itself; scanning one tree would miss every import in the
// others. Either way the failure surfaces as a broken Linux build rather than a red
// check — commit aabc4cb11 ("declare `libmimalloc-sys` as macOS-only so Linux builds
// again") is what that looks like in practice.
//
// A member that declares itself macOS-only is skipped: its gate is at the crate
// level, so a per-import `#[cfg]` inside it would protect nothing.
func RunCfgGate(ctx *CheckContext) (CheckResult, error) {
	kinds, err := ScannerMemberKinds("desktop-rust-cfg-gate")
	if err != nil {
		return CheckResult{}, err
	}
	members, err := MembersOfKind(ctx.RootDir, kinds...)
	if err != nil {
		return CheckResult{}, err
	}

	var totals cfgGateTotals
	for _, m := range members {
		if err := scanMemberForCfgGates(ctx.RootDir, m, &totals); err != nil {
			return CheckResult{}, err
		}
	}

	violations, gatedUseCount := totals.violations, totals.gatedUses
	macOSCrateCount, macOSModuleCount := totals.macOSCrates, totals.macOSModules
	gatedFileCount, scannedMembers := totals.gatedFiles, totals.scannedMembers

	if len(violations) > 0 {
		return CheckResult{}, cfgGateViolationError(violations)
	}

	if scannedMembers == 0 {
		return Success("No macOS-only dependencies found"), nil
	}
	return Success(fmt.Sprintf(
		"%d gated %s of %d macOS-only %s and %d macOS-only %s verified across %d workspace %s (%d %s skipped via module-level gating)",
		gatedUseCount, Pluralize(gatedUseCount, "use", "uses"),
		macOSCrateCount, Pluralize(macOSCrateCount, "crate", "crates"),
		macOSModuleCount, Pluralize(macOSModuleCount, "module", "modules"),
		scannedMembers, Pluralize(scannedMembers, "member", "members"),
		gatedFileCount, Pluralize(gatedFileCount, "file", "files"),
	)), nil
}

// cfgGateTotals accumulates what one pass over the workspace members found.
type cfgGateTotals struct {
	violations     []violation
	gatedUses      int
	macOSCrates    int
	macOSModules   int
	gatedFiles     int
	scannedMembers int
}

// scanMemberForCfgGates folds one workspace member into the running totals. A member that
// doesn't build on Linux, has no source tree, or names nothing macOS-only contributes nothing.
func scanMemberForCfgGates(rootDir string, m WorkspaceMember, totals *cfgGateTotals) error {
	if !m.BuildsOn("linux") {
		return nil
	}
	if info, err := os.Stat(m.SrcDir); err != nil || !info.IsDir() {
		return nil
	}

	// Step 1: this member's own macOS-only crate names, and its own macOS-only module paths.
	macOSModules, err := extractMacOSCrateModules(m.ManifestPath)
	if err != nil {
		return fmt.Errorf("failed to parse %s: %w", m.RelDir(rootDir)+"/Cargo.toml", err)
	}
	macOSOnlyMods, err := macOSOnlyModulePaths(m.SrcDir)
	if err != nil {
		return fmt.Errorf("failed to collect macOS-only modules: %w", err)
	}
	if len(macOSModules) == 0 && len(macOSOnlyMods) == 0 {
		return nil
	}
	totals.scannedMembers++
	totals.macOSCrates += len(macOSModules)
	totals.macOSModules += len(macOSOnlyMods)

	// Step 2: files inside cfg(target_os = "macos") modules are inherently gated.
	gatedFiles, err := buildModuleGatedFileSet(m.SrcDir)
	if err != nil {
		return fmt.Errorf("failed to build module-gated file set: %w", err)
	}
	totals.gatedFiles += len(gatedFiles)

	// Step 3: the remaining files must gate every use of a macOS-only crate or module.
	memberViolations, memberGatedUses, err := scanForUngatedUses(
		rootDir, m.SrcDir, macOSModules, moduleRefPattern(macOSOnlyMods), gatedFiles)
	if err != nil {
		return fmt.Errorf("failed to scan Rust files: %w", err)
	}
	totals.violations = append(totals.violations, memberViolations...)
	totals.gatedUses += memberGatedUses
	return nil
}

// cfgGateViolationError renders the findings, ordered by file then line so the list reads
// the same on every run.
func cfgGateViolationError(violations []violation) error {
	sort.Slice(violations, func(i, j int) bool {
		if violations[i].relPath == violations[j].relPath {
			return violations[i].line < violations[j].line
		}
		return violations[i].relPath < violations[j].relPath
	})
	var sb strings.Builder
	for _, v := range violations {
		sb.WriteString(fmt.Sprintf("  %s:%d: use of macOS-only %s '%s' without #[cfg(target_os = \"macos\")]\n",
			v.relPath, v.line, v.kind, v.name))
	}
	return fmt.Errorf(
		"found %d ungated %s of macOS-only crates or modules:\n%s",
		len(violations), Pluralize(len(violations), "use", "uses"), sb.String(),
	)
}

// violation records a single ungated use of a macOS-only crate or module.
type violation struct {
	relPath string
	line    int
	// kind is "crate" or "module", so the message says which name set was hit.
	kind string
	name string
}

// extractMacOSCrateModules parses Cargo.toml and returns the set of Rust module names
// (hyphens converted to underscores) for crates declared under [target.'cfg(target_os = "macos")'.dependencies].
//
// A crate ALSO declared unconditionally (in `[dependencies]` or `[dev-dependencies]`) is
// left out: it links on every target through that second declaration, so naming it
// outside a gate breaks nothing. `tar` is the live example, macOS-only for production
// extraction and an all-target dev-dependency so the tarball-building test compiles in
// the Linux lane.
func extractMacOSCrateModules(cargoPath string) (map[string]bool, error) {
	var cargo map[string]any
	if _, err := toml.DecodeFile(cargoPath, &cargo); err != nil {
		return nil, err
	}

	allTargets := allTargetDepNames(cargo)

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
		if allTargets[crateName] {
			continue
		}
		moduleName := strings.ReplaceAll(crateName, "-", "_")
		modules[moduleName] = true
	}
	return modules, nil
}

// allTargetDepNames collects the crate names a manifest declares for every target, from
// the unconditional `[dependencies]` and `[dev-dependencies]` tables.
func allTargetDepNames(cargo map[string]any) map[string]bool {
	names := map[string]bool{}
	for _, table := range []string{"dependencies", "dev-dependencies"} {
		deps, ok := cargo[table].(map[string]any)
		if !ok {
			continue
		}
		for crateName := range deps {
			names[crateName] = true
		}
	}
	return names
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
		if d.IsDir() || !isModuleRootFile(d.Name()) {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		dir := filepath.Dir(path)
		for _, modName := range findCfgGatedModules(strings.Split(string(data), "\n")) {
			for _, f := range moduleFiles(dir, modName) {
				gatedFiles[f] = true
			}
		}
		return nil
	})
	if err != nil {
		return nil, err
	}

	closeOverModuleChildren(gatedFiles)
	return gatedFiles, nil
}

// isModuleRootFile reports whether a file name is one whose `mod x;` declarations resolve
// against its own directory.
func isModuleRootFile(name string) bool {
	return name == "lib.rs" || name == "main.rs" || name == "mod.rs"
}

// closeOverModuleChildren grows a gated file set to every file those files pull in. A gated
// `payload.rs` may bring in a sibling through `#[path = "payload_tests.rs"] mod payload_tests;`,
// and that sibling is as absent from the Linux build as `payload.rs` is; scanning it for gates
// would report every import in it.
func closeOverModuleChildren(gatedFiles map[string]bool) {
	queue := make([]string, 0, len(gatedFiles))
	for f := range gatedFiles {
		queue = append(queue, f)
	}
	for len(queue) > 0 {
		f := queue[len(queue)-1]
		queue = queue[:len(queue)-1]
		for _, child := range childModuleFiles(f) {
			if !gatedFiles[child] {
				gatedFiles[child] = true
				queue = append(queue, child)
			}
		}
	}
}

// moduleFiles resolves a `mod <name>;` declared against moduleDir to the files it brings in:
// `<moduleDir>/<name>.rs`, or `mod.rs` plus every .rs under `<moduleDir>/<name>/`.
func moduleFiles(moduleDir, modName string) []string {
	sub := filepath.Join(moduleDir, modName)
	if info, err := os.Stat(sub); err == nil && info.IsDir() {
		return append([]string{filepath.Join(sub, "mod.rs")}, rustFilesUnder(sub)...)
	}
	single := filepath.Join(moduleDir, modName+".rs")
	if _, err := os.Stat(single); err == nil {
		return []string{single}
	}
	return nil
}

// rustFilesUnder lists every .rs file in a directory tree.
func rustFilesUnder(dir string) []string {
	var found []string
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !d.IsDir() && strings.HasSuffix(d.Name(), ".rs") {
			found = append(found, path)
		}
		return nil
	})
	return found
}

// pathAttrPattern captures the file named by a `#[path = "..."]` attribute.
var pathAttrPattern = regexp.MustCompile(`#\[path\s*=\s*"([^"]+)"\]`)

// childModuleFiles returns the files of the child modules a Rust source file declares. A
// `#[path = "..."]` attribute resolves against the directory holding the declaring file;
// a plain `mod x;` resolves against that file's own module directory, which for `lib.rs`,
// `main.rs`, and `mod.rs` is the same directory and for `foo.rs` is `foo/`.
func childModuleFiles(path string) []string {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	dir := filepath.Dir(path)
	modDir := dir
	if !isModuleRootFile(filepath.Base(path)) {
		modDir = filepath.Join(dir, strings.TrimSuffix(filepath.Base(path), ".rs"))
	}

	var children []string
	lines := strings.Split(string(data), "\n")
	for i, line := range lines {
		matches := modDeclPattern.FindStringSubmatch(strings.TrimSpace(line))
		if matches == nil {
			continue
		}
		if explicit := explicitModulePath(lines, i); explicit != "" {
			children = append(children, filepath.Join(dir, explicit))
			continue
		}
		children = append(children, moduleFiles(modDir, matches[1])...)
	}
	return children
}

// explicitModulePath returns the file named by a `#[path = "..."]` attribute directly above
// a module declaration, or "" when there is none.
func explicitModulePath(lines []string, modLineIdx int) string {
	for _, attrText := range directAttributesAbove(lines, modLineIdx) {
		if m := pathAttrPattern.FindStringSubmatch(attrText); m != nil {
			return m[1]
		}
	}
	return ""
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

// crateRefPattern matches every path-qualified reference to a crate root on a line:
// `use libc::…` and a bare `unsafe { libc::geteuid() }` alike. Matching only `use` lines
// would miss the inline form, which compiles on macOS and breaks the Linux build with
// nothing red locally.
var crateRefPattern = regexp.MustCompile(`\b(\w+)::`)

// lineCommentPattern strips a `//` comment (including `///` and `//!`) so prose naming a
// macOS-only crate doesn't read as a use of it. Block comments aren't stripped: naming a
// crate path inside one is rare enough to fix by rewording.
var lineCommentPattern = regexp.MustCompile(`//.*$`)

// macOSCratesReferencedOn returns the macOS-only crates a line reaches for, each at most
// once, in first-appearance order so violations stay stably ordered.
func macOSCratesReferencedOn(line string, macOSModules map[string]bool) []string {
	code := lineCommentPattern.ReplaceAllString(line, "")
	var found []string
	seen := map[string]bool{}
	for _, m := range crateRefPattern.FindAllStringSubmatch(code, -1) {
		name := m[1]
		if !macOSModules[name] || seen[name] {
			continue
		}
		seen[name] = true
		found = append(found, name)
	}
	return found
}

// useLinePattern matches an import line, the only form the module lane reads.
var useLinePattern = regexp.MustCompile(`^\s*(?:pub(?:\s*\((?:crate|super|in [^)]*)\))?\s+)?use\s`)

// macOSModulesReferencedOn returns the macOS-only module paths an IMPORT line reaches for
// through a `crate::`-qualified path, each at most once, in first-appearance order.
//
// Two deliberate narrowings, both there to keep the lane quiet enough to trust:
//
//   - Only the `crate::`-qualified form. A relative `super::` or bare-sibling reference
//     resolves against the referring file's own module, which this line-based scan doesn't
//     know, and guessing would flag correct code.
//   - Only `use` lines. Unlike a macOS-only DEPENDENCY crate, a macOS-only module of our own
//     is named all over ordinary code: inside the `ipc.rs` command-registry macro, in a
//     closure body, in a multi-line signature or `let`. Deciding whether such a line sits
//     under a gate needs a real parse, and the line-based walk misreads enough of them that
//     the noise would bury the finding. An import sits at the top of its scope, where the
//     walk is reliable, and it is the shape that broke the Linux build: a `use` inserted
//     directly under an existing `#[cfg]` steals that attribute from the import below it.
func macOSModulesReferencedOn(line string, modRefs *regexp.Regexp) []string {
	if modRefs == nil || !useLinePattern.MatchString(line) {
		return nil
	}
	code := lineCommentPattern.ReplaceAllString(line, "")
	var found []string
	seen := map[string]bool{}
	for _, m := range modRefs.FindAllStringSubmatch(code, -1) {
		name := m[1]
		if seen[name] {
			continue
		}
		seen[name] = true
		found = append(found, name)
	}
	return found
}

// moduleRefPattern compiles the alternation that spots a `crate::<path>` reference to any
// of the given macOS-only module paths. Longest path first, so `crate::mtp::macos_workaround`
// reports the nested module rather than its parent. Returns nil when there is nothing to match.
func moduleRefPattern(modPaths []string) *regexp.Regexp {
	if len(modPaths) == 0 {
		return nil
	}
	sorted := append([]string(nil), modPaths...)
	sort.Slice(sorted, func(i, j int) bool {
		if len(sorted[i]) != len(sorted[j]) {
			return len(sorted[i]) > len(sorted[j])
		}
		return sorted[i] < sorted[j]
	})
	quoted := make([]string, 0, len(sorted))
	for _, p := range sorted {
		quoted = append(quoted, regexp.QuoteMeta(p))
	}
	return regexp.MustCompile(`\bcrate::(` + strings.Join(quoted, "|") + `)\b`)
}

// macOSOnlyModulePaths walks lib.rs and mod.rs files and returns the crate-relative paths
// of every module gated on macOS AND NOTHING ELSE, for example "native_drag" or
// "mtp::macos_workaround".
//
// A module gated `#[cfg(any(target_os = "macos", target_os = "linux"))]` is deliberately
// left out: it exists in the Linux lane, so naming it outside a gate compiles there.
func macOSOnlyModulePaths(srcDir string) ([]string, error) {
	var paths []string

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
		prefix, ok := modulePathPrefix(srcDir, path)
		if !ok {
			return nil
		}
		for _, modName := range findMacOSOnlyModules(strings.Split(string(data), "\n")) {
			if prefix == "" {
				paths = append(paths, modName)
			} else {
				paths = append(paths, prefix+"::"+modName)
			}
		}
		return nil
	})

	sort.Strings(paths)
	return paths, err
}

// modulePathPrefix turns a lib.rs/mod.rs path into the crate-relative module path of the
// module that file defines: `src/lib.rs` gives "", `src/mtp/mod.rs` gives "mtp".
func modulePathPrefix(srcDir, path string) (string, bool) {
	rel, err := filepath.Rel(srcDir, path)
	if err != nil {
		return "", false
	}
	dir := filepath.Dir(rel)
	if dir == "." {
		return "", true
	}
	if strings.HasPrefix(dir, "..") {
		return "", false
	}
	return strings.ReplaceAll(dir, string(filepath.Separator), "::"), true
}

// findMacOSOnlyModules returns the names of `mod x;` declarations gated on macOS and
// nothing else.
func findMacOSOnlyModules(lines []string) []string {
	var result []string
	for i, line := range lines {
		matches := modDeclPattern.FindStringSubmatch(strings.TrimSpace(line))
		if matches == nil {
			continue
		}
		for _, attrText := range directAttributesAbove(lines, i) {
			if isExclusivelyMacOSGateAttribute(attrText) {
				result = append(result, matches[1])
				break
			}
		}
	}
	return result
}

// isExclusivelyMacOSGateAttribute reports whether an attribute gates on macOS and on no
// other platform, so what it guards is absent from the Linux build.
func isExclusivelyMacOSGateAttribute(attrText string) bool {
	return isMacOSGateAttribute(attrText) && strings.Count(attrText, `target_os = `) == 1
}

// scanForUngatedUses walks all .rs files, skipping gated files, and checks that
// uses of macOS-only crates and macOS-only modules are properly gated. `modRefs` may be
// nil when the member declares no macOS-only modules. Returns violations and the count
// of properly gated uses found.
func scanForUngatedUses(
	rootDir, srcDir string,
	macOSModules map[string]bool,
	modRefs *regexp.Regexp,
	gatedFiles map[string]bool,
) ([]violation, int, error) {
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
			hits := make([]violation, 0, 2)
			for _, crateName := range macOSCratesReferencedOn(line, macOSModules) {
				hits = append(hits, violation{kind: "crate", name: crateName})
			}
			for _, modPath := range macOSModulesReferencedOn(line, modRefs) {
				hits = append(hits, violation{kind: "module", name: "crate::" + modPath})
			}
			if len(hits) == 0 {
				continue
			}
			// One gate decision per line, whatever it names.
			gated := hasMacOSCfgAttribute(lines, i)
			relPath, relErr := filepath.Rel(rootDir, path)
			if relErr != nil {
				relPath = path
			}
			for _, hit := range hits {
				if gated {
					gatedUseCount++
					continue
				}
				hit.relPath = relPath
				hit.line = i + 1 // 1-indexed
				violations = append(violations, hit)
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
	// Phase 1: Check for cfg attributes directly above this line (no code lines in between).
	if hasDirectCfgAttribute(lines, lineIdx) {
		return true
	}

	// Phase 2: Find the enclosing block via brace tracking and check if it's cfg-gated.
	return isInsideCfgGatedBlock(lines, lineIdx)
}

// hasDirectCfgAttribute checks whether a macOS cfg attribute appears directly above lineIdx,
// separated only by blank lines and other attributes.
func hasDirectCfgAttribute(lines []string, lineIdx int) bool {
	for _, attrText := range directAttributesAbove(lines, lineIdx) {
		if isMacOSGateAttribute(attrText) {
			return true
		}
	}
	return false
}

// directAttributesAbove returns the full text of each attribute sitting directly above
// lineIdx, separated from it only by blank lines and other attributes, nearest first.
func directAttributesAbove(lines []string, lineIdx int) []string {
	// Brackets still open once a line has been read. While this is positive we're in the
	// middle of a multi-line attribute, whatever the line happens to look like: the inner
	// `)` of a nested `#[cfg_attr(feature = "x", allow(...))]` is a shape no list of line
	// forms can enumerate, and stopping there reads a gated item as ungated.
	openBrackets := 0
	var attrs []string

	for j := lineIdx - 1; j >= 0; j-- {
		trimmed := strings.TrimSpace(lines[j])

		if trimmed == "" {
			continue
		}

		openBrackets += strings.Count(trimmed, ")") + strings.Count(trimmed, "]") -
			strings.Count(trimmed, "(") - strings.Count(trimmed, "[")
		if openBrackets < 0 {
			openBrackets = 0
		}
		if openBrackets > 0 {
			continue
		}

		if attrLinePattern.MatchString(lines[j]) {
			attrs = append(attrs, collectAttribute(lines, j))
			continue
		}

		if isAttributeContinuation(trimmed) {
			continue
		}

		// Hit a code line: no further attributes belong to this item.
		break
	}
	return attrs
}

// isInsideCfgGatedBlock walks backwards from lineIdx, tracking brace depth, to find the
// enclosing block opener. If found, it recursively checks whether that block is cfg-gated.
func isInsideCfgGatedBlock(lines []string, lineIdx int) bool {
	braceDepth := 0
	for j := lineIdx - 1; j >= 0; j-- {
		trimmed := strings.TrimSpace(lines[j])
		if trimmed == "" {
			continue
		}

		braceDepth += strings.Count(trimmed, "}") - strings.Count(trimmed, "{")

		if braceDepth < 0 {
			// Found an unmatched '{': this is the enclosing block.
			// Check if it (or its enclosing scope) has a macOS cfg gate.
			return hasMacOSCfgAttribute(lines, j)
		}
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
