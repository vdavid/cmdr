// The Stack VALUE: what a fixture stack is made of, and where its machine-wide
// host state lives. The policy that acts on it (acquire, adopt-or-reconcile, the
// dead-PID sweep, down-at-zero) is `stacklease.go`, which is also the package
// doc; the concrete stacks are `registry.go`.

package stacklease

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
)

// leaseRootEnv points the lock file and lease dir at a sandbox. Tests set it to
// isolate from the real machine-wide paths; production callers leave it unset
// and get the fixed /tmp paths that all worktrees share. `/tmp` (not `$TMPDIR`)
// deliberately: we want one shared namespace across all of a user's worktrees,
// not a per-shell one.
const leaseRootEnv = "CMDR_FIXTURE_LEASE_ROOT"

// KeysLeaf is one per-service leaf under a stack's KeysDir: the directory the
// compose file binds into that container's key mount, and the service whose
// entrypoint generates the pair into it.
//
// ❗ Pairing the two here, rather than keeping a list of directories beside a
// list of services, is what lets a leaf that lost its key material name the ONE
// container to restart: healing is surgical instead of a stack-wide bounce under
// whoever else is running against it.
type KeysLeaf struct {
	// Dir is the leaf's basename under KeysDir, and the compose bind source.
	Dir string
	// Service is the compose service that fills it at start.
	Service string
}

// Stack is one Docker Compose fixture stack: which containers it holds, where
// its compose files live, and the lock + lease namespace that keeps its users
// refcounted. Fields are unexported so the registry is the only source of
// stacks; `Lookup` and `All` read it.
type Stack struct {
	// Name is the stack's short id ("smb", "sftp"). It's the CLI's first
	// argument and the token every log line carries.
	Name string
	// ProjectName is the Docker Compose project (`-p`) all callers share.
	ProjectName string

	// lockFile and leaseDirName are the /tmp basenames. Held per stack rather
	// than derived from Name because a sibling worktree on older code holds its
	// lease at the exact historical path; moving one would hide a live holder
	// and re-open the teardown race.
	lockFile     string
	leaseDirName string

	// keysDirName is the third machine-wide /tmp basename: the host directory a
	// stack's containers bind-mount generated key material into, with
	// keysLeaves the per-service leaves under it, keysFileName the file each
	// container publishes there, and keysDirEnv the var that overrides the whole
	// path. All of them are empty on a stack that mounts nothing from the host,
	// where KeysDir answers "".
	//
	// ❗ Machine-wide, beside the lock and the lease dir, for exactly the reason
	// they are: the stack is machine-wide and a sibling worktree ADOPTS a
	// running one rather than recreating it. A per-checkout bind source bakes
	// the STARTING worktree's absolute path into a live container, so deleting
	// that worktree takes key auth down for every worktree at once, and the
	// reader (which resolves against its OWN checkout) can't even see that the
	// two scopes disagree.
	keysDirName  string
	keysLeaves   []KeysLeaf
	keysFileName string
	keysDirEnv   string

	// composeDirEnv is the env var that overrides compose-dir resolution;
	// composeDirRel is the slash-separated repo-relative fallback.
	composeDirRel string
	composeDirEnv string
	// composeFiles are the files `up` layers, in order. SMB layers a vendored
	// compose under a cmdr-owned override; a first-party stack needs one file.
	composeFiles []string
	// buildContextsRel are the stack's image build contexts, relative to the
	// compose dir, for a stack whose Dockerfiles are FIRST-PARTY and edited in
	// this repo. Declaring them does two things: every context's contents fold
	// into the config hash, and `up` passes `--build`.
	//
	// ❗ Without both, an edit to an image is invisible. `up -d` never rebuilds
	// on its own and never recreates a healthy container, so a stack brought up
	// before the edit keeps serving the old entrypoint for as long as it lives —
	// which is across reboots. Empty for a stack whose images are vendored, where
	// a rebuild is somebody else's call.
	//
	// A slice because one stack can serve two unrelated servers: WebDAV builds
	// an httpd image and a Nextcloud one, and an edit to either has to read as
	// staleness.
	buildContextsRel []string

	// modeServices maps a mode to the exact service set the stack's start.sh
	// brings up for it, and must stay in lock-step with that script. A nil
	// value means "every service the project defines"; the caller resolves the
	// concrete set at runtime.
	modeServices map[string][]string
	// servicesWithoutHealthcheck are the services that intentionally ship no
	// HEALTHCHECK, so adoption must NOT require them healthy — only running.
	servicesWithoutHealthcheck map[string]bool

	// portEnvPrefix selects the env vars that change container port bindings,
	// which is the one config dimension that genuinely differs across
	// worktrees and sessions. They fold into the config hash.
	portEnvPrefix string
}

// LockPath is the flock target: a stable inode held for the full
// acquire-or-release critical section. Separate from the lease dir so flock
// targets a fixed file.
func (s *Stack) LockPath() string {
	return filepath.Join(leaseRoot(), s.lockFile)
}

