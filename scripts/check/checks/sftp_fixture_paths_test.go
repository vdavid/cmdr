package checks

import (
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"testing"

	"cmdr/scripts/check/stacklease"
)

// The SFTP fixture stack's host paths and host ports are each named in four
// places that must agree, and nothing but this test can tell whether they do:
// Go decides them, the compose file and `start.sh` carry a `:-` default for the
// runner-less path, and the Rust fixtures read the same variable back.
//
// A drift is silent in the worst way. The keys dir is a bind SOURCE, so a
// mismatch leaves eleven healthy containers serving happily while every
// key-auth cell fails against a directory nobody writes — which reads as a
// backend bug, not a fixture one. That is exactly how the per-checkout `.keys/`
// path bit: compose resolved it against the STARTING worktree, the Rust reader
// resolved it against its own, and deleting the first worktree broke key auth
// for every worktree at once.
//
// The ports have carried the same promise in a comment ("the compose defaults
// match this table, so a bare start.sh lands on the same ports") with nothing
// enforcing it. They ride along here for that reason.

// composeDefaultRE captures one `${NAME:-default}` interpolation.
var composeDefaultRE = regexp.MustCompile(`\$\{([A-Z0-9_]+):-([^}]*)\}`)

// rustKeysDefaultRE captures the Rust fixtures' fallback keys dir.
var rustKeysDefaultRE = regexp.MustCompile(`const FIXTURE_KEYS_DIR_DEFAULT: &str = "([^"]*)"`)

// The three non-Go copies are `sftpComposeRel` / `sftpStartRel` / `sftpTestingRel`
// in `inputs.go`, where they also join the Go test lane's fingerprint.

func TestSftpFixturePathsAgree(t *testing.T) {
	root := repoRootForTest(t)

	// Neutralize both overrides so the stack answers with its shipped default,
	// which is what the other three files hard-code. Empty reads as unset to
	// every resolver involved.
	t.Setenv("CMDR_FIXTURE_LEASE_ROOT", "")
	t.Setenv("CMDR_SFTP_KEYS_DIR", "")

	keysEnv := stacklease.SFTP.KeysDirEnv()
	if keysEnv != "CMDR_SFTP_KEYS_DIR" {
		t.Fatalf("the SFTP keys-dir env var is %q; the compose file, start.sh, and %s all name CMDR_SFTP_KEYS_DIR", keysEnv, sftpTestingRel)
	}
	wantKeys := stacklease.SFTP.KeysDir()
	if !filepath.IsAbs(wantKeys) {
		t.Fatalf("the SFTP keys dir is %q; it has to be an absolute machine-wide path, or compose resolves it against whichever worktree brought the stack up", wantKeys)
	}

	compose := readRepoFile(t, root, sftpComposeRel)
	start := readRepoFile(t, root, sftpStartRel)
	testingRS := readRepoFile(t, root, sftpTestingRel)

	// 1. The compose file's default, once per key-auth service, plus the leaf
	//    each one binds.
	gotLeaves := map[string]bool{}
	for _, m := range regexp.MustCompile(`\$\{CMDR_SFTP_KEYS_DIR:-([^}]*)\}/([^:]+):/keys`).FindAllStringSubmatch(compose, -1) {
		if m[1] != wantKeys {
			t.Errorf("%s binds ${CMDR_SFTP_KEYS_DIR:-%s}; stacklease says %s", sftpComposeRel, m[1], wantKeys)
		}
		gotLeaves[m[2]] = true
	}
	wantLeaves := map[string]bool{}
	for _, leaf := range stacklease.SFTP.KeysLeaves() {
		wantLeaves[leaf.Dir] = true
	}
	if !sameStringSet(gotLeaves, wantLeaves) {
		t.Errorf("%s binds the leaves %s; stacklease creates %s. A leaf nobody creates is auto-made root-owned by Docker on Linux, and the container's own write into it fails",
			sftpComposeRel, sortedKeys(gotLeaves), sortedKeys(wantLeaves))
	}
	if len(gotLeaves) == 0 {
		t.Errorf("%s binds no keys dir at all; the regex above is what pins this file to the others, so a reshaped `volumes:` line would silently stop being checked", sftpComposeRel)
	}

	// 2. `start.sh`'s default, for a bare run with no check runner around it.
	assertSingleDefault(t, sftpStartRel, start, "CMDR_SFTP_KEYS_DIR", wantKeys)

	// 3. The Rust fixtures' fallback.
	rustKeys := rustKeysDefaultRE.FindStringSubmatch(testingRS)
	if rustKeys == nil {
		t.Fatalf("%s no longer declares `const FIXTURE_KEYS_DIR_DEFAULT`; that constant is what this guard reads, so renaming it turns the guard off", sftpTestingRel)
	}
	if rustKeys[1] != wantKeys {
		t.Errorf("%s falls back to %s; stacklease says %s", sftpTestingRel, rustKeys[1], wantKeys)
	}
	if !regexp.MustCompile(`"CMDR_SFTP_KEYS_DIR"`).MatchString(testingRS) {
		t.Errorf("%s never reads CMDR_SFTP_KEYS_DIR, so a run that moved the keys dir would read the wrong one", sftpTestingRel)
	}
	if regexp.MustCompile(`CARGO_MANIFEST_DIR[\s\S]{0,400}?sftp-servers`).MatchString(testingRS) {
		t.Errorf("%s derives a fixture stack path from CARGO_MANIFEST_DIR again. That resolves against the READING worktree while the container holds the STARTING one's path, and the two need not be the same checkout", sftpTestingRel)
	}

	// 4. The private key's BASENAME, a fifth copy of a shared name. The
	//    entrypoint writes it, the Rust fixtures open it, and the lease now
	//    stats it to tell a stack that lost its published key material from one
	//    that never had any. A drift leaves the lease healing forever and the
	//    suite reading a file nobody writes.
	wantFile := stacklease.SFTP.KeysFileName()
	if wantFile == "" {
		t.Fatal("the SFTP stack declares no key filename, so the lease cannot tell whether a container republished its pair")
	}
	entrypoint := readRepoFile(t, root, sftpEntrypointRel)
	if !strings.Contains(entrypoint, "/keys/"+wantFile) {
		t.Errorf("%s never writes /keys/%s; stacklease stats that path to decide whether a container republished its pair", sftpEntrypointRel, wantFile)
	}
	if !regexp.MustCompile(`\.join\("` + regexp.QuoteMeta(wantFile) + `"\)`).MatchString(testingRS) {
		t.Errorf("%s no longer joins %q onto the keys dir; stacklease and the entrypoint both name that file", sftpTestingRel, wantFile)
	}
}

