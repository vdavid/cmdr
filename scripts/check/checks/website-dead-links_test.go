package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeDistPage(t *testing.T, distDir, relPath, body string) {
	t.Helper()
	path := filepath.Join(distDir, relPath)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("<!doctype html><html><body>"+body+"</body></html>"), 0o644); err != nil {
		t.Fatal(err)
	}
}

func deadLinksIn(t *testing.T, distDir string) []string {
	t.Helper()
	scan, err := scanWebsiteDeadLinks(distDir)
	if err != nil {
		t.Fatal(err)
	}
	return scan.broken
}

func TestWebsiteDeadLinksPassesResolvableLinks(t *testing.T) {
	distDir := t.TempDir()
	writeDistPage(t, distDir, "index.html", `
		<a href="/features/#s3">S3</a>
		<a href="/features#s3">S3, no slash</a>
		<a href="/blog/post/">post</a>
		<a href="blog/post">relative</a>
		<a href="#top" id="top">self</a>
		<a href="#">script button</a>
		<a href="/rss.xml">feed</a>
		<img src="/blog/post/shot.webp" alt="">
		<a href="https://example.com/nope">external</a>
		<a href="mailto:hi@example.com">mail</a>
		<a href="/features#:~:text=S3">text fragment</a>`)
	writeDistPage(t, distDir, "features/index.html", `<section id="s3"></section>`)
	writeDistPage(t, distDir, "blog/post/index.html", `<a href="../../features/#s3">up</a>`)
	writeDistFile(t, distDir, "rss.xml", 10)
	writeDistFile(t, distDir, "blog/post/shot.webp", 10)

	if broken := deadLinksIn(t, distDir); len(broken) != 0 {
		t.Fatalf("expected no dead links, got:\n%s", strings.Join(broken, "\n"))
	}
}

func TestWebsiteDeadLinksFlagsEmptyMissingAndBadAnchor(t *testing.T) {
	distDir := t.TempDir()
	// The empty href is the regression anchor: markdown `[text](<>)` shipped as `href=""` on a blog post.
	writeDistPage(t, distDir, "blog/post/index.html", `
		<a href="">empty</a>
		<a href="/no-such-page/">missing page</a>
		<a href="/features#nope">missing anchor</a>
		<img src="/missing.webp" alt="">`)
	writeDistPage(t, distDir, "features/index.html", `<section id="s3"></section>`)

	broken := deadLinksIn(t, distDir)
	joined := strings.Join(broken, "\n")
	for _, want := range []string{`href=""`, "/no-such-page/", "/features#nope", "/missing.webp"} {
		if !strings.Contains(joined, want) {
			t.Errorf("expected a finding for %s, got:\n%s", want, joined)
		}
	}
	if len(broken) != 4 {
		t.Errorf("expected 4 findings, got %d:\n%s", len(broken), joined)
	}
}
