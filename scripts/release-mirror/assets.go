package main

import (
	"encoding/json"
	"fmt"
	"regexp"
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

// mirroredAssets picks, from a release's asset names, the files the EU mirror serves: every
// update archive and its signature, every DMG, the pkg when the release has one, and
// `checksums.txt` last, so it never names a file the mirror doesn't hold yet. The SBOMs, the
// provenance bundle, and `latest.json` stay on GitHub only (the website serves the manifest).
// A release missing any required file is refused, naming each one.
func mirroredAssets(names []string, version string) ([]string, error) {
	have := map[string]bool{}
	for _, n := range names {
		have[n] = true
	}
	var want, missing []string
	for _, arch := range arches {
		base := "Cmdr_" + version + "_" + arch
		for _, name := range []string{base + ".app.tar.gz", base + ".app.tar.gz.sig", base + ".dmg"} {
			want = append(want, name)
			if !have[name] {
				missing = append(missing, name)
			}
		}
	}
	if pkg := "Cmdr_" + version + "_universal.pkg"; have[pkg] {
		want = append(want, pkg)
	}
	slices.Sort(want)
	want = append(want, "checksums.txt")
	if !have["checksums.txt"] {
		missing = append(missing, "checksums.txt")
	}
	if len(missing) > 0 {
		return nil, fmt.Errorf("release %s is missing files the mirror needs: %s", version, strings.Join(missing, ", "))
	}
	return want, nil
}

// objectKey is where a release file lives in the bucket: one folder per version, so a key is
// never reused and every object can be cached forever.
func objectKey(version, name string) string {
	return "v" + version + "/" + name
}

// publicURL is the file's address on the mirror's public host.
func publicURL(host, version, name string) string {
	return strings.TrimSuffix(host, "/") + "/" + objectKey(version, name)
}

// immutableCache is every object's `Cache-Control`. A key names one version's one file, and the
// mirror runs only once the release's bytes are final, so a cached copy never goes stale.
const immutableCache = "public, max-age=31536000, immutable"

func contentType(name string) string {
	switch {
	case strings.HasSuffix(name, ".dmg"):
		return "application/x-apple-diskimage"
	case strings.HasSuffix(name, ".tar.gz"):
		return "application/gzip"
	case strings.HasSuffix(name, ".sig"), strings.HasSuffix(name, ".txt"):
		return "text/plain; charset=utf-8"
	default:
		return "application/octet-stream"
	}
}

var checksumLineRE = regexp.MustCompile(`^([0-9a-f]{64})  (\S+)$`)

// parseChecksums reads `checksums.txt` (`shasum -a 256` output) into name → hex SHA-256.
func parseChecksums(body string) (map[string]string, error) {
	sums := map[string]string{}
	for line := range strings.Lines(body) {
		line = strings.TrimRight(line, "\r\n")
		if line == "" {
			continue
		}
		m := checksumLineRE.FindStringSubmatch(line)
		if m == nil {
			return nil, fmt.Errorf("checksums.txt has a line that isn't `<sha256>  <name>`: %q", line)
		}
		sums[m[2]] = m[1]
	}
	return sums, nil
}

// checkManifest requires `latest.json` to name this version and, for each of the three
// platforms, exactly this release's archive for its arch on the mirror. Every installed copy
// fetches these URLs verbatim with no fallback, so a URL off the mirror (or a drifted name)
// must fail the release while nothing has shipped.
func checkManifest(body []byte, version, host string) error {
	var m struct {
		Version   string `json:"version"`
		Platforms map[string]struct {
			URL string `json:"url"`
		} `json:"platforms"`
	}
	if err := json.Unmarshal(body, &m); err != nil {
		return fmt.Errorf("latest.json isn't JSON: %w", err)
	}
	if m.Version != version {
		return fmt.Errorf("latest.json says version %q, not %q", m.Version, version)
	}
	for platform, arch := range archByPlatform {
		p, ok := m.Platforms[platform]
		if !ok {
			return fmt.Errorf("latest.json has no %s platform", platform)
		}
		want := publicURL(host, version, "Cmdr_"+version+"_"+arch+".app.tar.gz")
		if p.URL != want {
			return fmt.Errorf("latest.json's %s URL is %s, not %s", platform, p.URL, want)
		}
	}
	return nil
}