// TestTheFixtureEntrypointRegeneratesAKeyItCanNoLongerBack pins the self-heal
// itself, because the failure it prevents is silent and expensive.
//
// The keys dir is a bind SOURCE under /tmp, which macOS empties on reboot, while
// the containers come back (`restart: unless-stopped`) still holding the
// `authorized_keys` they wrote before it. If the entrypoint provisions the pair
// only on its first run, that server spends the rest of its life naming a public
// key whose private half exists nowhere: healthy to `docker ps`, healthy to the
// HEALTHCHECK, and refusing every key-auth cell.
func TestTheFixtureEntrypointRegeneratesAKeyItCanNoLongerBack(t *testing.T) {
	root := repoRootForTest(t)
	entrypoint := readRepoFile(t, root, sftpEntrypointRel)

	// The `if` line itself, ❌ never a bare "/etc/fixture-configured": the file
	// names the marker in prose above the guard too.
	guard := strings.Index(entrypoint, "if [ -f /etc/fixture-configured ]")
	if guard < 0 {
		t.Fatalf("%s no longer guards on `if [ -f /etc/fixture-configured ]`; this test reads that line to find the once-per-container section", sftpEntrypointRel)
	}
	provision := strings.Index(entrypoint, "provision_client_key()")
	if provision < 0 {
		t.Fatalf("%s declares no provision_client_key; the key pair has to be provisionable on any start, not only the first", sftpEntrypointRel)
	}
	if provision > guard {
		t.Errorf("%s declares provision_client_key after the /etc/fixture-configured guard, so a restarted container can't call it", sftpEntrypointRel)
	}
	// The guard's own branch has to call it: that branch IS the restart path.
	branch := entrypoint[guard:]
	if end := strings.Index(branch, "\nfi\n"); end > 0 {
		branch = branch[:end]
	}
	if !strings.Contains(branch, "provision_client_key") {
		t.Errorf("%s short-circuits on /etc/fixture-configured without re-provisioning the client key. A container that outlived a wipe of the host keys dir would then serve an authorized_keys whose private half is gone, and every key-auth cell fails with an auth refusal that reads like a backend bug", sftpEntrypointRel)
	}
	// Byte-comparing the pair against authorized_keys is what makes it a
	// consistency check rather than a presence check; a half-written pair and a
	// previous container's pair both have to count as missing.
	if !strings.Contains(entrypoint, "cmp -s /keys/") {
		t.Errorf("%s decides whether to regenerate without comparing the public half against authorized_keys; a stale pair from an earlier container would then be kept and refused", sftpEntrypointRel)
	}
}

