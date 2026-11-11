package checks

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// The `cfg-gate` check's module resolution: which files a `mod x;` declaration brings in,
// and which of those only macOS compiles (a `#[cfg(target_os = "macos")]` module, or the
// macOS arm a platform seam picks). The scan and its verdicts are in desktop-rust-cfg-gate.go.

// modDeclPattern matches cfg-gated module declarations: optional visibility, then mod <name>;
var modDeclPattern = regexp.MustCompile(`^(?:pub(?:\s*\((?:crate|super)\))?\s+)?mod\s+(\w+)\s*;`)

// buildModuleGatedFileSet scans every Rust file for modules gated behind
// #[cfg(target_os = "macos")], then resolves them to actual file paths. Every file, not only
// lib.rs and mod.rs: a leaf `text_editor.rs` keeps its macOS half in a sibling through
// `#[cfg(target_os = "macos")] #[path = "text_editor_macos.rs"] mod imp;`.
// Returns a set of absolute file paths that are inherently gated.
func buildModuleGatedFileSet(srcDir string) (map[string]bool, error) {
	gatedFiles := make(map[string]bool)

	err := corpus.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		data, err := corpus.ReadFile(path)
		if err != nil {
			return err
		}
		lines := strings.Split(data, "\n")
		for _, f := range declaredModuleFiles(path, lines, findCfgGatedModules) {
			gatedFiles[f] = true
		}
		for _, f := range macOSSeamArmFiles(path, lines) {
			gatedFiles[f] = true
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
// `<moduleDir>/<name>.rs` and every .rs under `<moduleDir>/<name>/` (Rust 2018 keeps both),
// or `mod.rs` plus that tree.
func moduleFiles(moduleDir, modName string) []string {
	var files []string
	single := filepath.Join(moduleDir, modName+".rs")
	if _, err := os.Stat(single); err == nil {
		files = append(files, single)
	}
	sub := filepath.Join(moduleDir, modName)
	if info, err := os.Stat(sub); err == nil && info.IsDir() {
		if len(files) == 0 {
			files = append(files, filepath.Join(sub, "mod.rs"))
		}
		files = append(files, rustFilesUnder(sub)...)
	}
	return files
}

// rustFilesUnder lists every .rs file in a directory tree.
func rustFilesUnder(dir string) []string {
	var found []string
	_ = corpus.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
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

// macOSArmPathPattern captures the file a platform seam's `mod.rs` picks for macOS:
// `#[cfg_attr(target_os = "macos", path = "macos.rs")]` (`docs/guides/platform-seams.md`).
var macOSArmPathPattern = regexp.MustCompile(`#\[cfg_attr\(\s*target_os\s*=\s*"macos"\s*,\s*path\s*=\s*"([^"]+)"\s*\)\]`)

// macOSSeamArmFiles returns the macOS arm of every platform seam a file declares. The seam
// compiles that file only on macOS, as surely as a `#[cfg(target_os = "macos")] mod x;` would,
// so its imports need no gate of their own.
func macOSSeamArmFiles(path string, lines []string) []string {
	var files []string
	for _, decl := range findModuleDecls(lines) {
		for _, attrText := range directAttributesAbove(lines, decl.line) {
			if m := macOSArmPathPattern.FindStringSubmatch(attrText); m != nil {
				files = append(files, filepath.Join(filepath.Dir(path), m[1]))
			}
		}
	}
	return files
}

// childModuleFiles returns the files of every child module a Rust source file declares.
func childModuleFiles(path string) []string {
	data, err := corpus.ReadFile(path)
	if err != nil {
		return nil
	}
	return declaredModuleFiles(path, strings.Split(data, "\n"), findModuleDecls)
}

// declaredModuleFiles resolves the module declarations `find` picks out of a source file's
// lines to the files they bring in. A `#[path = "..."]` attribute resolves against the
// directory holding the declaring file; a plain `mod x;` resolves against that file's own
// module directory, which for `lib.rs`, `main.rs`, and `mod.rs` is the same directory and for
// `foo.rs` is `foo/`.
func declaredModuleFiles(path string, lines []string, find func([]string) []moduleDecl) []string {
	dir := filepath.Dir(path)
	modDir := dir
	if !isModuleRootFile(filepath.Base(path)) {
		modDir = filepath.Join(dir, strings.TrimSuffix(filepath.Base(path), ".rs"))
	}

	var files []string
	for _, decl := range find(lines) {
		if explicit := explicitModulePath(lines, decl.line); explicit != "" {
			files = append(files, filepath.Join(dir, explicit))
			continue
		}
		files = append(files, moduleFiles(modDir, decl.name)...)
	}
	return files
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

// moduleDecl is one `mod <name>;` declaration: the module's name, and the line it sits on,
// which is where its attributes (a `#[cfg]`, a `#[path]`) are read from.
type moduleDecl struct {
	name string
	line int
}

// findModuleDecls finds every `mod <name>;` declaration in the given lines.
func findModuleDecls(lines []string) []moduleDecl {
	var result []moduleDecl
	for i, line := range lines {
		if matches := modDeclPattern.FindStringSubmatch(strings.TrimSpace(line)); matches != nil {
			result = append(result, moduleDecl{name: matches[1], line: i})
		}
	}
	return result
}

// findCfgGatedModules finds the module declarations preceded by #[cfg(target_os = "macos")]
// in the given lines. Handles blank lines and other attributes between the cfg and the mod.
func findCfgGatedModules(lines []string) []moduleDecl {
	var result []moduleDecl
	for _, decl := range findModuleDecls(lines) {
		if hasMacOSCfgAttribute(lines, decl.line) {
			result = append(result, decl)
		}
	}
	return result
}
