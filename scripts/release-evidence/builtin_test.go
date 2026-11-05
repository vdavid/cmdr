package main

import (
	"fmt"
	"strings"
	"testing"
)

func manifestNaming(repo, version string) []byte {
	return []byte(fmt.Sprintf(`{"version": %q, "platforms": {
    "darwin-aarch64": {"url": "https://github.com/%[2]s/releases/download/v%[1]s/Cmdr_%[1]s_aarch64.app.tar.gz"},
    "darwin-universal": {"url": "https://github.com/%[2]s/releases/download/v%[1]s/Cmdr_%[1]s_universal.app.tar.gz"},
    "darwin-x86_64": {"url": "https://github.com/%[2]s/releases/download/v%[1]s/Cmdr_%[1]s_x64.app.tar.gz"}
  }}`, version, repo))
}

func mirrorManifest(version string) []byte {
	return []byte(fmt.Sprintf(`{"version": %q, "platforms": {
    "darwin-aarch64": {"url": "https://download.getcmdr.com/v%[1]s/Cmdr_%[1]s_aarch64.app.tar.gz"},
    "darwin-universal": {"url": "https://download.getcmdr.com/v%[1]s/Cmdr_%[1]s_universal.app.tar.gz"},
    "darwin-x86_64": {"url": "https://download.getcmdr.com/v%[1]s/Cmdr_%[1]s_x64.app.tar.gz"}
  }}`, version))
}

func TestBuiltInRepo(t *testing.T) {
	mixed := strings.Replace(string(manifestNaming("vdavid/cmdr", "0.55.0")), "vdavid/cmdr", "getcmdr/cmdr", 1)
	cases := []struct {
		name, current, builtIn, wantErr string
		manifest                        []byte
	}{
		{name: "built where it lives", current: "vdavid/cmdr", manifest: manifestNaming("vdavid/cmdr", "0.55.0"), builtIn: "vdavid/cmdr"},
		{name: "built after the move", current: "getcmdr/cmdr", manifest: manifestNaming("getcmdr/cmdr", "0.55.0"), builtIn: "getcmdr/cmdr"},
		// The release that matters most: the last one before the move, checked after it.
		{name: "built before the move", current: "getcmdr/cmdr", manifest: manifestNaming("vdavid/cmdr", "0.55.0"), builtIn: "vdavid/cmdr"},
		{name: "a repo that was never ours", current: "getcmdr/cmdr", manifest: manifestNaming("evil/cmdr", "0.55.0"), wantErr: "evil/cmdr"},
		{name: "the org name before any move", current: "vdavid/cmdr", manifest: manifestNaming("getcmdr/cmdr", "0.55.0"), wantErr: "getcmdr/cmdr"},
		{name: "another version's manifest", current: "vdavid/cmdr", manifest: manifestNaming("vdavid/cmdr", "0.54.0"), wantErr: "0.54.0"},
		{name: "two repos in one manifest", current: "getcmdr/cmdr", manifest: []byte(mixed), wantErr: "more than one"},
		{name: "not JSON", current: "vdavid/cmdr", manifest: []byte("nope"), wantErr: "JSON"},
		// The EU mirror's URLs carry no repo name: the release on this repo is the one it built.
		{name: "on the EU mirror", current: "vdavid/cmdr", manifest: mirrorManifest("0.55.0"), builtIn: "vdavid/cmdr"},
		{name: "on a host that isn't ours", current: "vdavid/cmdr", manifest: []byte(strings.ReplaceAll(string(mirrorManifest("0.55.0")), "download.getcmdr.com", "evil.example")), wantErr: "evil.example"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, err := builtInRepo(c.manifest, "0.55.0", c.current)
			if c.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), c.wantErr) {
					t.Fatalf("got %q, %v; want an error naming %q", got, err, c.wantErr)
				}
				return
			}
			if err != nil || got != c.builtIn {
				t.Fatalf("got %q, %v; want %q", got, err, c.builtIn)
			}
		})
	}
}
