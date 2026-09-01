package checks

import (
	"strconv"
	"strings"
	"testing"
)

// The WebDAV stack's host ports are named in two places that must agree: Go
// decides them, and the compose file carries a `:-` default for the runner-less
// path. Same promise as SFTP's (`sftp_fixture_paths_test.go`), same guard; no
// keys-dir half, because HTTP publishes no key material.

func TestWebdavFixturePortsMatchComposeDefaults(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, webdavComposeRel)

	composePorts := map[string]int{}
	for _, m := range composeDefaultRE.FindAllStringSubmatch(compose, -1) {
		suffix, ok := trimWebdavPortEnv(m[1])
		if !ok {
			continue
		}
		port, err := strconv.Atoi(m[2])
		if err != nil {
			t.Errorf("%s: ${%s:-%s} has a non-numeric default", webdavComposeRel, m[1], m[2])
			continue
		}
		composePorts[suffix] = port
	}

	for service, want := range webdavServiceHostPorts {
		got, ok := composePorts[service]
		if !ok {
			t.Errorf("webdavServiceHostPorts has %s (%d) but %s declares no ${WEBDAV_FIXTURE_%s_PORT:-…} default; a bare start.sh would land the service somewhere the suite never looks", service, want, webdavComposeRel, service)
			continue
		}
		if got != want {
			t.Errorf("%s defaults WEBDAV_FIXTURE_%s_PORT to %d; webdavServiceHostPorts pins %d. A bare start.sh and a check run would then serve the same fixture on two ports", webdavComposeRel, service, got, want)
		}
	}

	// Every compose service has to be in the table: the lane waits on the whole
	// table, and a service outside it is one the lane never guards.
	extras := map[string]bool{}
	for service := range composePorts {
		if _, inTable := webdavServiceHostPorts[service]; !inTable {
			extras[service] = true
		}
	}
	if len(extras) != 0 {
		t.Errorf("%s declares port defaults for %s that webdavServiceHostPorts doesn't carry. See the fixture README's \"Adding a server\"",
			webdavComposeRel, sortedKeys(extras))
	}
}

func TestWebdavFixturePortsBindToLoopback(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, webdavComposeRel)

	const wantPrefix = "${" + webdavBindAddrEnv + ":-127.0.0.1}:"

	matches := sftpComposePortsRE.FindAllStringSubmatch(compose, -1)
	if len(matches) != len(webdavServiceHostPorts) {
		t.Fatalf("%s declares %d `ports:` entries; %d services are in webdavServiceHostPorts. A service publishing no port, or a second publish on one, means this guard is reading the wrong set", webdavComposeRel, len(matches), len(webdavServiceHostPorts))
	}

	for _, m := range matches {
		if !strings.HasPrefix(m[1], wantPrefix) {
			t.Errorf("%s publishes `%s` with no %q prefix, so Docker binds it on 0.0.0.0 and puts a writable DAV export whose credentials this repo documents in public on the LAN and the tailnet of whoever runs the suite", webdavComposeRel, m[1], wantPrefix)
		}
	}
}

func trimWebdavPortEnv(name string) (string, bool) {
	const prefix, suffix = "WEBDAV_FIXTURE_", "_PORT"
	if len(name) <= len(prefix)+len(suffix) {
		return "", false
	}
	if name[:len(prefix)] != prefix || name[len(name)-len(suffix):] != suffix {
		return "", false
	}
	return name[len(prefix) : len(name)-len(suffix)], true
}
