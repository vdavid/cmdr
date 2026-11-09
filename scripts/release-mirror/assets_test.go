package main

import (
	"slices"
	"strings"
	"testing"
)

func fullRelease(version string, withPkg bool) []string {
	names := []string{
		"latest.json", "checksums.txt", "Cmdr_" + version + ".intoto.jsonl",
		"Cmdr_" + version + "_frontend.cdx.json", "Cmdr_" + version + "_aarch64.rust.cdx.json",
	}
	for _, arch := range []string{"aarch64", "x64", "universal"} {
		base := "Cmdr_" + version + "_" + arch
		names = append(names, base+".dmg", base+".app.tar.gz", base+".app.tar.gz.sig")
	}
	if withPkg {
		names = append(names, "Cmdr_"+version+"_universal.pkg")
	}
	return names
}

func TestMirroredAssetsTakesTheDownloadsAndUpdatesButNotTheSBOMsOrManifest(t *testing.T) {
	got, err := mirroredAssets(fullRelease("0.60.0", true), "0.60.0")
	if err != nil {
		t.Fatal(err)
	}
	want := []string{
		"Cmdr_0.60.0_aarch64.app.tar.gz", "Cmdr_0.60.0_aarch64.app.tar.gz.sig", "Cmdr_0.60.0_aarch64.dmg",
		"Cmdr_0.60.0_universal.app.tar.gz", "Cmdr_0.60.0_universal.app.tar.gz.sig", "Cmdr_0.60.0_universal.dmg",
		"Cmdr_0.60.0_universal.pkg",
		"Cmdr_0.60.0_x64.app.tar.gz", "Cmdr_0.60.0_x64.app.tar.gz.sig", "Cmdr_0.60.0_x64.dmg",
		// Last, so it never names a file the mirror doesn't hold yet.
		"checksums.txt",
	}
	if !slices.Equal(got, want) {
		t.Fatalf("mirroredAssets =\n  %v\nwant\n  %v", got, want)
	}
}

func TestMirroredAssetsWorksWithoutThePkg(t *testing.T) {
	got, err := mirroredAssets(fullRelease("0.60.0", false), "0.60.0")
	if err != nil {
		t.Fatal(err)
	}
	if slices.Contains(got, "Cmdr_0.60.0_universal.pkg") || len(got) != 10 {
		t.Fatalf("mirroredAssets without a pkg = %v", got)
	}
}

func TestMirroredAssetsRefusesAReleaseMissingAnUpdateFile(t *testing.T) {
	names := slices.DeleteFunc(fullRelease("0.60.0", true), func(n string) bool {
		return n == "Cmdr_0.60.0_x64.app.tar.gz.sig" || n == "checksums.txt"
	})
	_, err := mirroredAssets(names, "0.60.0")
	if err == nil {
		t.Fatal("a release without the x64 signature and the checksums was accepted")
	}
	for _, missing := range []string{"Cmdr_0.60.0_x64.app.tar.gz.sig", "checksums.txt"} {
		if !strings.Contains(err.Error(), missing) {
			t.Errorf("the error doesn't name %s: %v", missing, err)
		}
	}
}

func TestObjectKeyAndPublicURLShareOnePath(t *testing.T) {
	if got := objectKey("0.60.0", "Cmdr_0.60.0_x64.dmg"); got != "v0.60.0/Cmdr_0.60.0_x64.dmg" {
		t.Errorf("objectKey = %q", got)
	}
	if got := publicURL("https://download.getcmdr.com", "0.60.0", "Cmdr_0.60.0_x64.dmg"); got != "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_x64.dmg" {
		t.Errorf("publicURL = %q", got)
	}
}

