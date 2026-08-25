package checks

import (
	"os"
	"strconv"
	"strings"
)

// The SFTP fixture stack runs on a host-port range of its own, 12480+, disjoint
// from SMB's two: cmdr's vendored `smb-consumer` stack owns 11480+ and smb2's own
// test harness defaults to 10480+. Two stacks sharing a range made them mutually
// exclusive on one machine — a stack leaked by an interrupted run squats the
// ports and blocks every later run with "port is already allocated".
//
// The compose file reads each one as `${SFTP_FIXTURE_<SERVICE>_PORT:-<default>}`
// and the Rust fixtures read the same variable back
// (`cmdr_sftp::volume::testing::fixture_port`), so pinning the range in this
// process is enough: every child inherits it. The compose defaults match this
// table, so a bare `start.sh` with no runner around it lands on the same ports.
var sftpServiceHostPorts = map[string]int{
	"OPENSSH": 12480, "KEYONLY": 12481, "PASSPHRASE": 12482, "KBDINT": 12483,
	"TWOKEYS": 12484, "CHANGEDKEY": 12485, "NOPOSIXRENAME": 12486,
	"SHORTREADS": 12487, "SMALLLIMITS": 12488, "BIGDIR": 12489, "ODDNAMES": 12490,
}

// sftpBindAddrEnv names the interface every fixture port publishes on. The
// compose file defaults it to 127.0.0.1 so the stack is invisible to the LAN and
// the tailnet; nothing here sets it, and setting it to 0.0.0.0 is the deliberate
// escape hatch for a NAT'd VM or a second machine, which reach the host by
// gateway IP and can't see loopback. `TestSftpFixturePortsBindToLoopback` fails
// the run if a `ports:` entry loses the prefix.
const sftpBindAddrEnv = "SFTP_BIND_ADDR"

// ApplySftpPortEnv pins the SFTP stack to its dedicated host-port range in the
// current process environment, so every child (compose via the lease helper,
// cargo nextest) inherits it. Call once before bringing the stack up. Idempotent.
func ApplySftpPortEnv() {
	for service, port := range sftpServiceHostPorts {
		_ = os.Setenv("SFTP_FIXTURE_"+service+"_PORT", strconv.Itoa(port))
	}
}

// SftpFixtureServices lists every service the SFTP core mode brings up, for the
// integration lane's readiness guard. Derived from the port table so the two
// can't drift.
func SftpFixtureServices() []string {
	services := make([]string, 0, len(sftpServiceHostPorts))
	for service := range sftpServiceHostPorts {
		services = append(services, "sftp-fixture-"+strings.ToLower(service))
	}
	return services
}
