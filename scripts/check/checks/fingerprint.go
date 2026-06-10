package checks

import (
	"crypto/sha1"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// Fingerprinting answers one question per check: "did anything this check reads
// change since it last passed?" It does so content-addressably and git-aware, so
// the whole pass stays well under a second and never walks node_modules/ or
// target/.
//
// The mechanism, in one repo-wide pass shared by every check:
//   - `git ls-files -s` gives the index blob SHA of every tracked file straight
//     from the index — no file reads, no working-tree stat storm.
//   - `git status --porcelain` lists the few files whose working-tree content
//     diverges from the index (modified, staged-but-restaged, untracked) plus
//     deletions. Those are the only files whose index SHA is stale, so only they
//     are hashed from disk; deletions drop the path from the set entirely.
//
// Per check, we filter that repo-wide picture to the check's input globs (its
// Inputs plus the GlobalInputs), sort the surviving path→content-hash pairs, and
// hash the whole sorted list. Including the sorted file list in the hash means an
// add or a remove changes the fingerprint even if no surviving file's content
// did.

// RepoFingerprintData is the result of one repo-wide git scan, reused across all
// checks in a run so git forks twice total, not 2×(number of checks).
type RepoFingerprintData struct {
	rootDir string
	// indexBlobs maps a repo-relative path to its index blob SHA (tracked files).
	indexBlobs map[string]string
	// dirtyContentHashes maps a repo-relative path to a content hash computed from
	// the working tree, for files whose working-tree content differs from the
	// index (modified/untracked). Overrides indexBlobs for the same path.
	dirtyContentHashes map[string]string
	// deleted is the set of repo-relative paths removed from the working tree but
	// still tracked in the index; they must drop out of every fingerprint.
	deleted map[string]bool
}

// CollectRepoFingerprintData runs the two git commands once and assembles the
// shared picture. A non-git tree, or git failing for any reason, returns an error
// so the caller can degrade to "run everything" (never an error to the user).
func CollectRepoFingerprintData(rootDir string) (*RepoFingerprintData, error) {
	data := &RepoFingerprintData{
		rootDir:            rootDir,
		indexBlobs:         map[string]string{},
		dirtyContentHashes: map[string]string{},
		deleted:            map[string]bool{},
	}

	lsOut, err := runGit(rootDir, "ls-files", "-s")
	if err != nil {
		return nil, fmt.Errorf("git ls-files failed: %w", err)
	}
	parseLsFiles(lsOut, data.indexBlobs)

	statusOut, err := runGit(rootDir, "status", "--porcelain", "-z", "--untracked-files=all")
	if err != nil {
		return nil, fmt.Errorf("git status failed: %w", err)
	}
	if err := parseStatusAndHashDirty(rootDir, statusOut, data); err != nil {
		return nil, err
	}

	return data, nil
}

// runGit runs a git subcommand at rootDir and returns its stdout.
func runGit(rootDir string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = rootDir
	return RunCommand(cmd, true)
}

// parseLsFiles fills paths from `git ls-files -s` output. Each line is
// "<mode> <sha> <stage>\t<path>". Reading the SHA straight from the index is the
// whole point: no file content is touched for clean tracked files.
func parseLsFiles(out string, into map[string]string) {
	for _, line := range strings.Split(out, "\n") {
		if line == "" {
			continue
		}
		tab := strings.IndexByte(line, '\t')
		if tab < 0 {
			continue
		}
		meta := strings.Fields(line[:tab])
		if len(meta) < 2 {
			continue
		}
		into[line[tab+1:]] = meta[1] // meta[1] is the blob SHA
	}
}

// parseStatusAndHashDirty parses `git status --porcelain -z` and, for every
// modified/untracked file, hashes its working-tree content so the fingerprint
// reflects unstaged edits. Deletions are recorded so they drop from fingerprints.
// The -z format is NUL-separated, with a second NUL-delimited field for renames.
func parseStatusAndHashDirty(rootDir, out string, data *RepoFingerprintData) error {
	fields := strings.Split(out, "\x00")
	for i := 0; i < len(fields); i++ {
		entry := fields[i]
		if len(entry) < 4 {
			continue
		}
		x, y := entry[0], entry[1]
		path := entry[3:]
		// Rename/copy entries carry the source path as the next NUL field; skip it.
		if x == 'R' || x == 'C' {
			i++
		}
		switch {
		case x == 'D' || y == 'D':
			data.deleted[path] = true
		default:
			// Modified (M), untracked (??), added (A), renamed-dest, etc.: the
			// working-tree content is what matters, so hash it from disk. A file
			// that vanished between status and here is treated as deleted.
			h, err := hashFileContent(filepath.Join(rootDir, path))
			if err != nil {
				if os.IsNotExist(err) {
					data.deleted[path] = true
					continue
				}
				return fmt.Errorf("hashing %s: %w", path, err)
			}
			data.dirtyContentHashes[path] = h
		}
	}
	return nil
}

// hashFileContent returns the file's git blob object id (SHA-1 of
// "blob <len>\x00<content>", exactly like `git hash-object`). Using git's own
// algorithm — not a generic content hash — matters: a clean tracked file
// contributes its index blob SHA to the fingerprint, so a dirty-REPORTED file
// with identical content must hash to the same value. Git can transiently report
// unchanged files right after a merge/checkout (stale stat cache, "racily clean"
// entries); a mismatched hash algorithm flips the fingerprint and triggers a
// spurious lane-wide re-run that self-heals only on the next pass. SHA-1 is fine
// here: this is content identity for caching, not a security boundary, and it
// must mirror what the git index stores.
func hashFileContent(absPath string) (string, error) {
	b, err := os.ReadFile(absPath)
	if err != nil {
		return "", err
	}
	hasher := sha1.New()
	fmt.Fprintf(hasher, "blob %d\x00", len(b))
	hasher.Write(b)
	return hex.EncodeToString(hasher.Sum(nil)), nil
}

// FingerprintFor computes the fingerprint of a single check's input set against
// the shared repo data. The input set is the check's Inputs plus GlobalInputs.
// The fingerprint is stable for an identical tree and changes on any content,
// add, or remove within the input set.
func (data *RepoFingerprintData) FingerprintFor(def *CheckDefinition) string {
	patterns := make([]string, 0, len(def.Inputs)+len(GlobalInputs))
	patterns = append(patterns, def.Inputs...)
	patterns = append(patterns, GlobalInputs...)

	// Gather path→hash for every input file: clean tracked files use the index
	// blob SHA; dirty/untracked files override with their working-tree hash;
	// deleted files are excluded.
	matched := map[string]string{}
	for path, sha := range data.indexBlobs {
		if data.deleted[path] {
			continue
		}
		if matchesAny(path, patterns) {
			matched[path] = sha
		}
	}
	for path, h := range data.dirtyContentHashes {
		if matchesAny(path, patterns) {
			matched[path] = h
		}
	}

	paths := make([]string, 0, len(matched))
	for p := range matched {
		paths = append(paths, p)
	}
	sort.Strings(paths)

	hasher := sha256.New()
	for _, p := range paths {
		// Include the path itself so adds/removes shift the hash, and a stable
		// delimiter so "a\nb" can't collide with "a\0b"-style ambiguity.
		fmt.Fprintf(hasher, "%s\x00%s\x00", p, matched[p])
	}
	return hex.EncodeToString(hasher.Sum(nil))
}

// matchesAny reports whether path matches at least one glob pattern.
func matchesAny(path string, patterns []string) bool {
	for _, pat := range patterns {
		if matchGlob(pat, path) {
			return true
		}
	}
	return false
}

// matchGlob matches a repo-relative path against a git-pathspec-style glob.
// Supports exact paths, `prefix/**` (everything under prefix), and `**` /
// single-`*` segment wildcards via segment-wise matching. `**` matches any number
// of path segments; `*`, `?`, and `[...]` match within a single segment
// (filepath.Match semantics). This is deliberately a small, predictable subset:
// the Inputs lists only use exact paths and `prefix/**`.
func matchGlob(pattern, path string) bool {
	if pattern == path {
		return true
	}
	if suffix, ok := strings.CutSuffix(pattern, "/**"); ok {
		// `dir/**` matches the dir's descendants (but not the bare dir itself).
		return strings.HasPrefix(path, suffix+"/")
	}
	return matchSegments(strings.Split(pattern, "/"), strings.Split(path, "/"))
}

// matchSegments matches path segments against pattern segments, treating a `**`
// pattern segment as "zero or more segments".
func matchSegments(pat, name []string) bool {
	if len(pat) == 0 {
		return len(name) == 0
	}
	if pat[0] == "**" {
		// Try consuming 0..n leading name segments.
		for i := 0; i <= len(name); i++ {
			if matchSegments(pat[1:], name[i:]) {
				return true
			}
		}
		return false
	}
	if len(name) == 0 {
		return false
	}
	ok, err := filepath.Match(pat[0], name[0])
	if err != nil || !ok {
		return false
	}
	return matchSegments(pat[1:], name[1:])
}
