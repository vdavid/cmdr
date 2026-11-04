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
		<img src="/blog/post/shot.webp?v=3f9a1c2b7d" alt="">
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

// The browser demo is the desktop frontend's build, copied in by `src/plugins/demo.ts`: Vite
// content-hashes its assets itself (no `?v=`), and the desktop's checks own its markup. A site page
// linking TO it still resolves against its files.
func TestWebsiteDeadLinksSkipsTheBrowserDemo(t *testing.T) {
	distDir := t.TempDir()
	writeDistPage(t, distDir, "index.html", `<a href="/demo/">demo</a>`)
	writeDistPage(t, distDir, "demo/index.html", `<link href="/demo/_app/immutable/assets/0.7l93QJKm.css"><a href="/gone/">x</a>`)
	writeDistFile(t, distDir, "demo/_app/immutable/assets/0.7l93QJKm.css", 10)

	if broken := deadLinksIn(t, distDir); len(broken) != 0 {
		t.Fatalf("expected the demo to be skipped, got:\n%s", strings.Join(broken, "\n"))
	}
}

// The demo is published only by an opt-in build, so a site built without it can't judge links
// into it (the home page's hero embeds it). A build WITH the demo still checks them.
func TestWebsiteDeadLinksSkipsDemoLinksWhenTheDemoIsNotBuilt(t *testing.T) {
	distDir := t.TempDir()
	writeDistPage(t, distDir, "index.html", `<a href="/demo/">demo</a><a href="/demo">demo</a><a href="/demonstration/">x</a>`)

	broken := deadLinksIn(t, distDir)
	if len(broken) != 1 || !strings.Contains(broken[0], `"/demonstration/"`) {
		t.Fatalf("expected only /demonstration/ flagged, got:\n%s", strings.Join(broken, "\n"))
	}

	writeDistPage(t, distDir, "demo/index.html", `<p>app</p>`)
	writeDistPage(t, distDir, "index.html", `<a href="/demo/">demo</a><a href="/demo/gone.css">x</a>`)
	broken = deadLinksIn(t, distDir)
	if len(broken) != 1 || !strings.Contains(broken[0], `"/demo/gone.css"`) {
		t.Fatalf("expected /demo/gone.css flagged once the demo is built, got:\n%s", strings.Join(broken, "\n"))
	}
}

func TestWebsiteDeadLinksFlagsUnversionedPublicAssets(t *testing.T) {
	distDir := t.TempDir()
	// `asset-versions.ts` appends `?v=` to every `public/` asset; one it missed would be cached
	// under a name that never changes, which is how an updated hero image stayed stale.
	writeDistPage(t, distDir, "index.html", `
		<img src="/hero/frame.webp?v=3f9a1c2b7d" alt="">
		<img src="/hero/stale.webp" alt="">
		<script src="/_astro/app.B1x2y3.js"></script>
		<a href="/features/">features</a>`)
	writeDistPage(t, distDir, "features/index.html", ``)
	writeDistFile(t, distDir, "hero/frame.webp", 10)
	writeDistFile(t, distDir, "hero/stale.webp", 10)
	writeDistFile(t, distDir, "_astro/app.B1x2y3.js", 10)

	broken := deadLinksIn(t, distDir)
	if len(broken) != 1 || !strings.Contains(broken[0], "/hero/stale.webp") || !strings.Contains(broken[0], "unversioned") {
		t.Fatalf("expected exactly the unversioned hero image, got:\n%s", strings.Join(broken, "\n"))
	}
}
