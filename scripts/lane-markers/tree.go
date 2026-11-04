package main

import (
	"bytes"
	"fmt"
	"os/exec"
	"sort"
	"strings"
)

// entry is one tracked file of a commit's tree, as `git ls-tree -r` prints it.
type entry struct {
	mode, typ, object, path string
}

// tree is a commit's tracked files plus a way to read one. Everything lane-markers computes is a
// pure function of it, so the working tree (dirty or not) never leaks into a hash.
type tree interface {
	entries() []entry // sorted by path
	read(path string) ([]byte, error)
}

type gitTree struct {
	dir   string
	rev   string
	list  []entry
	index map[string]entry
}

func loadGitTree(dir, rev string) (*gitTree, error) {
	cmd := exec.Command("git", "ls-tree", "-r", "-z", "--full-tree", rev)
	cmd.Dir = dir
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git ls-tree %s: %v: %s", rev, err, strings.TrimSpace(stderr.String()))
	}
	list, err := parseLsTree(out)
	if err != nil {
		return nil, err
	}
	t := &gitTree{dir: dir, rev: rev, list: list, index: make(map[string]entry, len(list))}
	for _, e := range list {
		t.index[e.path] = e
	}
	return t, nil
}

// parseLsTree reads `git ls-tree -r -z` output: `<mode> SP <type> SP <object> TAB <path> NUL`.
func parseLsTree(out []byte) ([]entry, error) {
	var list []entry
	for rec := range bytes.SplitSeq(out, []byte{0}) {
		if len(rec) == 0 {
			continue
		}
		meta, path, ok := bytes.Cut(rec, []byte{'\t'})
		fields := strings.Fields(string(meta))
		if !ok || len(fields) != 3 {
			return nil, fmt.Errorf("unexpected git ls-tree record %q", rec)
		}
		list = append(list, entry{mode: fields[0], typ: fields[1], object: fields[2], path: string(path)})
	}
	sort.Slice(list, func(i, j int) bool { return list[i].path < list[j].path })
	return list, nil
}

func (t *gitTree) entries() []entry { return t.list }

func (t *gitTree) read(path string) ([]byte, error) {
	e, ok := t.index[path]
	if !ok {
		return nil, fmt.Errorf("%s isn't tracked at %s", path, t.rev)
	}
	cmd := exec.Command("git", "cat-file", "blob", e.object)
	cmd.Dir = t.dir
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git cat-file blob %s (%s): %v", e.object, path, err)
	}
	return out, nil
}

// hasFile and hasDir answer whether a local `uses:` target is a file or a directory.
func hasFile(t tree, path string) bool {
	for _, e := range t.entries() {
		if e.path == path {
			return true
		}
	}
	return false
}

func hasDir(t tree, dir string) bool {
	prefix := dir + "/"
	for _, e := range t.entries() {
		if strings.HasPrefix(e.path, prefix) {
			return true
		}
	}
	return false
}
