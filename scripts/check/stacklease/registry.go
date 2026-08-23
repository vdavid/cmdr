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
	// ModeBench is the SFTP stack's local-only measurement server. ❗ No check
	// declares it and CI never brings it up: a throughput number measured under
	// runner contention is a flake, not a gate.
	ModeBench = "bench"
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
// ❗ `modeServices` has to stay in lock-step with the fixture's own case table
// (`apps/desktop/test/sftp-servers/start.sh`). A drift between them shows up as a
// cell with no server, which reads as a backend bug rather than as a fixture one.
var SFTP = &Stack{
	Name:         "sftp",
	ProjectName:  "sftp-fixture",
	lockFile:     "cmdr-sftp.lock",
	leaseDirName: "cmdr-sftp-leases",
	// The two key-auth services bind-mount a host directory to publish the key
	// pair they generate at start. ❗ It lives beside the lock and the lease dir
	// because it is machine-wide state of a machine-wide stack: a per-checkout
	// path bakes the starting worktree into a live container that sibling
	// worktrees then ADOPT, so deleting that worktree breaks key auth for all of
	// them. `apps/desktop/test/sftp-servers/docker-compose.yml`,
	// `start.sh`, and `cmdr_sftp::volume::testing::fixture_key_path` read the
	// same `CMDR_SFTP_KEYS_DIR` with the same default; `TestSftpFixturePathsAgree`
	// is what keeps those four copies equal.
	keysDirName:   "cmdr-sftp-keys",
	keysSubdirs:   []string{"keyonly", "passphrase"},
	keysDirEnv:    "CMDR_SFTP_KEYS_DIR",
	composeDirRel: "apps/desktop/test/sftp-servers",
	composeDirEnv: "CMDR_SFTP_COMPOSE_DIR",
	composeFiles:  []string{"docker-compose.yml"},
	// Host ports live in their own pinned range, clear of SMB's 11480+ and
	// smb2's 10480+.
	portEnvPrefix: "SFTP_FIXTURE_",
	modeServices: map[string][]string{
		ModeMinimal: {"sftp-fixture-openssh", "sftp-fixture-keyonly"},
		ModeCore: {
			"sftp-fixture-openssh", "sftp-fixture-keyonly", "sftp-fixture-passphrase",
			"sftp-fixture-kbdint", "sftp-fixture-twokeys", "sftp-fixture-changedkey",
			"sftp-fixture-noposixrename", "sftp-fixture-shortreads",
			"sftp-fixture-smalllimits", "sftp-fixture-bigdir", "sftp-fixture-oddnames",
		},
		ModeBench: {"sftp-fixture-bench"},
		ModeAll:   nil,
	},
	// Empty, and it stays that way: the one image bakes a HEALTHCHECK that reads
	// the listening socket out of `netstat`, so every service reports health.
	// (`nc -z`, which SMB's images use, is not implemented by busybox — it
	// answers 1 unconditionally, which reads as a container that never comes up.)
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
