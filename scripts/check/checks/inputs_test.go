package checks

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// repoRootForTest walks up from the test's working directory to the repo root,
// using the same landmark `findRootDir` does. Tests that assert something about
// the REAL tree (rather than a fixture) need it; a fixture can't tell us whether
// today's sources embed a file today's Inputs forget.
func repoRootForTest(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	for {
		if _, statErr := os.Stat(filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")); statErr == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatalf("couldn't find the repo root above %s", dir)
		}
		dir = parent
	}
}

// embeddedPathRE captures the literal path of an `include_str!`/`include_bytes!`.
// Non-literal forms (a `concat!`, a `env!("CARGO_MANIFEST_DIR")` join) don't match
// and aren't checked; nothing in the tree uses one today, and a regex that tried
// would report paths that don't exist.
var embeddedPathRE = regexp.MustCompile(`include_(?:str|bytes)!\s*\(\s*"([^"]+)"`)

// TestRustInputsCoverEveryEmbeddedFile is the guardrail behind `rustInputs`'
// exclusions. A file pulled into the binary with `include_str!` is a compile-time
// input just as much as a `.rs` file: change it and the tests can change verdict.
// If such a file falls outside `rustInputs`, every Rust lane cache-skips the edit
// and reports a green that describes the previous content.
//
// This is not hypothetical. `whats_new` embeds the repo-root `CHANGELOG.md`, which
// `rustInputs` didn't list at all, so a changelog edit never re-ran the Rust tests.
func TestRustInputsCoverEveryEmbeddedFile(t *testing.T) {
	root := repoRootForTest(t)
	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	patterns := inputs(rustInputs, GlobalInputs)

	checked := 0
	for _, m := range members {
		walkErr := filepath.WalkDir(m.SrcDir, func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() || !strings.HasSuffix(path, ".rs") {
				return nil
			}
			source, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			for _, match := range embeddedPathRE.FindAllStringSubmatch(string(source), -1) {
				embedded := filepath.Clean(filepath.Join(filepath.Dir(path), filepath.FromSlash(match[1])))
				rel, relErr := filepath.Rel(root, embedded)
				if relErr != nil || strings.HasPrefix(rel, "..") {
					continue // outside the repo; not something Inputs can express
				}
				checked++
				if !matchesAny(filepath.ToSlash(rel), patterns) {
					t.Errorf("%s embeds %s, which `rustInputs` doesn't cover: editing it would be invisible to every Rust lane",
						mustRel(t, root, path), filepath.ToSlash(rel))
				}
			}
			return nil
		})
		if walkErr != nil && !os.IsNotExist(walkErr) {
			t.Fatalf("walking %s: %v", m.SrcDir, walkErr)
		}
	}
	if checked == 0 {
		t.Fatal("found no `include_str!`/`include_bytes!` call at all; the scan is broken, not the tree")
	}
}

// TestRustInputsExcludeOnlyAgentDocs keeps the exclusions honest: they may take
// out the colocated agent docs and nothing else, so a future `!` entry can't
// quietly hide a source tree from the cache.
func TestRustInputsExcludeOnlyAgentDocs(t *testing.T) {
	allowed := map[string]bool{"!**/CLAUDE.md": true, "!**/DETAILS.md": true}
	for _, pattern := range rustInputs {
		if strings.HasPrefix(pattern, "!") && !allowed[pattern] {
			t.Errorf("`rustInputs` excludes %q; only the agent docs may be excluded, and a new exclusion needs its own reasoning and its own test", pattern)
		}
	}
}

func mustRel(t *testing.T, root, path string) string {
	t.Helper()
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return path
	}
	return filepath.ToSlash(rel)
}