func TestContentTypes(t *testing.T) {
	for name, want := range map[string]string{
		"Cmdr_0.60.0_x64.dmg":            "application/x-apple-diskimage",
		"Cmdr_0.60.0_x64.app.tar.gz":     "application/gzip",
		"Cmdr_0.60.0_x64.app.tar.gz.sig": "text/plain; charset=utf-8",
		"Cmdr_0.60.0_universal.pkg":      "application/octet-stream",
		"checksums.txt":                  "text/plain; charset=utf-8",
	} {
		if got := contentType(name); got != want {
			t.Errorf("contentType(%s) = %q, want %q", name, got, want)
		}
	}
}

func TestParseChecksums(t *testing.T) {
	body := "521ba1808b849cf2a31d7821b08cace1a30dba8d7e5a4654519e766342c0ac4d  Cmdr_0.59.0_aarch64.dmg\n" +
		"30c2f22b3fc7af917ccd624955d03d61ee9eaa51fa0f60eafebfa4ee67bd55f0  Cmdr_0.59.0_universal.pkg\n"
	got, err := parseChecksums(body)
	if err != nil {
		t.Fatal(err)
	}
	if got["Cmdr_0.59.0_aarch64.dmg"] != "521ba1808b849cf2a31d7821b08cace1a30dba8d7e5a4654519e766342c0ac4d" || len(got) != 2 {
		t.Fatalf("parseChecksums = %v", got)
	}
	if _, err := parseChecksums("not a checksum line\n"); err == nil {
		t.Error("a malformed line was accepted")
	}
}

func TestCheckHashes(t *testing.T) {
	local := map[string]string{"a.dmg": "11", "a.app.tar.gz": "22"}
	if err := checkHashes(local, map[string]string{"a.dmg": "11", "a.app.tar.gz": ""}, map[string]string{"a.dmg": "11"}); err != nil {
		t.Errorf("matching hashes: %v", err)
	}
	for name, c := range map[string]struct{ github, listed map[string]string }{
		"GitHub disagrees":                  {github: map[string]string{"a.app.tar.gz": "99"}},
		"checksums.txt disagrees":           {listed: map[string]string{"a.dmg": "99"}},
		"checksums.txt lists a missing one": {listed: map[string]string{"a.dmg": "11", "b.dmg": "33"}},
	} {
		if err := checkHashes(local, c.github, c.listed); err == nil {
			t.Errorf("%s: accepted", name)
		}
	}
}

const mirroredManifest = `{
  "version": "0.60.0",
  "platforms": {
    "darwin-universal": {"signature": "s", "url": "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_universal.app.tar.gz"},
    "darwin-aarch64": {"signature": "s", "url": "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_aarch64.app.tar.gz"},
    "darwin-x86_64": {"signature": "s", "url": "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_x64.app.tar.gz"}
  }
}`

func TestCheckManifestAcceptsOneNamingTheMirror(t *testing.T) {
	if err := checkManifest([]byte(mirroredManifest), "0.60.0", "https://download.getcmdr.com"); err != nil {
		t.Fatal(err)
	}
}

func TestCheckManifestRefusesAnyURLOffTheMirror(t *testing.T) {
	for name, body := range map[string]string{
		"github":        strings.Replace(mirroredManifest, "https://download.getcmdr.com/v0.60.0/Cmdr_0.60.0_x64", "https://github.com/getcmdr/cmdr/releases/download/v0.60.0/Cmdr_0.60.0_x64", 1),
		"wrong arch":    strings.Replace(mirroredManifest, "Cmdr_0.60.0_x64.app", "Cmdr_0.60.0_aarch64.app", 1),
		"wrong version": strings.Replace(mirroredManifest, `"version": "0.60.0"`, `"version": "0.59.0"`, 1),
		"no platform":   strings.Replace(mirroredManifest, `"darwin-x86_64"`, `"linux-x86_64"`, 1),
		"not json":      "{",
	} {
		if err := checkManifest([]byte(body), "0.60.0", "https://download.getcmdr.com"); err == nil {
			t.Errorf("%s: accepted", name)
		}
	}
}
