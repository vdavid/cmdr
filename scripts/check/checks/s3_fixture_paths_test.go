package checks

import (
	"sort"
	"strconv"
	"strings"
	"testing"

	"cmdr/scripts/check/stacklease"
)

// The S3 stack makes the same promises as WebDAV's (`webdav_fixture_paths_test.go`):
// Go decides the host ports and the compose file carries matching `:-` defaults,
// every port binds to loopback, and the mode table agrees across the lease
// registry, `start.sh`, and the lane's readiness list.

func TestS3FixturePortsMatchComposeDefaults(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, s3ComposeRel)

	composePorts := map[string]int{}
	for _, m := range composeDefaultRE.FindAllStringSubmatch(compose, -1) {
		suffix, ok := strings.CutPrefix(m[1], "S3_FIXTURE_")
		if !ok {
			continue
		}
		suffix, ok = strings.CutSuffix(suffix, "_PORT")
		if !ok || suffix == "" {
			continue
		}
		port, err := strconv.Atoi(m[2])
		if err != nil {
			t.Errorf("%s: ${%s:-%s} has a non-numeric default", s3ComposeRel, m[1], m[2])
			continue
		}
		composePorts[suffix] = port
	}

	for service, want := range s3ServiceHostPorts {
		got, ok := composePorts[service]
		if !ok {
			t.Errorf("s3ServiceHostPorts has %s (%d) but %s declares no ${S3_FIXTURE_%s_PORT:-…} default; a bare start.sh would land the service somewhere the suite never looks", service, want, s3ComposeRel, service)
			continue
		}
		if got != want {
			t.Errorf("%s defaults S3_FIXTURE_%s_PORT to %d; s3ServiceHostPorts pins %d. A bare start.sh and a check run would then serve the same fixture on two ports", s3ComposeRel, service, got, want)
		}
	}

	extras := map[string]bool{}
	for service := range composePorts {
		if _, inTable := s3ServiceHostPorts[service]; !inTable {
			extras[service] = true
		}
	}
	if len(extras) != 0 {
		t.Errorf("%s declares port defaults for %s that s3ServiceHostPorts doesn't carry. See the fixture README's \"Adding a server\"",
			s3ComposeRel, sortedKeys(extras))
	}
}

func TestS3FixturePortsBindToLoopback(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, s3ComposeRel)

	const wantPrefix = "${" + s3BindAddrEnv + ":-127.0.0.1}:"

	matches := sftpComposePortsRE.FindAllStringSubmatch(compose, -1)
	if len(matches) != len(s3ServiceHostPorts) {
		t.Fatalf("%s declares %d `ports:` entries; %d services are in s3ServiceHostPorts. A service publishing no port, or a second publish on one, means this guard is reading the wrong set", s3ComposeRel, len(matches), len(s3ServiceHostPorts))
	}

	for _, m := range matches {
		if !strings.HasPrefix(m[1], wantPrefix) {
			t.Errorf("%s publishes `%s` with no %q prefix, so Docker binds it on 0.0.0.0 and puts a writable bucket whose credentials this repo documents in public on the LAN and the tailnet of whoever runs the suite", s3ComposeRel, m[1], wantPrefix)
		}
	}
}

func TestS3ModeServicesAgree(t *testing.T) {
	root := repoRootForTest(t)
	start := readRepoFile(t, root, s3StartRel)

	// `start.sh` shares WebDAV's case-table shape, so the same two patterns read it.
	fromStart := map[string][]string{}
	for _, arm := range webdavStartModeRE.FindAllStringSubmatch(start, -1) {
		mode, body := arm[1], arm[2]
		if mode == "all" {
			fromStart[mode] = nil
			continue
		}
		m := webdavStartServicesRE.FindStringSubmatch(body)
		if m == nil {
			t.Errorf("%s: the %q arm assigns no `services=(…)`, so it would bring up the WHOLE project", s3StartRel, mode)
			continue
		}
		fromStart[mode] = strings.Fields(m[1])
	}

	for _, mode := range stacklease.S3.Modes() {
		want := stacklease.S3.ServicesForMode(mode)
		got, ok := fromStart[mode]
		if !ok {
			t.Errorf("%s has no %q arm; the lease registry serves that mode, so a bare `start.sh %s` would refuse a mode a check run accepts", s3StartRel, mode, mode)
			continue
		}
		if strings.Join(got, " ") != strings.Join(want, " ") {
			t.Errorf("%s brings up %v for %q; the lease registry brings up %v. A cell then talks to a server nobody started", s3StartRel, got, mode, want)
		}
	}
	for mode := range fromStart {
		if !contains(stacklease.S3.Modes(), mode) {
			t.Errorf("%s offers the mode %q the lease registry doesn't serve, so `start.sh %s` and a check run disagree about what is up", s3StartRel, mode, mode)
		}
	}

	got, want := append([]string(nil), S3FixtureServices()...), append([]string(nil), stacklease.S3.ServicesForMode(stacklease.ModeCore)...)
	sort.Strings(got)
	sort.Strings(want)
	if strings.Join(got, " ") != strings.Join(want, " ") {
		t.Errorf("the integration lane waits on %v; S3's core mode brings up %v. Waiting on a container the mode never starts burns the whole timeout", got, want)
	}
}
