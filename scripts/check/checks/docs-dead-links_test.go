package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeDeadLinkFile(t *testing.T, dir, relPath, content string) {
	t.Helper()
	full := filepath.Join(dir, relPath)
	if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(full, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestRunDocsDeadLinks_AllResolve(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Root. See [agents](AGENTS.md) and [sub](lib/foo/CLAUDE.md).")
	writeDeadLinkFile(t, tmp, "AGENTS.md", "Agents doc.")
	writeDeadLinkFile(t, tmp, "lib/foo/CLAUDE.md", "Foo. Depth in [DETAILS.md](DETAILS.md), parent at [up](../../AGENTS.md).")
	writeDeadLinkFile(t, tmp, "lib/foo/DETAILS.md", "Depth.")

	result, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("all links resolve, expected success: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunDocsDeadLinks_MissingTarget(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Root links [gone](docs/missing.md) and [also-gone](nope/CLAUDE.md).")

	_, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for links to non-existent targets")
	}
	for _, want := range []string{"docs/missing.md", "nope/CLAUDE.md", "dead doc"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected %q in the error, got: %v", want, err)
		}
	}
}

func TestRunDocsDeadLinks_SkipsExternalAndAnchors(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", strings.Join([]string{
		"[web](https://example.com/missing)",
		"[mail](mailto:nobody@example.com)",
		"[anchor](#a-section)",
		"[proto](//cdn.example.com/x)",
	}, " "))

	result, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("external and anchor links must be skipped, got: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got: %s", result.Message)
	}
}

func TestRunDocsDeadLinks_SkipsLinksInCode(t *testing.T) {
	tmp := t.TempDir()
	// A Markdown-link example inside a fenced block and inside inline code must not
	// be treated as a live link, even though its target doesn't exist.
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Example: use `[text](some/missing/path.md)` like this.\n\n```\n[demo](another/missing.md)\n```\n")

	result, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("links inside code must be skipped, got: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got: %s", result.Message)
	}
}

func TestRunDocsDeadLinks_DirectoryTargetResolves(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "docs/specs/index.md", "Deferred work lives under [later](later/).")
	writeDeadLinkFile(t, tmp, "docs/specs/later/thing.md", "A later spec.")

	result, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("a link to an existing directory must resolve, got: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got: %s", result.Message)
	}
}

func TestRunDocsDeadLinks_BlogSlugLinkResolves(t *testing.T) {
	tmp := t.TempDir()
	// A blog post linking a sibling post by its rendered /blog/<slug> URL must resolve
	// to the sibling's content directory, not be flagged as a missing /blog dir.
	writeDeadLinkFile(t, tmp, "apps/website/src/content/blog/post-a/index.md", "See [b](/blog/post-b) and [b-slash](/blog/post-b/).")
	writeDeadLinkFile(t, tmp, "apps/website/src/content/blog/post-b/index.md", "Post B.")

	result, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("a /blog/<slug> cross-post link must resolve, got: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got: %s", result.Message)
	}
}

func TestRunDocsDeadLinks_BlogSlugLinkMissingFails(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "apps/website/src/content/blog/post-a/index.md", "Dangling [gone](/blog/post-gone).")

	_, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for a /blog/<slug> link with no matching post")
	}
	if !strings.Contains(err.Error(), "/blog/post-gone") {
		t.Errorf("expected the dead /blog link in the error, got: %v", err)
	}
}

func TestRunDocsDeadLinks_PageRouteLinkResolves(t *testing.T) {
	tmp := t.TempDir()
	// A blog post linking a site-absolute Astro page route (/pricing, /roadmap, with
	// or without an anchor) must resolve to the page source, not be flagged as a
	// missing repo-root path. /features as a directory-style route resolves to
	// pages/features/index.astro.
	writeDeadLinkFile(t, tmp, "apps/website/src/content/blog/post-a/index.md",
		"See [price](/pricing), [plans](/roadmap#very-soon), and [feat](/features).")
	writeDeadLinkFile(t, tmp, "apps/website/src/pages/pricing.astro", "Pricing page.")
	writeDeadLinkFile(t, tmp, "apps/website/src/pages/roadmap.astro", "Roadmap page.")
	writeDeadLinkFile(t, tmp, "apps/website/src/pages/features/index.astro", "Features page.")

	result, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("a site-absolute page-route link must resolve, got: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got: %s", result.Message)
	}
}

func TestRunDocsDeadLinks_PageRouteLinkMissingFails(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "apps/website/src/content/blog/post-a/index.md", "Dangling [gone](/no-such-page).")

	_, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for a site-absolute link with no matching page")
	}
	if !strings.Contains(err.Error(), "/no-such-page") {
		t.Errorf("expected the dead page-route link in the error, got: %v", err)
	}
}