func TestSftpFixturePortsMatchComposeDefaults(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, sftpComposeRel)

	// Every `${SFTP_FIXTURE_<SERVICE>_PORT:-<default>}` the compose file
	// declares, by service suffix.
	composePorts := map[string]int{}
	for _, m := range composeDefaultRE.FindAllStringSubmatch(compose, -1) {
		suffix, ok := trimPortEnv(m[1])
		if !ok {
			continue
		}
		port, err := strconv.Atoi(m[2])
		if err != nil {
			t.Errorf("%s: ${%s:-%s} has a non-numeric default", sftpComposeRel, m[1], m[2])
			continue
		}
		composePorts[suffix] = port
	}

	for service, want := range sftpServiceHostPorts {
		got, ok := composePorts[service]
		if !ok {
			t.Errorf("sftpServiceHostPorts has %s (%d) but %s declares no ${SFTP_FIXTURE_%s_PORT:-…} default; a bare start.sh would land the service somewhere the suite never looks", service, want, sftpComposeRel, service)
			continue
		}
		if got != want {
			t.Errorf("%s defaults SFTP_FIXTURE_%s_PORT to %d; sftpServiceHostPorts pins %d. A bare start.sh and a check run would then serve the same fixture on two ports", sftpComposeRel, service, got, want)
		}
	}

	// ❗ The bench server is deliberately outside the table: the integration lane
	// waits on every service in it, and a throughput measurement must never gate
	// CI. Any OTHER extra means a service was added to compose and never wired
	// into the port table or the lane's wait guard.
	extras := map[string]bool{}
	for service := range composePorts {
		if _, inTable := sftpServiceHostPorts[service]; !inTable {
			extras[service] = true
		}
	}
	if !sameStringSet(extras, map[string]bool{"BENCH": true}) {
		t.Errorf("%s declares port defaults for %s that sftpServiceHostPorts doesn't carry (only BENCH may be absent, and only because CI must never gate on it). See the fixture README's \"Adding a server\"",
			sftpComposeRel, sortedKeys(extras))
	}
}

// assertSingleDefault reads the one `${name:-default}` a shell script declares
// for name, and fails unless it equals want.
func assertSingleDefault(t *testing.T, rel, body, name, want string) {
	t.Helper()
	found := regexp.MustCompile(`\$\{`+regexp.QuoteMeta(name)+`:-([^}]*)\}`).FindAllStringSubmatch(body, -1)
	if len(found) == 0 {
		t.Errorf("%s declares no ${%s:-…} default, so a bare run and a check run can disagree about where the keys land", rel, name)
		return
	}
	for _, m := range found {
		if m[1] != want {
			t.Errorf("%s defaults %s to %s; stacklease says %s", rel, name, m[1], want)
		}
	}
}

func trimPortEnv(name string) (string, bool) {
	const prefix, suffix = "SFTP_FIXTURE_", "_PORT"
	if len(name) <= len(prefix)+len(suffix) {
		return "", false
	}
	if name[:len(prefix)] != prefix || name[len(name)-len(suffix):] != suffix {
		return "", false
	}
	return name[len(prefix) : len(name)-len(suffix)], true
}

func readRepoFile(t *testing.T, root, rel string) string {
	t.Helper()
	body, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(rel)))
	if err != nil {
		t.Fatalf("read %s: %v", rel, err)
	}
	return string(body)
}

func sameStringSet(a, b map[string]bool) bool {
	if len(a) != len(b) {
		return false
	}
	for k := range a {
		if !b[k] {
			return false
		}
	}
	return true
}

// sftpComposePortsRE captures one `ports: ['…']` value.
var sftpComposePortsRE = regexp.MustCompile(`ports:\s*\['([^']*)'\]`)

func TestSftpFixturePortsBindToLoopback(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, sftpComposeRel)

	const wantPrefix = "${" + sftpBindAddrEnv + ":-127.0.0.1}:"

	matches := sftpComposePortsRE.FindAllStringSubmatch(compose, -1)
	if len(matches) != len(sftpServiceHostPorts)+1 {
		t.Fatalf("%s declares %d `ports:` entries; %d services are in sftpServiceHostPorts plus the bench server. A service publishing no port, or a second publish on one, means this guard is reading the wrong set", sftpComposeRel, len(matches), len(sftpServiceHostPorts))
	}

	for _, m := range matches {
		if !strings.HasPrefix(m[1], wantPrefix) {
			t.Errorf("%s publishes `%s` with no %q prefix, so Docker binds it on 0.0.0.0 and puts an sshd whose credentials this repo documents in public on the LAN and the tailnet of whoever runs the suite", sftpComposeRel, m[1], wantPrefix)
		}
	}
}
