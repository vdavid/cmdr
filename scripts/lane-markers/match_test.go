package main

import (
	"reflect"
	"strings"
	"testing"
)

// goldenPaths and goldenGlobs were run through picomatch 2.3.2 (dorny/paths-filter v4.0.3 depends
// on `picomatch ^2.3.1`) with dorny's options, `picomatch(p, {dot: true}, true)`, on 2026-11-05.
// `want` is exactly the paths that matcher returned true for, so these pin our matcher to
// picomatch's verdicts, negation included.
var goldenPaths = []string{
	"apps/desktop/src-tauri/src/main.rs", "apps/desktop/src-tauri", "apps/desktop/src-tauri/CLAUDE.md",
	"apps/desktop/src-tauri-x/a.rs", "CLAUDE.md", "docs/CLAUDE.md", "docs/XCLAUDE.md", ".mise.toml",
	"x/.mise.toml", "crates/a/.hidden/b.rs", "crates", "apps/website/src/lib/trust-mdm.ts",
	"apps/website/src/lib/trust-mdm.tsx", "a/b.ts", "a/.b.ts", "a/b/c.ts", "a/c.ts", "a/x/y/c.ts",
	"a/bb.ts", "a/x.ts", "scripts/build-listing-wasm.sh", ".github/workflows/ci.yml", "a/b",
}

var goldenGlobs = []struct {
	pattern string
	want    []string
}{
	{"apps/desktop/src-tauri/**", []string{"apps/desktop/src-tauri/src/main.rs", "apps/desktop/src-tauri", "apps/desktop/src-tauri/CLAUDE.md"}},
	{"**/CLAUDE.md", []string{"apps/desktop/src-tauri/CLAUDE.md", "CLAUDE.md", "docs/CLAUDE.md"}},
	{".mise.toml", []string{".mise.toml"}},
	{"crates/**", []string{"crates/a/.hidden/b.rs", "crates"}},
	{"apps/website/src/lib/trust-mdm.ts", []string{"apps/website/src/lib/trust-mdm.ts"}},
	{"a/*.ts", []string{"a/b.ts", "a/.b.ts", "a/c.ts", "a/bb.ts", "a/x.ts"}},
	{"a/?.ts", []string{"a/b.ts", "a/c.ts", "a/x.ts"}},
	{"scripts/build-listing-wasm/**", nil},
	{"**", goldenPaths},
	{"a/**/c.ts", []string{"a/b/c.ts", "a/c.ts", "a/x/y/c.ts"}},
	// picomatch inverts a negated matcher: everything but the `**/CLAUDE.md` hits above.
	{"!**/CLAUDE.md", []string{
		"apps/desktop/src-tauri/src/main.rs", "apps/desktop/src-tauri", "apps/desktop/src-tauri-x/a.rs",
		"docs/XCLAUDE.md", ".mise.toml", "x/.mise.toml", "crates/a/.hidden/b.rs", "crates",
		"apps/website/src/lib/trust-mdm.ts", "apps/website/src/lib/trust-mdm.tsx", "a/b.ts", "a/.b.ts",
		"a/b/c.ts", "a/c.ts", "a/x/y/c.ts", "a/bb.ts", "a/x.ts", "scripts/build-listing-wasm.sh",
		".github/workflows/ci.yml", "a/b",
	}},
}

func TestGlobMatchesPicomatch(t *testing.T) {
	for _, tc := range goldenGlobs {
		g, err := compileGlob(tc.pattern)
		if err != nil {
			t.Fatalf("compile %q: %v", tc.pattern, err)
		}
		var got []string
		for _, p := range goldenPaths {
			if g.matches(p) {
				got = append(got, p)
			}
		}
		if !reflect.DeepEqual(got, tc.want) {
			t.Errorf("%q:\n got  %q\n want %q", tc.pattern, got, tc.want)
		}
	}
}

// goldenClassPaths and goldenClassGlobs pin the character classes the `scorecard` filter uses, run
// through picomatch 2.3.2 with dorny's options (`picomatch(p, {dot: true}, true)`) on 2026-11-05,
// the same way as `goldenGlobs`.
var goldenClassPaths = []string{
	"Dockerfile", "apps/website/Dockerfile", "x/dockerfile.base", "x/DOCKERFILE", "x/Dockerfile.e2e", "x/docker-compose.yml",
	"LICENSE", "license.md", "LICENCE", "x/LICENSE", "LICENSES/a.txt", "COPYING", "SECURITY.md", "docs/security.md",
	"docs/x/SECURITY.md", "security", ".mise.toml", "a/b.ts", "Ab", "ab", "aB", "xb",
}

var goldenClassGlobs = []struct {
	pattern string
	want    []string
}{
	{"**/*[Dd][Oo][Cc][Kk][Ee][Rr][Ff][Ii][Ll][Ee]*", []string{"Dockerfile", "apps/website/Dockerfile", "x/dockerfile.base", "x/DOCKERFILE", "x/Dockerfile.e2e"}},
	{"*[Ll][Ii][Cc][Ee][Nn][CcSs][Ee]*", []string{"LICENSE", "license.md", "LICENCE"}},
	// picomatch doesn't let a trailing `/**` match the bare name when the segment before it is a
	// glob (it does for a literal one: `crates/**` matches `crates` above).
	{"*[Ll][Ii][Cc][Ee][Nn][CcSs][Ee]*/**", []string{"LICENSES/a.txt"}},
	{"[Ss][Ee][Cc][Uu][Rr][Ii][Tt][Yy].*", []string{"SECURITY.md"}},
	{"docs/[Ss][Ee][Cc][Uu][Rr][Ii][Tt][Yy].*", []string{"docs/security.md"}},
	{"[Aa]b", []string{"Ab", "ab"}},
	{"!**/*[Dd][Oo][Cc][Kk][Ee][Rr][Ff][Ii][Ll][Ee]*", []string{
		"x/docker-compose.yml", "LICENSE", "license.md", "LICENCE", "x/LICENSE", "LICENSES/a.txt", "COPYING", "SECURITY.md",
		"docs/security.md", "docs/x/SECURITY.md", "security", ".mise.toml", "a/b.ts", "Ab", "ab", "aB", "xb",
	}},
}

