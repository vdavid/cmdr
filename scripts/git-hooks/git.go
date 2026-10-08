package main

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// repo is the worktree a hook runs in.
type repo struct {
	// root is the absolute path of the worktree's top level.
	root string
}

// git runs a git command in the worktree and returns its stdout.
//
// `--literal-pathspecs` is always on: every path a hook passes names one real file,
// and the repo has route files like `[slug].astro` that git would otherwise read as
// a glob.
func (r repo) git(args ...string) (string, error) {
	return r.gitWithStdin("", args...)
}

func (r repo) gitWithStdin(stdin string, args ...string) (string, error) {
	cmd := exec.Command("git", append([]string{"--literal-pathspecs"}, args...)...)
	cmd.Dir = r.root
	if stdin != "" {
		cmd.Stdin = strings.NewReader(stdin)
	}
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return stdout.String(), fmt.Errorf("git %s: %w: %s", strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return stdout.String(), nil
}

// paths runs a git command that prints NUL-separated paths (`-z`) and returns them.
func (r repo) paths(args ...string) ([]string, error) {
	out, err := r.git(args...)
	if err != nil {
		return nil, err
	}
	return splitNul(out), nil
}

// hasCommit reports whether this clone has the commit a sha names.
func (r repo) hasCommit(sha string) bool {
	_, err := r.git("cat-file", "-e", sha+"^{commit}")
	return err == nil
}

// gitPath resolves a name inside this worktree's own git directory, which for a
// linked worktree is not `<root>/.git`.
func (r repo) gitPath(name string) (string, error) {
	out, err := r.git("rev-parse", "--git-path", name)
	if err != nil {
		return "", err
	}
	p := strings.TrimSpace(out)
	if !filepath.IsAbs(p) {
		p = filepath.Join(r.root, p)
	}
	return p, nil
}

// gitPathExists reports whether a name inside the worktree's git directory is present.
func (r repo) gitPathExists(name string) bool {
	p, err := r.gitPath(name)
	if err != nil {
		return false
	}
	_, statErr := os.Stat(p)
	return statErr == nil
}

// add stages the given files as they are in the working tree.
func (r repo) add(paths []string) error {
	_, err := r.gitWithStdin(joinNul(paths), "add", "--pathspec-from-file=-", "--pathspec-file-nul")
	return err
}

// abs turns a repo-relative, slash-separated path into an absolute one.
func (r repo) abs(path string) string {
	return filepath.Join(r.root, filepath.FromSlash(path))
}

func splitNul(s string) []string {
	var out []string
	for p := range strings.SplitSeq(s, "\x00") {
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func joinNul(paths []string) string {
	return strings.Join(paths, "\x00") + "\x00"
}

func splitLines(s string) []string {
	var out []string
	for line := range strings.SplitSeq(s, "\n") {
		if line = strings.TrimSpace(line); line != "" {
			out = append(out, line)
		}
	}
	return out
}

func toSet(paths []string) map[string]bool {
	set := make(map[string]bool, len(paths))
	for _, p := range paths {
		set[p] = true
	}
	return set
}

// intersect returns the members of paths that are in set, in their original order.
func intersect(paths []string, set map[string]bool) []string {
	var out []string
	for _, p := range paths {
		if set[p] {
			out = append(out, p)
		}
	}
	return out
}

func pluralize(n int, singular, plural string) string {
	if n == 1 {
		return singular
	}
	return plural
}
