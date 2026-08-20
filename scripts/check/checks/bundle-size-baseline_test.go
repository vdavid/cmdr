package checks

import "testing"

// Behavior shared by every bundle-size lane: how a built asset is given a stable
// identity across rebuilds, how a directory is walked and totalled, and how the
// largest assets are ranked. The per-lane run tests live beside their lane.

func TestNormalizeAssetName(t *testing.T) {
	cases := map[string]string{
		"_astro/About.DvK3R9p1.css":  "_astro/About.*.css",
		"_astro/hoisted.DargAyOQ.js": "_astro/hoisted.*.js",
		"index.html":                 "index.html",
		"blog/post-1/index.html":     "blog/post-1/index.html",
		"favicon.16.png":             "favicon.16.png", // too short to be a content hash
		"fonts/inter-latin.woff2":    "fonts/inter-latin.woff2",
	}
	for in, want := range cases {
		if got := normalizeAssetName(in); got != want {
			t.Errorf("normalizeAssetName(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestScanBundleDir(t *testing.T) {
	distDir := t.TempDir()
	writeDistFile(t, distDir, "index.html", 1000)
	writeDistFile(t, distDir, "_astro/app.AAAAAAAA.js", 5000)
	// Same logical asset under a different content hash: merges into one key.
	writeDistFile(t, distDir, "_astro/app.BBBBBBBB.js", 3000)
	writeDistFile(t, distDir, "blog/index.html", 2000)

	scan, err := scanBundleDir(distDir, nil)
	if err != nil {
		t.Fatal(err)
	}
	if scan.totalBytes != 11000 {
		t.Errorf("totalBytes = %d, want 11000", scan.totalBytes)
	}
	if scan.fileCount != 4 {
		t.Errorf("fileCount = %d, want 4", scan.fileCount)
	}
	if scan.assets["_astro/app.*.js"] != 8000 {
		t.Errorf("merged asset size = %d, want 8000", scan.assets["_astro/app.*.js"])
	}
	if scan.assets["index.html"] != 1000 {
		t.Errorf("index.html size = %d, want 1000", scan.assets["index.html"])
	}
}

func TestTopBundleAssets(t *testing.T) {
	assets := map[string]int64{"a": 10, "b": 30, "c": 20, "d": 5}
	top := topBundleAssets(assets, 2)
	if len(top) != 2 || top["b"] != 30 || top["c"] != 20 {
		t.Errorf("topWebsiteAssets = %v, want {b:30, c:20}", top)
	}
}
