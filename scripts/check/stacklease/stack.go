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
	// keysSubdirs the per-service leaves under it and keysDirEnv the var that
	// overrides the whole path. All three are empty on a stack that mounts
	// nothing from the host, where KeysDir answers "".
	//
	// ❗ Machine-wide, beside the lock and the lease dir, for exactly the reason
	// they are: the stack is machine-wide and a sibling worktree ADOPTS a
	// running one rather than recreating it. A per-checkout bind source bakes
	// the STARTING worktree's absolute path into a live container, so deleting
	// that worktree takes key auth down for every worktree at once, and the
	// reader (which resolves against its OWN checkout) can't even see that the
	// two scopes disagree.
	keysDirName string
	keysSubdirs []string
	keysDirEnv  string

	// composeDirEnv is the env var that overrides compose-dir resolution;
	// composeDirRel is the slash-separated repo-relative fallback.
	composeDirRel string
	composeDirEnv string
	// composeFiles are the files `up` layers, in order. SMB layers a vendored
	// compose under a cmdr-owned override; a first-party stack needs one file.
	composeFiles []string

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

// KeysSubdirs are the per-service leaves under KeysDir that the compose file
// binds, in registry order.
func (s *Stack) KeysSubdirs() []string { return s.keysSubdirs }

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
	for _, leaf := range s.keysSubdirs {
		if err := os.MkdirAll(filepath.Join(dir, leaf), 0o755); err != nil {
			return fmt.Errorf("create %s keys dir %s: %w", s.Name, filepath.Join(dir, leaf), err)
		}
	}
	return os.Setenv(s.keysDirEnv, dir)
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