func TestPageRouteCandidates(t *testing.T) {
	cases := []struct {
		srcDoc, target string
		want           []string
	}{
		{"apps/website/src/content/blog/a/index.md", "/pricing",
			[]string{"apps/website/src/pages/pricing.astro", "apps/website/src/pages/pricing/index.astro"}},
		{"apps/website/src/content/blog/a/index.md", "/blog/b",
			[]string{"apps/website/src/pages/blog/b.astro", "apps/website/src/pages/blog/b/index.astro"}},
		{"apps/website/src/content/blog/a/index.md", "/", nil},           // site root, no page file
		{"apps/website/src/content/blog/a/index.md", "relative.md", nil}, // not site-absolute
		{"docs/guides/thing.md", "/pricing", nil},                        // source not under a content tree
	}
	for _, c := range cases {
		got := pageRouteCandidates(c.srcDoc, c.target)
		if len(got) != len(c.want) {
			t.Errorf("pageRouteCandidates(%q, %q) = %v, want %v", c.srcDoc, c.target, got, c.want)
			continue
		}
		for i := range got {
			if got[i] != c.want[i] {
				t.Errorf("pageRouteCandidates(%q, %q)[%d] = %q, want %q", c.srcDoc, c.target, i, got[i], c.want[i])
			}
		}
	}
}

func TestBlogLinkCandidate(t *testing.T) {
	cases := []struct {
		srcDoc, target, want string
	}{
		{"apps/website/src/content/blog/a/index.md", "/blog/b", "apps/website/src/content/blog/b"},
		{"apps/website/src/content/blog/a/index.md", "/blog/b/", "apps/website/src/content/blog/b"},
		{"apps/website/src/content/blog/a/index.md", "/blog/b/c", ""}, // nested, not a post slug
		{"apps/website/src/content/blog/a/index.md", "/other/b", ""},  // not a /blog route
		{"docs/guides/writing-blog-posts.md", "/blog/b", ""},          // source isn't a blog post
	}
	for _, c := range cases {
		if got := blogLinkCandidate(c.srcDoc, c.target); got != c.want {
			t.Errorf("blogLinkCandidate(%q, %q) = %q, want %q", c.srcDoc, c.target, got, c.want)
		}
	}
}

func TestLocalLinkTarget(t *testing.T) {
	cases := map[string]string{
		"DETAILS.md":            "DETAILS.md",
		"./foo/bar.md":          "./foo/bar.md",
		"foo.md#section":        "foo.md",
		"foo.md?x=1":            "foo.md",
		"<path with spaces.md>": "path with spaces.md",
		"https://example.com":   "",
		"mailto:a@b.com":        "",
		"#anchor":               "",
		"//cdn/x":               "",
		"":                      "",
	}
	for in, want := range cases {
		if got := localLinkTarget(in); got != want {
			t.Errorf("localLinkTarget(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestIsRepoPathToken(t *testing.T) {
	tests := []struct {
		tok  string
		want bool
	}{
		{"docs/architecture.md", true},
		{"apps/desktop/src-tauri/src/indexing/DETAILS.md", true},
		{".claude/rules/docs.md", true},
		{"DETAILS.md", false},                           // single segment: too close to prose like `C.md`
		{"C.md", false},                                 // an abbreviation, not a path
		{"and/or", false},                               // prose pair, no extension
		{"read/write", false},                           // prose pair
		{"scripts/check", false},                        // no extension: not verified
		{"src/lib/foo/", false},                         // directory: not verified
		{"apps/desktop/src-tauri/src/lib.rs", false},    // only docs are verified
		{"~/projects-git/vdavid/smb2/AGENTS.md", false}, // another repo on this machine
		{"/tmp/report.md", false},                       // absolute: outside the repo
		{"file_system/.../backends/DETAILS.md", false},  // elided path
		{"node_modules/pkg/README.md", false},           // untracked dependency
		{"apps/x/node_modules/p/README.md", false},
		{"pnpm check --fast", false}, // command, not a path
		{"", false},
	}
	for _, tt := range tests {
		if got := isRepoPathToken(tt.tok); got != tt.want {
			t.Errorf("isRepoPathToken(%q) = %v, want %v", tt.tok, got, tt.want)
		}
	}
}

func TestRunDocsDeadLinks_BareBacktickPathMissing(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Root. Depth in `docs/gone/DETAILS.md`.")

	_, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for a backtick path with no target")
	}
	for _, want := range []string{"docs/gone/DETAILS.md", "backtick path"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected %q in the error, got: %v", want, err)
		}
	}
}

func TestRunDocsDeadLinks_BareBacktickPathResolves(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Root. Depth in `docs/here/DETAILS.md`, sibling in `DETAILS.md`.")
	writeDeadLinkFile(t, tmp, "docs/here/DETAILS.md", "Depth.")
	writeDeadLinkFile(t, tmp, "DETAILS.md", "Sibling.")

	if _, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("backtick paths resolve, expected success: %v", err)
	}
}

func TestRunDocsDeadLinks_BareBacktickPathInFenceIgnored(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Root.\n\n```md\nSee `docs/example/DETAILS.md`.\n```\n")

	if _, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("a path inside a fence is an example, expected success: %v", err)
	}
}

func TestRunDocsDeadLinks_BareBacktickProsePairsIgnored(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Root. Handles `read/write` and `and/or`, plus `~/elsewhere/AGENTS.md`.")

	if _, err := RunDocsDeadLinks(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("prose pairs and out-of-repo paths are not references: %v", err)
	}
}
