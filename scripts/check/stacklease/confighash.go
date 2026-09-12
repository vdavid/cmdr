package stacklease

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// ---- config-hash ----
//
// The hash captures the merged compose inputs so a later adopter can tell
// whether the running stack matches this session's config. We hash the stack's
// compose files, the resolved service set, and the stack's port env. We stamp it
// to a file next to the lock on `up` (writeConfigHash) and compare at adopt time
// (configHashMatches) — simpler and more reliable than round-tripping a compose
// label.

func (s *Stack) configHashPath() string {
	return s.LockPath() + ".confighash"
}

// repoRoot is the in-process override for compose-dir resolution. The check
// runner sets it once so every stack resolves against the repo it's checking,
// independent of the orchestrator's cwd.
var repoRoot string

// SetRepoRoot points compose-dir resolution at a known repo root.
func SetRepoRoot(dir string) { repoRoot = dir }

// composeDir resolves this stack's compose directory: the stack's own env
// override first, then the repo root the runner set, then a best-effort walk up
// from cwd. Returns "" when none of them find it, which `Up` reports rather than
// falling back to docker's default file lookup.
func (s *Stack) composeDir() string {
	if d := os.Getenv(s.composeDirEnv); d != "" {
		return d
	}
	rel := filepath.FromSlash(s.composeDirRel)
	if repoRoot != "" {
		if candidate := filepath.Join(repoRoot, rel); isDir(candidate) {
			return candidate
		}
	}
	wd, err := os.Getwd()
	if err != nil {
		return ""
	}
	for dir := wd; ; {
		if candidate := filepath.Join(dir, rel); isDir(candidate) {
			return candidate
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}
		dir = parent
	}
}

func isDir(path string) bool {
	st, err := os.Stat(path)
	if err != nil || st == nil {
		return false
	}
	return st.IsDir()
}

func (s *Stack) computeConfigHash(mode string) string {
	h := sha256.New()
	cd := s.composeDir()
	for _, f := range s.composeFiles {
		if cd != "" {
			if b, err := os.ReadFile(filepath.Join(cd, f)); err == nil {
				h.Write(b)
			}
		}
	}
	fmt.Fprintf(h, "stack=%s\nmode=%s\n", s.Name, mode)
	for _, svc := range s.modeServicesFor(mode) {
		fmt.Fprintf(h, "svc=%s\n", svc)
	}
	// Port env: the one config dimension that genuinely changes container
	// bindings across worktrees/sessions.
	var ports []string
	for _, kv := range os.Environ() {
		if strings.HasPrefix(kv, s.portEnvPrefix) && strings.Contains(kv, "_PORT=") {
			ports = append(ports, kv)
		}
	}
	sort.Strings(ports)
	for _, kv := range ports {
		fmt.Fprintf(h, "%s\n", kv)
	}
	// A first-party image: its build context decides what the containers RUN, so
	// an edited entrypoint has to read as staleness the same way an edited
	// compose file does. Nothing else would notice.
	//
	// ❗ The context's own name goes into the hash beside each file's, so two
	// contexts holding a same-named file (`Dockerfile`, say) can't cancel each
	// other out and leave an edit invisible.
	for _, ctx := range s.BuildContextDirs() {
		var files []string
		_ = filepath.WalkDir(ctx, func(path string, d fs.DirEntry, err error) error {
			if err != nil || d.IsDir() {
				return nil //nolint:nilerr // an unreadable entry just doesn't contribute
			}
			files = append(files, path)
			return nil
		})
		sort.Strings(files)
		for _, f := range files {
			rel, _ := filepath.Rel(ctx, f)
			fmt.Fprintf(h, "build=%s/%s\n", filepath.Base(ctx), filepath.ToSlash(rel))
			if b, err := os.ReadFile(f); err == nil {
				h.Write(b)
			}
		}
	}
	// The keys dir is a bind SOURCE, so a running stack that mounts a different
	// one is exactly as stale as one bound to different ports — and far quieter
	// about it, since the containers stay healthy while every key-auth cell
	// fails.
	if keys := s.KeysDir(); keys != "" {
		fmt.Fprintf(h, "keys=%s\n", keys)
	}
	return hex.EncodeToString(h.Sum(nil))
}

func (s *Stack) writeConfigHash(mode string) {
	if err := os.WriteFile(s.configHashPath(), []byte(s.computeConfigHash(mode)), 0o644); err != nil {
		Logf("WARN: could not stamp the %s config hash (%v); future adopters will treat config as mismatched", s.Name, err)
	}
}

// configHashMatches reports whether the stamped hash equals this session's
// computed hash. A missing stamp means "unknown" → treat as mismatch so the
// caller errs toward reconcile-when-safe / adopt-and-warn-under-foreign-lease.
func (s *Stack) configHashMatches(mode string) bool {
	stamped, err := os.ReadFile(s.configHashPath())
	if err != nil {
		return false
	}
	return strings.TrimSpace(string(stamped)) == s.computeConfigHash(mode)
}

func workingDir() string {
	wd, err := os.Getwd()
	if err != nil {
		return "?"
	}
	return wd
}
