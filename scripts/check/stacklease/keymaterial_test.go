// Tests for the two staleness signals nothing else in the lease can see: key
// material that vanished from the host side of a bind mount, and a first-party
// image edited after the containers that run it came up.
//
// Both share a shape: the stack looks completely healthy (running, healthy,
// config hash matching) while serving something wrong, and the only symptom is a
// suite failing somewhere that reads as a product bug.

package stacklease

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// withKeyMaterialDeadline shortens the wait for a test and returns the restore.
func withKeyMaterialDeadline(d time.Duration) func() {
	prev := keyMaterialDeadline
	keyMaterialDeadline = d
	return func() { keyMaterialDeadline = prev }
}

// ---- key material that vanished from the host ----
//
// The gap these cover: `/keys` is a bind SOURCE under /tmp, which macOS empties
// on reboot, while the containers come back (`restart: unless-stopped`) holding
// the `authorized_keys` they wrote before it. Every other signal the lease reads
// still says "fine" — running, healthy, config hash matching — and the only
// symptom is four key-auth cells failing an auth rung.

// seedKeyMaterial writes a stand-in private key into every leaf the stack binds,
// which is what a container's entrypoint does at start.
func seedKeyMaterial(t *testing.T, s *Stack) {
	t.Helper()
	for _, leaf := range s.KeysLeaves() {
		path := s.KeyMaterialPath(leaf)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("seed %s: %v", path, err)
		}
		if err := os.WriteFile(path, []byte("PRIVATE KEY"), 0o644); err != nil {
			t.Fatalf("seed %s: %v", path, err)
		}
	}
}

// serveEverything puts the stack in the state that makes Acquire adopt: every
// mode service running and healthy, with this session's config hash stamped.
func serveEverything(t *testing.T, s *Stack, f *fakeComposer, mode string) {
	t.Helper()
	for _, svc := range s.modeServicesFor(mode) {
		f.running[svc] = true
		f.healthy[svc] = true
	}
	s.writeConfigHash(mode)
}

func TestAcquireRepublishesKeyMaterialThatVanishedFromTheHost(t *testing.T) {
	fakes := withFakes(t)
	f := fakes.forStack(SFTP)
	if err := SFTP.EnsureKeysDir(); err != nil {
		t.Fatalf("EnsureKeysDir: %v", err)
	}
	serveEverything(t, SFTP, f, ModeCore)
	// The leaves exist and are EMPTY, which is exactly what a wiped /tmp plus a
	// `mkdir -p` leaves behind.
	f.restartWrites = func(services []string) {
		for _, svc := range services {
			for _, leaf := range SFTP.KeysLeaves() {
				if leaf.Service == svc {
					_ = os.WriteFile(SFTP.KeyMaterialPath(leaf), []byte("PRIVATE KEY"), 0o644)
				}
			}
		}
	}

	res, err := SFTP.Acquire("manual", ModeCore)
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionAdopt {
		t.Fatalf("Acquire took %s; a fully serving stack with a matching hash still adopts, the healing is separate", res.Action)
	}
	want := []string{"sftp-fixture-keyonly", "sftp-fixture-passphrase"}
	if len(f.restartCalls) != 1 || !equalStrings(f.restartCalls[0], want) {
		t.Fatalf("Acquire issued restarts %v; want exactly one restart of %v, so the containers regenerate the pair their authorized_keys names", f.restartCalls, want)
	}
	for _, leaf := range SFTP.KeysLeaves() {
		if _, err := os.Stat(SFTP.KeyMaterialPath(leaf)); err != nil {
			t.Fatalf("Acquire returned with %s still unpublished (%v); the suite would race the regeneration", SFTP.KeyMaterialPath(leaf), err)
		}
	}
}

func TestAcquireLeavesAPopulatedKeysDirAlone(t *testing.T) {
	fakes := withFakes(t)
	f := fakes.forStack(SFTP)
	if err := SFTP.EnsureKeysDir(); err != nil {
		t.Fatalf("EnsureKeysDir: %v", err)
	}
	serveEverything(t, SFTP, f, ModeCore)
	seedKeyMaterial(t, SFTP)

	if _, err := SFTP.Acquire("manual", ModeCore); err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if len(f.restartCalls) != 0 {
		t.Fatalf("Acquire restarted %v with the key material right where it belongs; a healthy adopt must issue no compose call at all", f.restartCalls)
	}
}

