package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

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

func makeBundleRoot(t *testing.T) (rootDir, distDir string) {
	t.Helper()
	rootDir = t.TempDir()
	distDir = filepath.Join(rootDir, "apps", "website", "dist")
	if err := os.MkdirAll(filepath.Join(rootDir, "scripts", "check", "checks"), 0o755); err != nil {
		t.Fatal(err)
	}
	return rootDir, distDir
}

// websiteBundleBaselineTestPath is where the website lane's baseline lives under
// a fixture root, spelled once so the tests and the spec can't drift.
func websiteBundleBaselineTestPath(rootDir string) string {
	return filepath.Join(rootDir, filepath.FromSlash(websiteBundleBaselineRel))
}

func writeTestBundleBaseline(t *testing.T, rootDir string, baseline bundleBaseline) {
	t.Helper()
	data, err := json.Marshal(baseline)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(websiteBundleBaselineTestPath(rootDir), data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func readBundleBaseline(t *testing.T, rootDir string) bundleBaseline {
	t.Helper()
	data, err := os.ReadFile(websiteBundleBaselineTestPath(rootDir))
	if err != nil {
		t.Fatal(err)
	}
	var baseline bundleBaseline
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
	if fileExists(websiteBundleBaselineTestPath(rootDir)) {
		t.Errorf("CI run must not write the baseline")
	}
}

func TestRunWebsiteBundleSizeWithinBudget(t *testing.T) {
	rootDir, distDir := makeBundleRoot(t)
	writeDistFile(t, distDir, "index.html", 1050)
	writeTestBundleBaseline(t, rootDir, bundleBaseline{TotalBytes: 1000})

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
	writeTestBundleBaseline(t, rootDir, bundleBaseline{
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
	writeTestBundleBaseline(t, rootDir, bundleBaseline{TotalBytes: 1000})

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
	writeTestBundleBaseline(t, rootDir, bundleBaseline{TotalBytes: 1000})

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
