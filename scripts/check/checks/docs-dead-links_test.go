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
