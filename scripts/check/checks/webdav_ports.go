package checks

import (
	"os"
	"strconv"
	"strings"
)

// The WebDAV fixture stack runs on a host-port range of its own, 13480+,
// disjoint from SFTP's 12480+, cmdr's vendored `smb-consumer` stack at 11480+,
// and smb2's own harness at 10480+. Two stacks sharing a range made them
// mutually exclusive on one machine — a stack leaked by an interrupted run
// squats the ports and blocks every later run with "port is already allocated".
//
// The compose file reads each one as `${WEBDAV_FIXTURE_<SERVICE>_PORT:-<default>}`
// and the Rust fixtures read the same variable back
// (`cmdr_webdav::volume::testing::fixture_port`), so pinning the range in this
// process is enough: every child inherits it. The compose defaults match this
// table (`TestWebdavFixturePortsMatchComposeDefaults`), so a bare `start.sh`
// with no runner around it lands on the same ports.
var webdavServiceHostPorts = map[string]int{
	"APACHE": 13480, "DIGEST": 13481,
}

// webdavBindAddrEnv names the interface every fixture port publishes on. The
// compose file defaults it to 127.0.0.1 so the stack is invisible to the LAN and
// the tailnet; nothing here sets it. `TestWebdavFixturePortsBindToLoopback`
// fails the run if a `ports:` entry loses the prefix.
const webdavBindAddrEnv = "WEBDAV_BIND_ADDR"

// ApplyWebdavPortEnv pins the WebDAV stack to its dedicated host-port range in
// the current process environment, so every child (compose via the lease helper,
// cargo nextest) inherits it. Call once before bringing the stack up. Idempotent.
func ApplyWebdavPortEnv() {
	for service, port := range webdavServiceHostPorts {
		_ = os.Setenv("WEBDAV_FIXTURE_"+service+"_PORT", strconv.Itoa(port))
	}
}

// WebdavFixtureServices lists every service the WebDAV core mode brings up, for
// the integration lane's readiness guard. Derived from the port table so the
// two can't drift.
func WebdavFixtureServices() []string {
	services := make([]string, 0, len(webdavServiceHostPorts))
	for service := range webdavServiceHostPorts {
		services = append(services, "webdav-fixture-"+strings.ToLower(service))
	}
	return services
}
