package main

import (
	"encoding/json"
	"fmt"
	"maps"
	"slices"
	"strings"
)

// The three build arches as release file names spell them (`x64`, never `x86_64`), keyed by
// their `latest.json` platform key.
var archByPlatform = map[string]string{
	"darwin-aarch64":   "aarch64",
	"darwin-x86_64":    "x64",
	"darwin-universal": "universal",
}

var arches = []string{"aarch64", "x64", "universal"}

type releaseAsset struct {
	Name string `json:"name"`
	Size int64  `json:"size"`
}

// Below these, an asset is a truncated upload or an empty bundle, not a release.
const (
	minBundleBytes = 20 << 20
	minSBOMBytes   = 10 << 10
)

// expectedAssets is every file a release carries (`docs/guides/releasing.md` § What a release
// publishes), each with the size below which it can't be real.
func expectedAssets(version string, withPkg bool) map[string]int64 {
	want := map[string]int64{
		"latest.json":                            1,
		"checksums.txt":                          1,
		"Cmdr_" + version + ".intoto.jsonl":      1,
		"Cmdr_" + version + "_frontend.cdx.json": minSBOMBytes,
	}
	for _, arch := range arches {
		base := "Cmdr_" + version + "_" + arch
		want[base+".dmg"] = minBundleBytes
		want[base+".app.tar.gz"] = minBundleBytes
		want[base+".app.tar.gz.sig"] = 1
	}
	for _, arch := range []string{"aarch64", "x64"} {
		want["Cmdr_"+version+"_"+arch+".rust.cdx.json"] = minSBOMBytes
	}
	if withPkg {
		want["Cmdr_"+version+"_universal.pkg"] = minBundleBytes
	}
	return want
}

// checkAssets lists every expected asset that's missing or too small to be real.
func checkAssets(assets []releaseAsset, version string, withPkg bool) error {
	have := map[string]int64{}
	for _, a := range assets {
		have[a.Name] = a.Size
	}
	var problems []string
	for name, min := range expectedAssets(version, withPkg) {
		size, ok := have[name]
		switch {
		case !ok:
			problems = append(problems, name+" is missing")
		case size < min:
			problems = append(problems, fmt.Sprintf("%s is only %d bytes", name, size))
		}
	}
	if len(problems) == 0 {
		return nil
	}
	slices.Sort(problems)
	return fmt.Errorf("the release's assets aren't complete:\n  - %s", strings.Join(problems, "\n  - "))
}

// checkManifest reads `latest.json` and returns its update archive URLs, requiring the version,
// all three platforms, and each URL naming this release's archive for its arch, on the EU mirror
// or (releases from before it) on GitHub. The release pipeline itself refuses a new manifest
// off the mirror (`scripts/release-mirror`); this only checks what's live answers.
func checkManifest(body []byte, version, repo string) ([]string, error) {
	var m struct {
		Version   string `json:"version"`
		Platforms map[string]struct {
			URL string `json:"url"`
		} `json:"platforms"`
	}
	if err := json.Unmarshal(body, &m); err != nil {
		return nil, fmt.Errorf("latest.json isn't JSON: %w", err)
	}
	if m.Version != version {
		return nil, fmt.Errorf("latest.json says version %q, not %q", m.Version, version)
	}
	var urls []string
	for _, platform := range slices.Sorted(maps.Keys(archByPlatform)) {
		p, ok := m.Platforms[platform]
		if !ok {
			return nil, fmt.Errorf("latest.json has no %s platform", platform)
		}
		file := fmt.Sprintf("Cmdr_%s_%s.app.tar.gz", version, archByPlatform[platform])
		mirrored := fmt.Sprintf("%sv%s/%s", mirrorHost, version, file)
		github := fmt.Sprintf("https://github.com/%s/releases/download/v%s/%s", repo, version, file)
		if p.URL != mirrored && p.URL != github {
			return nil, fmt.Errorf("latest.json's %s URL is %s, not %s", platform, p.URL, mirrored)
		}
		urls = append(urls, p.URL)
	}
	return urls, nil
}
