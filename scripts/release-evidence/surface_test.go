package main

import (
	"strings"
	"testing"
)

func fullAssets(version string) []releaseAsset {
	var assets []releaseAsset
	for name, min := range expectedAssets(version, true) {
		assets = append(assets, releaseAsset{Name: name, Size: min})
	}
	return assets
}

func TestCheckAssets(t *testing.T) {
	assets := fullAssets("0.55.0")
	if len(assets) != 16 {
		t.Fatalf("v0.55.0 shipped 16 assets, expectedAssets lists %d", len(assets))
	}
	if err := checkAssets(assets, "0.55.0", true); err != nil {
		t.Error(err)
	}

	var noPkg []releaseAsset
	for _, a := range assets {
		if !strings.HasSuffix(a.Name, ".pkg") {
			noPkg = append(noPkg, a)
		}
	}
	if err := checkAssets(noPkg, "0.55.0", true); err == nil || !strings.Contains(err.Error(), "Cmdr_0.55.0_universal.pkg is missing") {
		t.Errorf("a missing pkg should fail by default: %v", err)
	}
	if err := checkAssets(noPkg, "0.55.0", false); err != nil {
		t.Errorf("-no-pkg should accept it: %v", err)
	}

	for i := range assets {
		if assets[i].Name == "Cmdr_0.55.0_x64.dmg" {
			assets[i].Size = 1024
		}
	}
	if err := checkAssets(assets, "0.55.0", true); err == nil || !strings.Contains(err.Error(), "Cmdr_0.55.0_x64.dmg is only 1024 bytes") {
		t.Errorf("a truncated DMG should fail: %v", err)
	}
}

const goodManifest = `{
  "version": "0.55.0",
  "platforms": {
    "darwin-aarch64": {"url": "https://github.com/vdavid/cmdr/releases/download/v0.55.0/Cmdr_0.55.0_aarch64.app.tar.gz"},
    "darwin-universal": {"url": "https://github.com/vdavid/cmdr/releases/download/v0.55.0/Cmdr_0.55.0_universal.app.tar.gz"},
    "darwin-x86_64": {"url": "https://github.com/vdavid/cmdr/releases/download/v0.55.0/Cmdr_0.55.0_x64.app.tar.gz"}
  }
}`

func TestCheckManifest(t *testing.T) {
	urls, err := checkManifest([]byte(goodManifest), "0.55.0", "vdavid/cmdr")
	if err != nil || len(urls) != 3 {
		t.Fatalf("got %v %v", urls, err)
	}
	bad := map[string]string{
		"stale version":   strings.Replace(goodManifest, `"version": "0.55.0"`, `"version": "0.54.0"`, 1),
		"drifted name":    strings.Replace(goodManifest, "Cmdr_0.55.0_x64.app", "Cmdr_0.55.0_x86_64.app", 1),
		"other release":   strings.Replace(goodManifest, "download/v0.55.0/Cmdr_0.55.0_aarch64", "download/v0.54.0/Cmdr_0.55.0_aarch64", 1),
		"missing arch":    strings.Replace(goodManifest, `"darwin-universal"`, `"darwin-other"`, 1),
		"not JSON at all": "<html>",
	}
	for name, body := range bad {
		if _, err := checkManifest([]byte(body), "0.55.0", "vdavid/cmdr"); err == nil {
			t.Errorf("%s: want an error", name)
		}
	}
}

const mirroredManifest = `{
  "version": "0.60.0",
  "platforms": {
    "darwin-aarch64": {"url": "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_aarch64.app.tar.gz"},
    "darwin-universal": {"url": "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_universal.app.tar.gz"},
    "darwin-x86_64": {"url": "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_x64.app.tar.gz"}
  }
}`

// Releases from the EU mirror on name `download.getcmdr.com`; older ones name GitHub, and both
// still have to verify.
func TestCheckManifestAcceptsTheEUMirror(t *testing.T) {
	urls, err := checkManifest([]byte(mirroredManifest), "0.60.0", "vdavid/cmdr")
	if err != nil || len(urls) != 3 || !strings.HasPrefix(urls[0], "https://download.getcmdr.com/v0.60.0/") {
		t.Fatalf("got %v %v", urls, err)
	}
	for name, body := range map[string]string{
		"other release on the mirror": strings.Replace(mirroredManifest, "v0.60.0/Cmdr_0.60.0_x64", "v0.59.0/Cmdr_0.60.0_x64", 1),
		"lookalike host":              strings.Replace(mirroredManifest, "download.getcmdr.com/v0.60.0/Cmdr_0.60.0_x64", "download.getcmdr.com.evil.example/v0.60.0/Cmdr_0.60.0_x64", 1),
	} {
		if _, err := checkManifest([]byte(body), "0.60.0", "vdavid/cmdr"); err == nil {
			t.Errorf("%s: want an error", name)
		}
	}
}
