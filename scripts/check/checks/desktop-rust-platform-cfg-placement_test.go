package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const platformCfgFixtureSrc = "apps/desktop/src-tauri/src"

// runPlatformCfgPlacementOn writes the files (paths relative to the app's `src/`) and an
// optional allowlist into a fixture repo, and runs the check there.
func runPlatformCfgPlacementOn(t *testing.T, files map[string]string, allow map[string]platformCfgCounts, ci bool) (string, CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	for rel, body := range files {
		mustWrite(t, filepath.Join(root, platformCfgFixtureSrc, rel), body)
	}
	if allow != nil {
		keyed := map[string]platformCfgCounts{}
		for rel, c := range allow {
			keyed[platformCfgFixtureSrc+"/"+rel] = c
		}
		if err := os.MkdirAll(filepath.Join(root, filepath.Dir(platformCfgAllowlistRelPath)), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := writeJSONAllowlist(filepath.Join(root, platformCfgAllowlistRelPath), platformCfgAllowlist{Files: keyed}); err != nil {
			t.Fatalf("seed allowlist: %v", err)
		}
	}
	res, err := RunPlatformCfgPlacement(&CheckContext{RootDir: root, CI: ci})
	return root, res, err
}

func sitesOf(t *testing.T, source string, rel string) []platformCfgSite {
	t.Helper()
	return platformCfgSitesIn(platformCfgFixtureSrc+"/"+rel, source)
}

func TestPlatformCfg_CountsNegatedAndUnseamedSitesSeparately(t *testing.T) {
	sites := sitesOf(t, `
#[cfg(not(target_os = "macos"))]
fn a() {}
#[cfg(target_os = "linux")]
fn b() {}
#[cfg(not(unix))]
fn c() {}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn d() {}
fn e() -> bool { cfg!(windows) }
`, "feature.rs")
	got := countPlatformCfgsPerFile(sites)[platformCfgFixtureSrc+"/feature.rs"]
	want := platformCfgCounts{Negated: 3, OutsideSeam: 5}
	if got != want {
		t.Fatalf("got %+v, want %+v (sites: %+v)", got, want, sites)
	}
}

func TestPlatformCfg_SeamFilesCountOnlyNegations(t *testing.T) {
	source := `
#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;
#[cfg(unix)]
mod unix;
#[cfg(not(target_os = "macos"))]
fn other() {}
`
	got := countPlatformCfgsPerFile(sitesOf(t, source, "icons/platform/mod.rs"))[platformCfgFixtureSrc+"/icons/platform/mod.rs"]
	if want := (platformCfgCounts{Negated: 1}); got != want {
		t.Fatalf("got %+v, want %+v", got, want)
	}
	// A `platform` FILE isn't a seam; only a directory is.
	if n := len(sitesOf(t, source, "icons/platform.rs")); n != 5 {
		t.Fatalf("expected all five cfgs in platform.rs counted, got %d", n)
	}
}

func TestPlatformCfg_IgnoresNonOSCfgsAndTheBrowserGate(t *testing.T) {
	sites := sitesOf(t, `
#[cfg(feature = "mimalloc")]
fn a() {}
#[cfg(not(target_family = "wasm"))]
fn b() {}
#[cfg(not(cmdr_host))]
compile_error!("nope");
#[cfg(debug_assertions)]
fn c() {}
fn my_cfg(x: u8) {}
fn d() { my_cfg(1); }
`, "plain.rs")
	if len(sites) != 0 {
		t.Fatalf("expected no sites, got %+v", sites)
	}
}

func TestPlatformCfg_SkipsTestCode(t *testing.T) {
	sites := sitesOf(t, `
fn prod() {}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn off_macos() {}
}

#[cfg(all(test, target_os = "macos"))]
mod mac_tests {
    #[cfg(not(target_os = "linux"))]
    fn helper() {}
}
`, "with_tests.rs")
	if len(sites) != 0 {
		t.Fatalf("expected test code to be skipped, got %+v", sites)
	}
	// `any(test, …)` holds in a production build too, so it counts.
	if n := len(sitesOf(t, "#[cfg(any(test, target_os = \"macos\"))]\nfn a() {}\n", "any_test.rs")); n != 1 {
		t.Fatalf("expected any(test, …) counted, got %d", n)
	}
}

func TestPlatformCfg_ReadsMultiLinePredicatesAndIgnoresProse(t *testing.T) {
	sites := sitesOf(t, `
// #[cfg(not(target_os = "macos"))] in a comment
/// Doc prose: cfg(target_os = "linux") is how you'd say it.
const NOTE: &str = "cfg(target_os = \"windows\")";
#[cfg(all(
    not(target_os = "macos"),
    feature = "playwright-e2e",
))]
fn e2e() {}
`, "multi.rs")
	if len(sites) != 1 {
		t.Fatalf("expected the one real multi-line cfg, got %+v", sites)
	}
	if !sites[0].negated || sites[0].line != 5 {
		t.Fatalf("expected a negated site on line 5, got %+v", sites[0])
	}
	if !strings.Contains(sites[0].predicate, `not(target_os = "macos")`) {
		t.Errorf("expected the predicate read from the original source, got %q", sites[0].predicate)
	}
}

func TestPlatformCfg_SkipsDedicatedTestFiles(t *testing.T) {
	_, _, err := runPlatformCfgPlacementOn(t, map[string]string{
		"thing/tests.rs":   "#[cfg(not(target_os = \"macos\"))]\nfn a() {}\n",
		"thing_tests/x.rs": "#[cfg(target_os = \"linux\")]\nfn b() {}\n",
	}, nil, true)
	if err != nil {
		t.Fatalf("expected test files skipped, got: %v", err)
	}
}

func TestPlatformCfg_FailsOnANewSiteAndNamesIt(t *testing.T) {
	_, _, err := runPlatformCfgPlacementOn(t, map[string]string{
		"fresh.rs": "fn a() {}\n#[cfg(not(target_os = \"macos\"))]\nfn b() {}\n",
	}, nil, true)
	if err == nil {
		t.Fatal("expected a failure for an unallowlisted negated cfg")
	}
	msg := err.Error()
	for _, want := range []string{"fresh.rs:2", "negated, outside a seam", "platform-seams.md"} {
		if !strings.Contains(msg, want) {
			t.Errorf("expected %q in the message, got: %s", want, msg)
		}
	}
}

func TestPlatformCfg_AllowlistedSitesPassAndOnlyTheExceededCountFails(t *testing.T) {
	files := map[string]string{
		"known.rs": "#[cfg(not(target_os = \"macos\"))]\nfn a() {}\n#[cfg(target_os = \"linux\")]\nfn b() {}\n",
	}
	if _, res, err := runPlatformCfgPlacementOn(t, files, map[string]platformCfgCounts{
		"known.rs": {Negated: 1, OutsideSeam: 2},
	}, true); err != nil || res.Code != ResultSuccess {
		t.Fatalf("expected success at the allowlisted counts, got %v / %+v", err, res)
	}

	// Within the negated budget but over the outside-seam one: only the second count trips,
	// and the listing shows the sites that count against it.
	_, _, err := runPlatformCfgPlacementOn(t, files, map[string]platformCfgCounts{
		"known.rs": {Negated: 1, OutsideSeam: 1},
	}, true)
	if err == nil {
		t.Fatal("expected the outside-seam count to fail")
	}
	if !strings.Contains(err.Error(), "known.rs:3 [outside a seam]") {
		t.Errorf("expected the unseamed site listed, got: %s", err)
	}
}

func TestPlatformCfg_ShrinkWrapsLocallyAndOnlyReportsInCI(t *testing.T) {
	files := map[string]string{
		"shrunk.rs": "#[cfg(target_os = \"linux\")]\nfn a() {}\n",
	}
	allow := map[string]platformCfgCounts{
		"shrunk.rs": {Negated: 2, OutsideSeam: 4},
		"gone.rs":   {OutsideSeam: 3},
	}

	root, res, err := runPlatformCfgPlacementOn(t, files, allow, true)
	if err != nil {
		t.Fatalf("expected success, got: %v", err)
	}
	if !strings.Contains(res.Message, "a local run shrink-wraps them") {
		t.Errorf("expected CI to report the stale entries, got: %s", res.Message)
	}
	if got := loadPlatformCfgAllowlist(root).Files[platformCfgFixtureSrc+"/shrunk.rs"]; got != allow["shrunk.rs"] {
		t.Errorf("expected CI to leave the allowlist alone, got %+v", got)
	}

	root, _, err = runPlatformCfgPlacementOn(t, files, allow, false)
	if err != nil {
		t.Fatalf("expected success, got: %v", err)
	}
	list := loadPlatformCfgAllowlist(root)
	if got, want := list.Files[platformCfgFixtureSrc+"/shrunk.rs"], (platformCfgCounts{OutsideSeam: 1}); got != want {
		t.Errorf("expected shrunk.rs ratcheted to %+v, got %+v", want, got)
	}
	if _, ok := list.Files[platformCfgFixtureSrc+"/gone.rs"]; ok {
		t.Error("expected the deleted file's entry dropped")
	}
}

func TestParseCfgPredicate(t *testing.T) {
	cases := []struct {
		src                         string
		os, negated, testOnly, nilP bool
	}{
		{src: `target_os = "macos"`, os: true},
		{src: `unix`, os: true},
		{src: `target_family = "windows"`, os: true},
		{src: `target_family = "wasm"`},
		{src: `not(target_os = "macos")`, os: true, negated: true},
		{src: `all(test, not(unix))`, os: true, negated: true, testOnly: true},
		{src: `any(test, target_os = "linux")`, os: true},
		{src: `not(any(test, feature = "testing"))`},
		{src: `test`, testOnly: true},
		{src: `foo::bar`, nilP: true},
		{src: `all(unix`, nilP: true},
	}
	for _, c := range cases {
		n := parseCfgPredicate(c.src)
		if c.nilP {
			if n != nil {
				t.Errorf("%s: expected unparseable", c.src)
			}
			continue
		}
		if n == nil {
			t.Errorf("%s: failed to parse", c.src)
			continue
		}
		if n.mentionsOS() != c.os || n.hasNegatedOS() != c.negated || n.isTestOnly() != c.testOnly {
			t.Errorf("%s: got os=%v negated=%v testOnly=%v", c.src, n.mentionsOS(), n.hasNegatedOS(), n.isTestOnly())
		}
	}
}