func TestClassGlobsMatchPicomatch(t *testing.T) {
	for _, tc := range goldenClassGlobs {
		g, err := compileGlob(tc.pattern)
		if err != nil {
			t.Fatalf("compile %q: %v", tc.pattern, err)
		}
		var got []string
		for _, p := range goldenClassPaths {
			if g.matches(p) {
				got = append(got, p)
			}
		}
		if !reflect.DeepEqual(got, tc.want) {
			t.Errorf("%q:\n got  %q\n want %q", tc.pattern, got, tc.want)
		}
	}
}

func TestCompileGlobRefusesUnsupportedSyntax(t *testing.T) {
	for _, p := range []string{
		"src/{a,b}/**", "src/[a-b].rs", "src/[!a].rs", "src/[^a].rs", "src/[].rs", "src/[ab.rs", "src/a].rs",
		"src/[[:alpha:]].rs", "src/@(a|b).rs", "!(x)/**", `src/\*.rs`, "a!b", "!!a",
		"/abs", "./rel", "dir/", "a//b", "a/b**", "", "!",
	} {
		if _, err := compileGlob(p); err == nil {
			t.Errorf("compileGlob(%q) accepted a pattern dorny might match differently", p)
		}
	}
}

func mustFilterT(t *testing.T, patterns ...string) filter {
	t.Helper()
	f := filter{name: "f"}
	for _, p := range patterns {
		g, err := compileGlob(p)
		if err != nil {
			t.Fatal(err)
		}
		f.patterns = append(f.patterns, g)
	}
	return f
}

// The real `rust` filter's shape: includes, then the agent-doc excludes, under some-with-excludes.
func TestSomeWithExcludesLikeDorny(t *testing.T) {
	rust := mustFilterT(t,
		"apps/desktop/src-tauri/**", "apps/desktop/src/lib/ipc/bindings.ts", "crates/**", "Cargo.lock",
		".github/workflows/ci.yml", "!**/CLAUDE.md", "!**/DETAILS.md")
	cases := map[string]bool{
		"apps/desktop/src-tauri/src/main.rs":            true,
		"apps/desktop/src-tauri/src/agent/prompts/x.md": true, // other .md files stay in
		"apps/desktop/src-tauri/CLAUDE.md":              false,
		"crates/cmdr-fs/src/DETAILS.md":                 false,
		"crates/cmdr-fs/src/lib.rs":                     true,
		"apps/desktop/src/lib/ipc/bindings.ts":          true,
		"apps/desktop/src/lib/ipc/other.ts":             false,
		"Cargo.lock":                                    true,
		"CLAUDE.md":                                     false,
		"docs/tooling/ci.md":                            false,
		".github/workflows/ci.yml":                      true,
	}
	for p, want := range cases {
		if got := rust.matches(p, quantSomeWithExcludes); got != want {
			t.Errorf("rust matches %q = %v, want %v", p, got, want)
		}
	}
	// An exclusion is final, and excludes alone include nothing.
	if mustFilterT(t, "!**/CLAUDE.md").matches("src/a.rs", quantSomeWithExcludes) {
		t.Error("a filter of excludes only matched a file")
	}
}

// Under dorny's default `some`, a negated pattern matches every file it doesn't name, which is
// why ci.yml sets some-with-excludes. We mirror that, so a quantifier edit changes our hashes the
// way it changes dorny's verdicts.
func TestSomeAndEveryLikeDorny(t *testing.T) {
	f := mustFilterT(t, "crates/**", "!**/CLAUDE.md")
	if !f.matches("docs/guide.md", quantSome) {
		t.Error("some: a negated pattern should match a file it doesn't name")
	}
	if !f.matches("crates/CLAUDE.md", quantSome) {
		t.Error("some: crates/** should still match crates/CLAUDE.md")
	}
	if f.matches("crates/CLAUDE.md", quantEvery) || !f.matches("crates/a.rs", quantEvery) || f.matches("docs/a.md", quantEvery) {
		t.Error("every: wants both patterns satisfied")
	}
}

func TestParseFiltersFlattensAnchorsAndRefusesStatusRules(t *testing.T) {
	fs, err := parseFilters("shared: &shared\n  - 'a/**'\nlane:\n  - *shared\n  - 'b.txt'\n  - '!**/CLAUDE.md'\n")
	if err != nil {
		t.Fatal(err)
	}
	var got []string
	for _, g := range fs["lane"].patterns {
		got = append(got, g.raw)
	}
	if strings.Join(got, ",") != "a/**,b.txt,!**/CLAUDE.md" {
		t.Errorf("lane patterns = %q", got)
	}
	if _, err := parseFilters("lane:\n  - added|modified: 'a/**'\n"); err == nil {
		t.Error("a status-scoped rule was accepted")
	}
	if _, err := parseFilters(".github/filters.yml"); err == nil {
		t.Error("a filters file path was accepted")
	}
}
