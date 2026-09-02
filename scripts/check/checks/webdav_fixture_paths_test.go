package checks

import (
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"

	"cmdr/scripts/check/stacklease"
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

// webdavStartModeRE captures one arm of `start.sh`'s mode table: the mode name
// and everything up to its `;;`.
var webdavStartModeRE = regexp.MustCompile(`(?s)\n {4}(\w+)\)\n(.*?);;`)

// webdavStartServicesRE captures the service list an arm assigns.
var webdavStartServicesRE = regexp.MustCompile(`services=\(([^)]*)\)`)

// The WebDAV stack's mode table lives in THREE places that must agree: the lease
// registry (what a check run brings up), `start.sh` (what a bare bring-up does),
// and this package's core/nextcloud service lists (what a lane waits on). A
// drift shows up as a cell with no server, which reads as a backend bug rather
// than as a fixture one — or, since Nextcloud landed, as a lane spending its
// whole timeout waiting for a container its own mode never started.
func TestWebdavModeServicesAgree(t *testing.T) {
	root := repoRootForTest(t)
	start := readRepoFile(t, root, webdavStartRel)

	fromStart := map[string][]string{}
	for _, arm := range webdavStartModeRE.FindAllStringSubmatch(start, -1) {
		mode, body := arm[1], arm[2]
		if mode == "all" {
			// `all` deliberately assigns nothing: empty means "every service"
			// to both `compose up` and the probe loop.
			fromStart[mode] = nil
			continue
		}
		m := webdavStartServicesRE.FindStringSubmatch(body)
		if m == nil {
			t.Errorf("%s: the %q arm assigns no `services=(…)`, so it would bring up the WHOLE project", webdavStartRel, mode)
			continue
		}
		fromStart[mode] = strings.Fields(m[1])
	}

	for _, mode := range stacklease.WEBDAV.Modes() {
		want := stacklease.WEBDAV.ServicesForMode(mode)
		got, ok := fromStart[mode]
		if !ok {
			t.Errorf("%s has no %q arm; the lease registry serves that mode, so a bare `start.sh %s` would refuse a mode a check run accepts", webdavStartRel, mode, mode)
			continue
		}
		if strings.Join(got, " ") != strings.Join(want, " ") {
			t.Errorf("%s brings up %v for %q; the lease registry brings up %v. A cell then talks to a server nobody started", webdavStartRel, got, mode, want)
		}
	}
	for mode := range fromStart {
		if !contains(stacklease.WEBDAV.Modes(), mode) {
			t.Errorf("%s offers the mode %q the lease registry doesn't serve, so `start.sh %s` and a check run disagree about what is up", webdavStartRel, mode, mode)
		}
	}

	// The two readiness lists this package hands the lanes, against the same
	// source of truth.
	for _, c := range []struct {
		mode string
		got  []string
	}{
		{stacklease.ModeCore, WebdavFixtureServices()},
		{stacklease.ModeNextcloud, WebdavNextcloudServices()},
	} {
		want := stacklease.WEBDAV.ServicesForMode(c.mode)
		gotSorted, wantSorted := append([]string(nil), c.got...), append([]string(nil), want...)
		sort.Strings(gotSorted)
		sort.Strings(wantSorted)
		if strings.Join(gotSorted, " ") != strings.Join(wantSorted, " ") {
			t.Errorf("the %q lane waits on %v; that mode brings up %v. Waiting on a container the mode never starts burns the whole timeout", c.mode, gotSorted, wantSorted)
		}
	}
}

// A fixture whose cells the SHARED lane subtracts has to have a lane of its own,
// or the subtraction is a way to retire a cell without deleting it.
func TestEveryOwnLaneFixtureHasALaneOfItsOwn(t *testing.T) {
	root := repoRootForTest(t)
	filter := fixtureIntegrationFilter(root)

	for _, fixture := range laneFixtures {
		if fixture.ownLaneTests == "" {
			continue
		}
		if !strings.Contains(filter, "- test("+fixture.ownLaneTests+")") {
			t.Errorf("%s declares the out-of-lane atom %q, but the shared filter %q doesn't subtract it, so those cells run in a lane whose stack never comes up",
				fixture.name, fixture.ownLaneTests, filter)
		}
		var lane *CheckDefinition
		for i := range AllChecks {
			if AllChecks[i].ID == fixture.ownLaneCheckID {
				lane = &AllChecks[i]
			}
		}
		if lane == nil {
			t.Errorf("%s subtracts %q from the shared lane and names %q as the lane that runs them; no check by that id is registered, so those cells run nowhere",
				fixture.name, fixture.ownLaneTests, fixture.ownLaneCheckID)
			continue
		}
		if len(lane.NeedsContainers) == 0 {
			t.Errorf("%s declares no fixture stack, so the cells it inherits from %s's subtraction would run against nothing", lane.ID, fixture.name)
		}
	}
}