// LeaseDir holds one file per live holder of this stack. World-traversable,
// predictable, survives across worktrees.
func (s *Stack) LeaseDir() string {
	return filepath.Join(leaseRoot(), s.leaseDirName)
}

// KeysDir is the machine-wide host directory this stack's containers bind-mount
// their generated key material into, one leaf per service. "" when the stack
// mounts nothing from the host.
func (s *Stack) KeysDir() string {
	if d := os.Getenv(s.keysDirEnv); d != "" {
		return d
	}
	if s.keysDirName == "" {
		return ""
	}
	return filepath.Join(leaseRoot(), s.keysDirName)
}

// KeysDirEnv names the env var the compose file and the Rust fixtures read the
// keys dir from, so all three land on one path. "" when the stack has no keys
// dir.
func (s *Stack) KeysDirEnv() string { return s.keysDirEnv }

// KeysLeaves are the per-service leaves under KeysDir that the compose file
// binds, in registry order.
func (s *Stack) KeysLeaves() []KeysLeaf { return s.keysLeaves }

// KeyMaterialPath is where the host reads back the private key a leaf's
// container generated. It has to equal what the suite opens
// (`cmdr_sftp::volume::testing::fixture_key_path`); `TestSftpFixturePathsAgree`
// is what keeps the two equal.
func (s *Stack) KeyMaterialPath(leaf KeysLeaf) string {
	return filepath.Join(s.KeysDir(), leaf.Dir, s.keysFileName)
}

// KeysFileName is the private-key basename each container publishes into its
// leaf. "" on a stack that mounts nothing.
func (s *Stack) KeysFileName() string { return s.keysFileName }

// servicesMissingKeyMaterial names the requested services whose leaf holds no
// private key, in registry order.
//
// ❗ The one staleness the lease can't see any other way. `compose ps` reports
// running and healthy, the config hash still matches, and the containers are
// genuinely fine: what's gone is the HOST half of a bind mount under /tmp, which
// macOS empties on reboot while the containers come back holding the
// `authorized_keys` they wrote before it. The only other symptom is an auth rung
// refusing, which reads as a backend bug.
func (s *Stack) servicesMissingKeyMaterial(requested []string) []string {
	if s.KeysDir() == "" {
		return nil
	}
	wanted := make(map[string]bool, len(requested))
	for _, svc := range requested {
		wanted[svc] = true
	}
	var missing []string
	for _, leaf := range s.keysLeaves {
		if !wanted[leaf.Service] {
			continue
		}
		if _, err := os.Stat(s.KeyMaterialPath(leaf)); err != nil {
			missing = append(missing, leaf.Service)
		}
	}
	return missing
}

// EnsureKeysDir creates the keys dir and its leaves, then pins the resolved path
// in this process's environment so compose — which reads
// `${<KeysDirEnv>:-<default>}` — binds what this process resolved rather than
// its own default. No-op for a stack without one.
//
// ❗ Created here rather than left to Docker: Docker auto-creates a missing bind
// source ROOT-owned on Linux, and the container's own write into it then fails.
func (s *Stack) EnsureKeysDir() error {
	dir := s.KeysDir()
	if dir == "" {
		return nil
	}
	for _, leaf := range s.keysLeaves {
		if err := os.MkdirAll(filepath.Join(dir, leaf.Dir), 0o755); err != nil {
			return fmt.Errorf("create %s keys dir %s: %w", s.Name, filepath.Join(dir, leaf.Dir), err)
		}
	}
	return os.Setenv(s.keysDirEnv, dir)
}

// BuildContextDirs resolves this stack's first-party image build contexts, in
// declaration order. Empty when it declares none or the compose dir can't be
// found.
func (s *Stack) BuildContextDirs() []string {
	if len(s.buildContextsRel) == 0 {
		return nil
	}
	cd := s.composeDir()
	if cd == "" {
		return nil
	}
	dirs := make([]string, 0, len(s.buildContextsRel))
	for _, rel := range s.buildContextsRel {
		dirs = append(dirs, filepath.Join(cd, filepath.FromSlash(rel)))
	}
	return dirs
}

// ServicesForMode is the exact service set a mode brings up, or nil for a mode
// that means "every service the project defines". The check runner reads it so
// a lane waits on the containers its own mode started and no others. A copy,
// because the registry is the only source of stacks and a caller that sorted
// what it got back would edit it.
func (s *Stack) ServicesForMode(mode string) []string {
	services := s.modeServices[mode]
	if services == nil {
		return nil
	}
	return append([]string(nil), services...)
}

// Modes lists the modes this stack serves, sorted, for error messages and the
// CLI's usage line.
func (s *Stack) Modes() []string {
	modes := make([]string, 0, len(s.modeServices))
	for m := range s.modeServices {
		modes = append(modes, m)
	}
	sort.Strings(modes)
	return modes
}

func leaseRoot() string {
	if root := os.Getenv(leaseRootEnv); root != "" {
		return root
	}
	return "/tmp"
}
