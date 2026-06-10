package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

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

func writeDistFile(t *testing.T, distDir, relPath string, size int) {
	t.Helper()
	path := filepath.Join(distDir, relPath)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, make([]byte, size), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestScanWebsiteDist(t *testing.T) {
	distDir := t.TempDir()
	writeDistFile(t, distDir, "index.html", 1000)
	writeDistFile(t, distDir, "_astro/app.AAAAAAAA.js", 5000)
	// Same logical asset under a different content hash: merges into one key.
	writeDistFile(t, distDir, "_astro/app.BBBBBBBB.js", 3000)
	writeDistFile(t, distDir, "blog/index.html", 2000)

	scan, err := scanWebsiteDist(distDir)
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

func TestTopWebsiteAssets(t *testing.T) {
	assets := map[string]int64{"a": 10, "b": 30, "c": 20, "d": 5}
	top := topWebsiteAssets(assets, 2)
	if len(top) != 2 || top["b"] != 30 || top["c"] != 20 {
		t.Errorf("topWebsiteAssets = %v, want {b:30, c:20}", top)
	}
}

func makeBundleRoot(t *testing.T) (rootDir, distDir string) {
	t.Helper()
	rootDir = t.TempDir()
	distDir = filepath.Join(rootDir, "apps", "website", "dist")
	if err := os.MkdirAll(filepath.Join(rootDir, "scripts", "check", "checks"), 0o755); err != nil {
		t.Fatal(err)
	}
	return rootDir, distDir
}

func writeBundleBaseline(t *testing.T, rootDir string, baseline websiteBundleBaseline) {
	t.Helper()
	data, err := json.Marshal(baseline)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(websiteBundleBaselinePath(rootDir), data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func readBundleBaseline(t *testing.T, rootDir string) websiteBundleBaseline {
	t.Helper()
	data, err := os.ReadFile(websiteBundleBaselinePath(rootDir))
	if err != nil {
		t.Fatal(err)
	}
	var baseline websiteBundleBaseline
	if err := json.Unmarshal(data, &baseline); err != nil {
		t.Fatal(err)
	}
	return baseline
}

func TestRunWebsiteBundleSizeSkipsWithoutDist(t *testing.T) {
	rootDir, _ := makeBundleRoot(t)
	result, err := RunWebsiteBundleSize(&CheckContext{RootDir: rootDir})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSkipped {
		t.Errorf("got code %v, want ResultSkipped", result.Code)
	}
}

func TestRunWebsiteBundleSizeCreatesBaselineLocally(t *testing.T) {
	rootDir, distDir := makeBundleRoot(t)
	writeDistFile(t, distDir, "index.html", 1000)

	result, err := RunWebsiteBundleSize(&CheckContext{RootDir: rootDir})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges when creating the baseline")
	}
	baseline := readBundleBaseline(t, rootDir)
	if baseline.TotalBytes != 1000 {
		t.Errorf("baseline totalBytes = %d, want 1000", baseline.TotalBytes)
	}
}

func TestRunWebsiteBundleSizeMissingBaselineCIWarns(t *testing.T) {
	rootDir, distDir := makeBundleRoot(t)
	writeDistFile(t, distDir, "index.html", 1000)

	result, err := RunWebsiteBundleSize(&CheckContext{RootDir: rootDir, CI: true})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Errorf("got code %v, want ResultWarning (no committed baseline)", result.Code)
	}
	if fileExists(websiteBundleBaselinePath(rootDir)) {
		t.Errorf("CI run must not write the baseline")
	}
}

func TestRunWebsiteBundleSizeWithinBudget(t *testing.T) {
	rootDir, distDir := makeBundleRoot(t)
	writeDistFile(t, distDir, "index.html", 1050)
	writeBundleBaseline(t, rootDir, websiteBundleBaseline{TotalBytes: 1000})

	result, err := RunWebsiteBundleSize(&CheckContext{RootDir: rootDir})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("+5%% growth should pass, got code %v: %s", result.Code, result.Message)
	}
	if result.MadeChanges {
		t.Errorf("in-band run must not rewrite the baseline")
	}
}

func TestRunWebsiteBundleSizeWarnsOnGrowth(t *testing.T) {
	rootDir, distDir := makeBundleRoot(t)
	writeDistFile(t, distDir, "index.html", 200)
	writeDistFile(t, distDir, "_astro/app.AAAAAAAA.js", 1000)
	writeBundleBaseline(t, rootDir, websiteBundleBaseline{
		TotalBytes: 1000,
		TopAssets:  map[string]int64{"_astro/app.*.js": 850},
	})

	result, err := RunWebsiteBundleSize(&CheckContext{RootDir: rootDir})
	if err != nil {
		t.Fatal(err)
	}
	if result.Code != ResultWarning {
		t.Fatalf("+20%% growth should warn, got code %v: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "_astro/app.*.js") {
		t.Errorf("warn message should list the largest assets, got:\n%s", result.Message)
	}
	if !strings.Contains(result.Message, "website-bundle-size-baseline.json") {
		t.Errorf("warn message should explain how to refresh the baseline, got:\n%s", result.Message)
	}
	// Warn-only: the baseline must not be raised automatically.
	if readBundleBaseline(t, rootDir).TotalBytes != 1000 {
		t.Errorf("growth must not rewrite the baseline")
	}
}

func TestRunWebsiteBundleSizeRatchetsDownLocally(t *testing.T) {
	rootDir, distDir := makeBundleRoot(t)
	writeDistFile(t, distDir, "index.html", 500)
	writeBundleBaseline(t, rootDir, websiteBundleBaseline{TotalBytes: 1000})

	result, err := RunWebsiteBundleSize(&CheckContext{RootDir: rootDir})
	if err != nil {
		t.Fatal(err)
	}
	if !result.MadeChanges {
		t.Errorf("expected MadeChanges on downward ratchet")
	}
	if got := readBundleBaseline(t, rootDir).TotalBytes; got != 500 {
		t.Errorf("ratcheted baseline = %d, want 500", got)
	}
}

func TestRunWebsiteBundleSizeShrinkInCIWarnsWithoutWriting(t *testing.T) {
	rootDir, distDir := makeBundleRoot(t)
	writeDistFile(t, distDir, "index.html", 500)
	writeBundleBaseline(t, rootDir, websiteBundleBaseline{TotalBytes: 1000})

	result, err := RunWebsiteBundleSize(&CheckContext{RootDir: rootDir, CI: true})
	if err != nil {
		t.Fatal(err)
	}
	if result.MadeChanges {
		t.Errorf("CI run must not rewrite the baseline")
	}
	if result.Code != ResultWarning {
		t.Errorf("CI slack should be reported as a warning, got code %v", result.Code)
	}
	if got := readBundleBaseline(t, rootDir).TotalBytes; got != 1000 {
		t.Errorf("CI run must leave the baseline at 1000, got %d", got)
	}
}
