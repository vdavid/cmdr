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

// TestInputSetsExcludeOnlyAgentDocs keeps the exclusions honest: a shared set may
// take out the colocated agent docs and nothing else, so a future `!` entry can't
// quietly hide a source tree from the cache. An exclusion is the one construct
// that can make an input set too NARROW, which is the failure mode that ships a
// regression, so a new one needs its own reasoning and its own test.
func TestInputSetsExcludeOnlyAgentDocs(t *testing.T) {
	allowed := map[string]bool{}
	for _, pattern := range agentDocExclusions {
		allowed[pattern] = true
	}
	for name, set := range map[string][]string{
		"rustInputs":      rustInputs,
		"svelteInputs":    svelteInputs,
		"websiteInputs":   websiteInputs,
		"apiServerInputs": apiServerInputs,
		"dashboardInputs": dashboardInputs,
		"goScriptsInputs": goScriptsInputs,
		"workflowsInputs": workflowsInputs,
		"wholeRepoInputs": wholeRepoInputs,
		"desktopApp":      desktopAppInputs(),
		"GlobalInputs":    GlobalInputs,
	} {
		for _, pattern := range set {
			if strings.HasPrefix(pattern, "!") && !allowed[pattern] {
				t.Errorf("`%s` excludes %q; only the agent docs may be excluded", name, pattern)
			}
		}
	}
}

// frontendDocImportRE catches an ESM/CJS import or a Vite glob whose specifier is
// a Markdown file, including the `?raw` / `?url` query forms Vite uses to inline
// one. mdReadRE catches the Node side, where the path is usually assembled
// (`readFileSync(join(dir, 'CLAUDE.md'))`) rather than a bare literal.
var (
	frontendDocImportRE = regexp.MustCompile("\\b(?:from|import|require|glob)\\s*\\(?\\s*['\"`][^'\"`]*\\.md(?:\\?[a-z]+)?['\"`]")
	mdReadRE            = regexp.MustCompile(`\breadFile(?:Sync)?\s*\([^)]*\.md['"` + "`" + `]`)
)

// frontendSourceRoots are the directories inside `svelteInputs` that hold code.
var frontendSourceRoots = []string{
	"apps/desktop/src",
	"apps/desktop/test",
	"apps/desktop/scripts",
	"apps/desktop/eslint-plugins",
	"eslint-plugins",
}

// TestNoFrontendSourceLoadsAgentDocs is the guardrail behind `svelteInputs`'
// share of `agentDocExclusions`, and the frontend's answer to
// `TestRustInputsCoverEveryEmbeddedFile`. Vite turns `import doc from './X.md?raw'`
// into a build input, so a module that did that would make a doc edit change what
// the lanes verify — while the exclusion tells the cache it can't. Prose
// references to a `CLAUDE.md` in comments and ESLint messages are everywhere and
// are not loads, which is why this matches on the load construct, not the name.
func TestNoFrontendSourceLoadsAgentDocs(t *testing.T) {
	assertMarkdownLoadScanWorks(t)

	root := repoRootForTest(t)
	scanned := 0
	for _, rel := range frontendSourceRoots {
		walkErr := filepath.WalkDir(filepath.Join(root, rel), func(path string, entry os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() {
				return skipDirIf(entry.Name() == "node_modules")
			}
			if !isFrontendSourceFile(path) {
				return nil
			}
			source, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			scanned++
			if loadsMarkdown(source) {
				t.Errorf("%s loads a Markdown file, but `svelteInputs` excludes the agent docs: drop the load, or narrow the exclusion", mustRel(t, root, path))
			}
			return nil
		})
		if walkErr != nil && !os.IsNotExist(walkErr) {
			t.Fatalf("walking %s: %v", rel, walkErr)
		}
	}
	if scanned == 0 {
		t.Fatal("found no frontend source file at all; the scan is broken, not the tree")
	}
}

// assertMarkdownLoadScanWorks proves the detector both ways before the tree scan
// trusts its silence: a scan that can't see a load would pass on any tree, and one
// that reads prose as a load would fail on every tree.
func assertMarkdownLoadScanWorks(t *testing.T) {
	t.Helper()
	for _, positive := range []string{
		"import doc from './CLAUDE.md?raw'",
		"const md = await import('../DETAILS.md')",
		"const md = require('./CLAUDE.md')",
		"import.meta.glob('./**/DETAILS.md', { eager: true })",
		"readFileSync(join(dir, 'CLAUDE.md'), 'utf8')",
	} {
		if !loadsMarkdown([]byte(positive)) {
			t.Errorf("the Markdown-load scan misses %q", positive)
		}
	}
	for _, negative := range []string{
		"// See `lib/ui/CLAUDE.md` § \"Focus trapping\".",
		"'narrow it with `isCommandId`. See `lib/commands/CLAUDE.md`.',",
		"import.meta.glob('./messages/*/*.json', { eager: true })",
	} {
		if loadsMarkdown([]byte(negative)) {
			t.Errorf("the Markdown-load scan reads %q as a load; a prose reference is not one", negative)
		}
	}
}

// loadsMarkdown reports whether the source pulls a Markdown file in as a build or
// runtime input.
func loadsMarkdown(source []byte) bool {
	return frontendDocImportRE.Match(source) || mdReadRE.Match(source)
}

// isFrontendSourceFile reports whether the path is a module the frontend toolchain
// compiles or lints.
func isFrontendSourceFile(path string) bool {
	switch filepath.Ext(path) {
	case ".ts", ".js", ".mjs", ".cjs", ".svelte":
		return true
	}
	return false
}

// skipDirIf turns a "should I descend?" question into WalkDir's answer.
func skipDirIf(skip bool) error {
	if skip {
		return filepath.SkipDir
	}
	return nil
}

func mustRel(t *testing.T, root, path string) string {
	t.Helper()
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return path
	}
	return filepath.ToSlash(rel)
}

// TestSvelteInputsSkipAgentDocs is the contract behind `svelteInputs`' exclusions:
// the frontend lanes must not re-run because a colocated `CLAUDE.md` changed.
// Those docs sit inside `apps/desktop/src/**` and `apps/desktop/test/**` and get
// edited on nearly every session by house rule, so without the veto a docs-only
// commit reruns the whole ~8,600-test Vitest suite plus every ESLint and typecheck
// pass.
func TestSvelteInputsSkipAgentDocs(t *testing.T) {
	patterns := inputs(svelteInputs, GlobalInputs)
	for _, doc := range []string{
		"apps/desktop/src/lib/ui/CLAUDE.md",
		"apps/desktop/src/lib/ui/DETAILS.md",
		"apps/desktop/test/e2e-playwright/CLAUDE.md",
		"scripts/check/CLAUDE.md",
	} {
		if matchesAny(doc, patterns) {
			t.Errorf("`svelteInputs` still covers %s; a docs-only edit re-runs every frontend lane", doc)
		}
	}
	// The veto must not reach past the docs: the sources themselves stay in.
	for _, src := range []string{
		"apps/desktop/src/lib/ui/Button.svelte",
		"apps/desktop/test/e2e-playwright/queue.spec.ts",
		"scripts/check/main.go",
	} {
		if !matchesAny(src, patterns) {
			t.Errorf("`svelteInputs` no longer covers %s; the frontend lanes would cache-skip a real change", src)
		}
	}
}
