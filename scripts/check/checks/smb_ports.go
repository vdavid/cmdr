package checks

import (
	"os"
	"strconv"
)

// cmdr runs its vendored copy of smb2's `consumer` SMB stack on a DEDICATED host
// port range (11480+), disjoint from smb2's own test harness, which defaults to
// 10480+. Both bring up the same compose under different Docker project names
// (cmdr's `smb-consumer` vs smb2's `consumer`), so sharing ports made the two
// mutually exclusive: a stack leaked by an interrupted smb2 test run (its
// `Drop`-based teardown doesn't fire on SIGKILL) would squat 10480+ and block
// every cmdr `check.sh` run with "port is already allocated", and vice versa.
//
// Giving cmdr its own range lets the two coexist, and — crucially — leaves
// smb2's defaults and `guest_port()` contract untouched, so every OTHER smb2
// consumer is unaffected. We shift via smb2's existing per-service env override
// (`${SMB_CONSUMER_*_PORT:-…}` in the compose, read back by `guest_port()`), so
// the range flows everywhere by process-env inheritance — no script edits:
//   - `docker compose up` (start.sh) binds host ports from SMB_CONSUMER_*_PORT,
//   - the Rust integration tests resolve via smb2::testing::guest_port(),
//   - the macOS E2E app reads SMB_E2E_*_PORT (frontend fixture + virtual hosts).
//
// The Linux Docker E2E is unaffected: it talks to the containers over the Docker
// network on their internal :445, set explicitly in its `docker run -e` (which
// overrides anything inherited).
var smbServiceHostPorts = map[string]int{
	"GUEST": 11480, "AUTH": 11481, "BOTH": 11482, "50SHARES": 11483,
	"UNICODE": 11484, "LONGNAMES": 11485, "DEEPNEST": 11486, "MANYFILES": 11487,
	"READONLY": 11488, "WINDOWS": 11489, "SYNOLOGY": 11490, "LINUX": 11491,
	"FLAKY": 11492, "SLOW": 11493, "MAXREADSIZE": 11494,
}

// ApplySmbPortEnv pins cmdr's SMB stack to its dedicated host-port range in the
// current process environment, so every child process (compose via start.sh,
// cargo nextest, the E2E app) inherits it. Call once before bringing the stack
// up. Idempotent. Sets both env families — SMB_CONSUMER_*_PORT (compose +
// guest_port) and SMB_E2E_*_PORT (the E2E fixture + virtual hosts).
func ApplySmbPortEnv() {
	for svc, port := range smbServiceHostPorts {
		p := strconv.Itoa(port)
		_ = os.Setenv("SMB_CONSUMER_"+svc+"_PORT", p)
		_ = os.Setenv("SMB_E2E_"+svc+"_PORT", p)
	}
}