func TestAcquireFailsLoudlyWhenARestartDoesNotRepublishTheKey(t *testing.T) {
	fakes := withFakes(t)
	f := fakes.forStack(SFTP)
	if err := SFTP.EnsureKeysDir(); err != nil {
		t.Fatalf("EnsureKeysDir: %v", err)
	}
	serveEverything(t, SFTP, f, ModeCore)
	// No `restartWrites`: the restart happens and the leaf stays empty.
	t.Cleanup(withKeyMaterialDeadline(200 * time.Millisecond))

	_, err := SFTP.Acquire("manual", ModeCore)
	if err == nil {
		t.Fatal("Acquire succeeded with no key material published; four key-auth cells would then fail an auth rung and read as a backend bug")
	}
	if !strings.Contains(err.Error(), "sftp-fixture-keyonly") {
		t.Fatalf("Acquire failed with %q; the message has to name the service that never republished", err)
	}
}

func TestAStackThatMountsNothingNeverLooksForKeyMaterial(t *testing.T) {
	fakes := withFakes(t)
	f := fakes.forStack(SMB)
	serveEverything(t, SMB, f, ModeCore)

	if _, err := SMB.Acquire("manual", ModeCore); err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if len(f.restartCalls) != 0 {
		t.Fatalf("the SMB stack issued restarts %v; it bind-mounts nothing, so it has no key material to miss", f.restartCalls)
	}
}

func TestKeysLeavesNameTheServiceThatFillsThem(t *testing.T) {
	// ❗ The pairing is what makes healing surgical: one leaf gone restarts one
	// container, not the whole stack under whoever else is running against it.
	leaves := SFTP.KeysLeaves()
	if len(leaves) == 0 {
		t.Fatal("the SFTP stack lists no keys leaves")
	}
	services := map[string]bool{}
	for _, svc := range SFTP.modeServicesFor(ModeCore) {
		services[svc] = true
	}
	for _, leaf := range leaves {
		if leaf.Dir == "" || leaf.Service == "" {
			t.Fatalf("keys leaf %+v is half-declared", leaf)
		}
		if !services[leaf.Service] {
			t.Errorf("keys leaf %q names the service %q, which core mode never brings up; a restart would then target nothing", leaf.Dir, leaf.Service)
		}
	}
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestAnEditedFirstPartyImageReadsAsAStaleStack(t *testing.T) {
	// ❗ The staleness nothing else can see: `up -d` never rebuilds and never
	// recreates a healthy container, so a stack brought up before an entrypoint
	// edit keeps serving the old one across reboots. Folding the build context
	// into the hash is what turns that edit into a reconcile.
	withFakes(t)
	dir := t.TempDir()
	t.Setenv(SFTP.composeDirEnv, dir)
	ctx := filepath.Join(dir, "image")
	if err := os.MkdirAll(ctx, 0o755); err != nil {
		t.Fatal(err)
	}
	entrypoint := filepath.Join(ctx, "entrypoint.sh")
	if err := os.WriteFile(entrypoint, []byte("#!/bin/sh\nexec sshd -D\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	before := SFTP.computeConfigHash(ModeCore)
	if err := os.WriteFile(entrypoint, []byte("#!/bin/sh\nprovision_client_key\nexec sshd -D\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if after := SFTP.computeConfigHash(ModeCore); after == before {
		t.Fatal("editing the fixture image's entrypoint left the config hash unchanged, so a running stack would keep serving the old image forever")
	}
}

func TestOnlyAFirstPartyImageIsRebuiltOnUp(t *testing.T) {
	// SFTP's Dockerfile lives in this repo and is edited here; SMB's images come
	// from a vendored harness, where a rebuild is somebody else's call.
	if len(SFTP.buildContextsRel) == 0 {
		t.Error("the SFTP stack declares no build context, so an edited entrypoint would never reach a running container")
	}
	if len(WEBDAV.buildContextsRel) != 2 {
		t.Errorf("the WebDAV stack declares %v; it builds two first-party images (httpd and Nextcloud), and a context left out is an edit that never reaches a container", WEBDAV.buildContextsRel)
	}
	if len(SMB.buildContextsRel) != 0 {
		t.Errorf("the SMB stack declares build contexts (%q); its images are vendored", SMB.buildContextsRel)
	}
	if got := (dockerComposer{stack: SFTP}).upArgs(nil, []string{"-f", "x.yml"}); !containsString(got, "--build") {
		t.Errorf("`up` for the SFTP stack is %v, with no --build; the image would never be rebuilt", got)
	}
	if got := (dockerComposer{stack: SMB}).upArgs(nil, []string{"-f", "x.yml"}); containsString(got, "--build") {
		t.Errorf("`up` for the SMB stack is %v; it must not rebuild a vendored image", got)
	}
}

func containsString(haystack []string, needle string) bool {
	for _, s := range haystack {
		if s == needle {
			return true
		}
	}
	return false
}
