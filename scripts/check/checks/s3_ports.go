package checks

import (
	"os"
	"strconv"
	"strings"
)

// The S3 fixture stack runs on a host-port range of its own, 14480+, disjoint
// from WebDAV's 13480+, SFTP's 12480+, cmdr's vendored `smb-consumer` stack at
// 11480+, and smb2's own harness at 10480+. Same reason as the others: two
// stacks sharing a range made them mutually exclusive on one machine.
//
// The compose file reads each one as `${S3_FIXTURE_<SERVICE>_PORT:-<default>}`,
// and the defaults match this table (`TestS3FixturePortsMatchComposeDefaults`),
// so a bare `start.sh` with no runner around it lands on the same ports. Every
// child of this process (compose via the lease helper, cargo nextest) inherits
// the pinned values.
var s3ServiceHostPorts = map[string]int{
	"VERSITYGW": 14480, "GARAGE": 14481,
}

// s3CoreServices are the services `core` mode brings up, which is the set the
// integration lane waits on. `TestS3ModeServicesAgree` keeps it equal to
// `stacklease.S3`'s.
var s3CoreServices = []string{"VERSITYGW", "GARAGE"}

// s3BindAddrEnv names the interface every fixture port publishes on. The compose
// file defaults it to 127.0.0.1; nothing here sets it.
// `TestS3FixturePortsBindToLoopback` fails the run if a `ports:` entry loses the
// prefix.
const s3BindAddrEnv = "S3_BIND_ADDR"

// ApplyS3PortEnv pins the S3 stack to its dedicated host-port range in the
// current process environment. Call once before bringing the stack up.
// Idempotent.
func ApplyS3PortEnv() {
	for service, port := range s3ServiceHostPorts {
		_ = os.Setenv("S3_FIXTURE_"+service+"_PORT", strconv.Itoa(port))
	}
}

// S3FixtureServices lists every service the S3 core mode brings up, for the
// integration lane's readiness guard.
func S3FixtureServices() []string {
	services := make([]string, 0, len(s3CoreServices))
	for _, key := range s3CoreServices {
		services = append(services, "s3-fixture-"+strings.ToLower(key))
	}
	return services
}
