package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The write engine's local-disk boundary, made self-defending.
//
// `cmdr-operations` builds for the browser demo, which has no local disk. So the host OS is
// named in two places only: the local-engine module trees (gated out of the browser build) and
// `local_disk/`, the primitives the shared code asks through. Anywhere else, a `libc` call or a
// `std::os::unix` import is a host-disk fact smuggled into code the browser runs.
// `crates/cmdr-operations/DETAILS.md` § "The browser build".

// operationsSrcDir is the crate this check governs.
const operationsSrcDir = "crates/cmdr-operations/src"

// operationsLocalDiskModules may name the host OS: files, or directories ending in `/`, relative
// to the crate's `src/`.
var operationsLocalDiskModules = []string{
	"local_disk/",
	"transfer/copy/",
	"transfer/move_op/",
	"transfer/chunked_copy.rs",
	"transfer/macos_copy.rs",
	"transfer/linux_copy.rs",
	"transfer/copy_strategy.rs",
	"delete/walker.rs",
	"delete/trash.rs",
	"delete/cloud_trash.rs",
	"scan.rs",
	"scan_dry_run.rs",
	"scan_walker.rs",
	"cancellable.rs",
	"scan_preview/local.rs",
	"in_flight_sweep.rs",
	"in_flight_sweep_surface.rs",
	"unblocked_removal.rs",
	"local_start.rs",
	"scratch_dir.rs",
	"overwrite.rs",
	"overwrite_aside_park.rs",
	"durability.rs",
	"mount_boundary.rs",
	"free_space.rs",
}

var (
	// A host-OS crate at the start of a path (`libc::access`, `use xattr::get`), never a
	// module of ours that shares the name (`delete::trash::`, preceded by `::`).
	operationsHostCratePath = regexp.MustCompile(`(?:^|[^:\w])(libc|cmdr_local|objc2|xattr|trash|dirs)::`)
	operationsHostCrateUse  = regexp.MustCompile(`\buse\s+(libc|cmdr_local|objc2|xattr|trash|dirs)\b`)
	operationsUnixImport    = regexp.MustCompile(`\bstd::os::unix\b`)
	// A test-only module: `#[cfg(test)]` / `#[cfg(all(test, ...))]`, then `mod name;` or `mod name {`.
	operationsTestModule = regexp.MustCompile(`(?m)^\s*#\[cfg\([^\]]*\btest\b[^\]]*\)\]\s*\n\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*([;{])`)
)

// RunOperationsLocalDiskBoundary pins where `cmdr-operations` may name the host OS.
func RunOperationsLocalDiskBoundary(ctx *CheckContext) (CheckResult, error) {
	srcDir := filepath.Join(ctx.RootDir, operationsSrcDir)
	files, err := operationsRustFiles(srcDir)
	if err != nil {
		return CheckResult{}, err
	}
	if len(files) == 0 {
		return CheckResult{}, fmt.Errorf("found no Rust files in %s; update this check if the crate moved", operationsSrcDir)
	}
	violations, scanned := operationsBoundaryViolations(files)
	if len(violations) > 0 {
		return CheckResult{}, fmt.Errorf(
			"found %d host-OS %s outside `cmdr-operations`' local-disk modules. The browser build has no local disk, so ask through `local_disk/` (add the primitive there, with its browser answer), or, for a local-engine module, add it to `operationsLocalDiskModules` and gate it out of the browser build:\n%s",
			len(violations), Pluralize(len(violations), "name", "names"),
			indentOutput(strings.Join(violations, "\n")),
		)
	}
	return Success(fmt.Sprintf(
		"%d production %s scanned, the local-disk boundary holds",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

// operationsRustFiles reads every `.rs` file under srcDir, keyed by its path relative to srcDir
// (slash-separated).
func operationsRustFiles(srcDir string) (map[string]string, error) {
	files := map[string]string{}
	err := corpus.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		source, readErr := corpus.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		rel, relErr := filepath.Rel(srcDir, path)
		if relErr != nil {
			return relErr
		}
		files[filepath.ToSlash(rel)] = source
		return nil
	})
	return files, err
}

// operationsBoundaryViolations judges the crate's files (relative path → source) and answers
// each violation as `path:line: text`, plus how many production files it read.
func operationsBoundaryViolations(files map[string]string) ([]string, int) {
	testOnly := operationsTestOnlyPaths(files)
	paths := make([]string, 0, len(files))
	for path := range files {
		paths = append(paths, path)
	}
	sort.Strings(paths)

	var violations []string
	scanned := 0
	for _, path := range paths {
		if isOperationsTestPath(path, testOnly) || isOperationsLocalDiskModule(path) {
			continue
		}
		scanned++
		code := blankRustTestModules(stripRustLineComments(files[path]))
		for i, line := range strings.Split(code, "\n") {
			if operationsHostCratePath.MatchString(line) || operationsHostCrateUse.MatchString(line) ||
				operationsUnixImport.MatchString(line) {
				violations = append(violations, fmt.Sprintf("crates/cmdr-operations/src/%s:%d: %s", path, i+1, strings.TrimSpace(line)))
			}
		}
	}
	return violations, scanned
}

// operationsTestOnlyPaths finds the files a `#[cfg(test)] mod name;` declares, as `name.rs`
// and `name/` beside the declaring module.
func operationsTestOnlyPaths(files map[string]string) []string {
	var paths []string
	for path, source := range files {
		dir := filepath.ToSlash(filepath.Dir(path))
		base := filepath.Base(path)
		if base != "lib.rs" && base != "mod.rs" {
			// `foo.rs` declares its children in `foo/`.
			dir = strings.TrimSuffix(path, ".rs")
		}
		prefix := ""
		if dir != "." {
			prefix = dir + "/"
		}
		for _, m := range operationsTestModule.FindAllStringSubmatch(stripRustLineComments(source), -1) {
			if m[2] == ";" {
				paths = append(paths, prefix+m[1]+".rs", prefix+m[1]+"/")
			}
		}
	}
	return paths
}

func isOperationsTestPath(path string, testOnly []string) bool {
	base := filepath.Base(path)
	if strings.HasSuffix(base, "_test.rs") || strings.HasSuffix(base, "_tests.rs") || base == "tests.rs" ||
		strings.HasPrefix(path, "testing/") || strings.Contains(path, "/tests/") || strings.HasPrefix(path, "tests/") {
		return true
	}
	return matchesOperationsModule(path, testOnly)
}

func isOperationsLocalDiskModule(path string) bool {
	return matchesOperationsModule(path, operationsLocalDiskModules)
}

func matchesOperationsModule(path string, modules []string) bool {
	for _, module := range modules {
		if strings.HasSuffix(module, "/") && strings.HasPrefix(path, module) || path == module {
			return true
		}
	}
	return false
}

// blankRustTestModules blanks every inline `#[cfg(test)] mod name { ... }` body (to its matching
// brace), keeping line numbers true. Comments are already stripped, so a brace in a comment can't
// unbalance it; one in a string literal could, which test modules here don't have.
func blankRustTestModules(code string) string {
	out := []byte(code)
	for _, m := range operationsTestModule.FindAllStringSubmatchIndex(code, -1) {
		if code[m[4]:m[5]] != "{" {
			continue
		}
		depth := 0
		for i := m[4]; i < len(out); i++ {
			switch out[i] {
			case '{':
				depth++
			case '}':
				depth--
			}
			if out[i] != '\n' {
				out[i] = ' '
			}
			if depth == 0 {
				break
			}
		}
	}
	return string(out)
}
