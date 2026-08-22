package stacklease

import (
	"fmt"
	"sort"
	"strings"
)

// Mode names the service set a caller wants from a stack. Every stack defines
// its own table; these are the names the two current stacks share.
const (
	// ModeMinimal is the smallest set that answers a smoke test.
	ModeMinimal = "minimal"
	// ModeE2E is what the Linux Docker E2E suite talks to.
	ModeE2E = "e2e"
	// ModeCore is the integration-test set: every fixture an assertion needs.
	ModeCore = "core"
	// ModeAll is every service the compose project defines.
	ModeAll = "all"
)

// SMB is the SMB fixture stack: smb2's consumer harness, vendored into
// `apps/desktop/test/smb-servers/.compose` and layered under a cmdr-owned
// override. Its lock file and lease dir keep their historical names because a
// sibling worktree on older code holds a lease there.
var SMB = &Stack{
	Name:          "smb",
	ProjectName:   "smb-consumer",
	lockFile:      "cmdr-smb.lock",
	leaseDirName:  "cmdr-smb-leases",
	composeDirRel: "apps/desktop/test/smb-servers/.compose",
	composeDirEnv: "CMDR_SMB_COMPOSE_DIR",
	// The vendored compose plus the cmdr-owned override (`restart`, `mem_limit`,
	// `cpus`), in that order. Only `up` applies those keys.
	composeFiles:  []string{"docker-compose.yml", "docker-compose.override.yml"},
	portEnvPrefix: "SMB_CONSUMER_",
	modeServices: map[string][]string{
		ModeMinimal: {"smb-consumer-guest", "smb-consumer-auth"},
		ModeE2E: {
			"smb-consumer-guest", "smb-consumer-auth",
			"smb-consumer-50shares", "smb-consumer-unicode",
		},
		ModeCore: {
			"smb-consumer-guest", "smb-consumer-auth", "smb-consumer-both",
			"smb-consumer-readonly", "smb-consumer-flaky", "smb-consumer-slow",
			"smb-consumer-maxreadsize", "smb-consumer-50shares",
			"smb-consumer-unicode",
		},
		ModeAll: nil,
	},
	// `smb-consumer-flaky` cycles up/down by design and ships no HEALTHCHECK;
	// every other service bakes `HEALTHCHECK nc -z localhost 445`.
	servicesWithoutHealthcheck: map[string]bool{"smb-consumer-flaky": true},
}

// SFTP is the SFTP fixture stack: first-party, so one compose file sitting
// directly in the fixture dir rather than a vendored base plus an override under
// `.compose/`.
//
// Its service table is empty until the fixture lands under
// `apps/desktop/test/sftp-servers/`. Registration is inert data, so an empty
// table costs nothing: no check asks for this stack yet, `Acquire` refuses every
// mode with the list of modes it serves, and `Up` reports the unresolvable
// compose dir rather than letting docker guess at a compose file. Filling the
// table and pointing a check at it are the two lines that turn the lane on.
var SFTP = &Stack{
	Name:          "sftp",
	ProjectName:   "sftp-fixture",
	lockFile:      "cmdr-sftp.lock",
	leaseDirName:  "cmdr-sftp-leases",
	composeDirRel: "apps/desktop/test/sftp-servers",
	composeDirEnv: "CMDR_SFTP_COMPOSE_DIR",
	composeFiles:  []string{"docker-compose.yml"},
	// Host ports live in their own pinned range, clear of SMB's 11480+ and
	// smb2's 10480+.
	portEnvPrefix:              "SFTP_FIXTURE_",
	modeServices:               map[string][]string{},
	servicesWithoutHealthcheck: map[string]bool{},
}

// registered is every stack this package leases, keyed by name.
var registered = map[string]*Stack{
	SMB.Name:  SMB,
	SFTP.Name: SFTP,
}

// Lookup resolves a stack by name, listing the registered names when it can't.
// The CLI and the check runner both go through it, so a typo is a loud error
// rather than a silently unleased stack.
func Lookup(name string) (*Stack, error) {
	if s, ok := registered[name]; ok {
		return s, nil
	}
	return nil, fmt.Errorf("unknown fixture stack %q; registered: %s", name, strings.Join(Names(), ", "))
}

// All returns every registered stack in name order.
func All() []*Stack {
	out := make([]*Stack, 0, len(registered))
	for _, name := range Names() {
		out = append(out, registered[name])
	}
	return out
}

// Names lists the registered stack names, sorted.
func Names() []string {
	names := make([]string, 0, len(registered))
	for name := range registered {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}
