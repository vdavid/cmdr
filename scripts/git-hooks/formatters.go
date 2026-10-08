package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
)

// A formatter is one of the tools CI's formatting lanes run.
type formatter interface {
	// name is the tool's name, for messages.
	name() string
	// unformatted lists the files, repo-relative, that the tool's CI lane would flag.
	unformatted() ([]string, error)
	// format rewrites, in place, those of the given repo-relative files the tool is
	// responsible for. It leaves every other file alone, so a caller can hand the
	// same list to each formatter.
	format(paths []string) error
}

// formatters returns the formatters that can run in this worktree. A tool that
// isn't installed is left out: a fresh clone without `node_modules`, or a machine
// without a Rust toolchain, still commits and pushes.
func formatters(root string) []formatter {
	var fs []formatter
	if bin := filepath.Join(root, "node_modules", ".bin", "oxfmt"); isFile(bin) {
		fs = append(fs, oxfmt{root: root, bin: bin})
	}
	if isFile(filepath.Join(root, "Cargo.toml")) && onPath("cargo") && onPath("rustfmt") {
		fs = append(fs, &rustfmt{root: root})
	}
	if onPath("gofmt") {
		fs = append(fs, gofmt{root: root})
	}
	return fs
}

func isFile(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return info.Mode().IsRegular()
}

func isDir(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return info.IsDir()
}

func onPath(tool string) bool {
	_, err := exec.LookPath(tool)
	return err == nil
}

// runTool runs a formatter and returns its stdout, its exit code, and an error that
// carries stderr when the tool couldn't be started at all.
func runTool(dir string, stdin []byte, name string, args ...string) (stdout []byte, exitCode int, err error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	if stdin != nil {
		cmd.Stdin = bytes.NewReader(stdin)
	}
	var out, errOut bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &errOut
	runErr := cmd.Run()
	var exitErr *exec.ExitError
	switch {
	case runErr == nil:
		return out.Bytes(), 0, nil
	case errors.As(runErr, &exitErr):
		return out.Bytes(), exitErr.ExitCode(), fmt.Errorf("%s: %s", filepath.Base(name), strings.TrimSpace(errOut.String()))
	default:
		return nil, -1, runErr
	}
}

// argChunk caps how many paths go on one command line, far below any ARG_MAX.
const argChunk = 200

func chunked(paths []string, fn func(chunk []string) error) error {
	var errs []error
	for start := 0; start < len(paths); start += argChunk {
		end := min(start+argChunk, len(paths))
		if err := fn(paths[start:end]); err != nil {
			errs = append(errs, err)
		}
	}
	return errors.Join(errs...)
}

// oxfmt formats the frontend, config, and Markdown files. Its scope is whatever
// `.oxfmtrc.json` says, so nothing here decides which files it owns: it's handed
// every candidate and skips what it ignores or can't parse.
type oxfmt struct {
	root string
	bin  string
}

func (oxfmt) name() string { return "oxfmt" }

func (o oxfmt) unformatted() ([]string, error) {
	out, code, err := runTool(o.root, nil, o.bin, "--list-different", ".")
	switch code {
	case 0:
		return nil, nil
	case 1:
		return splitLines(string(out)), nil
	default:
		return nil, err
	}
}

func (o oxfmt) format(paths []string) error {
	return chunked(paths, func(chunk []string) error {
		// Without the flag, a list holding only ignored or unsupported files is an
		// error, and that's the common case for a Rust-only commit.
		args := append([]string{"--no-error-on-unmatched-pattern"}, chunk...)
		_, _, err := runTool(o.root, nil, o.bin, args...)
		return err
	})
}

// rustfmt formats the Rust sources of the cargo workspace's members, which is the
// set `cargo fmt --all` covers. `vendor/` crates are not members and stay
// byte-identical to upstream.
type rustfmt struct {
	root string

	once      sync.Once
	workspace cargoWorkspace
	err       error
}

// cargoWorkspace is the slice of `cargo metadata` the formatter needs: which
// directory belongs to which package, and the edition cargo formats it with.
type cargoWorkspace struct {
	root     string
	packages []cargoPackage
}

type cargoPackage struct {
	// dir is the package directory relative to the workspace root, slash-separated;
	// "." for a package at the root.
	dir     string
	edition string
}

func (*rustfmt) name() string { return "rustfmt" }

func (f *rustfmt) load() (cargoWorkspace, error) {
	f.once.Do(func() {
		// `--no-deps` keeps this to the members and skips dependency resolution, so it
		// answers in well under a tenth of a second.
		out, _, err := runTool(f.root, nil, "cargo", "metadata", "--no-deps", "--format-version", "1")
		if err != nil {
			f.err = err
			return
		}
		f.workspace, f.err = parseCargoMetadata(out)
	})
	return f.workspace, f.err
}

func parseCargoMetadata(data []byte) (cargoWorkspace, error) {
	var metadata struct {
		WorkspaceRoot string `json:"workspace_root"`
		Packages      []struct {
			ManifestPath string `json:"manifest_path"`
			Edition      string `json:"edition"`
		} `json:"packages"`
	}
	if err := json.Unmarshal(data, &metadata); err != nil {
		return cargoWorkspace{}, fmt.Errorf("couldn't read cargo metadata: %w", err)
	}
	workspace := cargoWorkspace{root: metadata.WorkspaceRoot}
	for _, pkg := range metadata.Packages {
		rel, err := filepath.Rel(metadata.WorkspaceRoot, filepath.Dir(pkg.ManifestPath))
		if err != nil || strings.HasPrefix(rel, "..") {
			continue
		}
		workspace.packages = append(workspace.packages, cargoPackage{dir: filepath.ToSlash(rel), edition: pkg.Edition})
	}
	return workspace, nil
}

// owner returns the package a repo-relative file belongs to: the one with the
// deepest directory containing it.
func (w cargoWorkspace) owner(file string) (cargoPackage, bool) {
	var best cargoPackage
	found := false
	for _, pkg := range w.packages {
		if pkg.dir != "." && !strings.HasPrefix(file, pkg.dir+"/") {
			continue
		}
		if !found || len(pkg.dir) > len(best.dir) {
			best, found = pkg, true
		}
	}
	return best, found
}

func (f *rustfmt) unformatted() ([]string, error) {
	workspace, err := f.load()
	if err != nil {
		return nil, err
	}
	// The exact command the `rustfmt` check lane runs.
	out, code, runErr := runTool(f.root, nil, "cargo", "fmt", "--all", "--", "--check", "--files-with-diff")
	var files []string
	for _, line := range splitLines(string(out)) {
		if !strings.HasSuffix(line, ".rs") {
			continue
		}
		rel, relErr := filepath.Rel(workspace.root, line)
		if relErr != nil || strings.HasPrefix(rel, "..") {
			continue
		}
		files = append(files, filepath.ToSlash(rel))
	}
	if code != 0 && len(files) == 0 {
		return nil, runErr
	}
	return files, nil
}

func (f *rustfmt) format(paths []string) error {
	var rustFiles []string
	for _, p := range paths {
		if strings.HasSuffix(p, ".rs") {
			rustFiles = append(rustFiles, p)
		}
	}
	if len(rustFiles) == 0 {
		return nil
	}
	workspace, err := f.load()
	if err != nil {
		return err
	}

	var wg sync.WaitGroup
	slots := make(chan struct{}, runtime.NumCPU())
	for _, file := range rustFiles {
		pkg, ok := workspace.owner(file)
		if !ok {
			continue
		}
		slots <- struct{}{}
		wg.Go(func() {
			defer func() { <-slots }()
			formatRustFile(filepath.Join(f.root, filepath.FromSlash(file)), pkg.edition)
		})
	}
	wg.Wait()
	return nil
}

// formatRustFile formats one file through rustfmt's stdin. Given a path instead,
// rustfmt also rewrites every out-of-line `mod` the file declares, which for a
// `lib.rs` is the whole crate, including files with work in progress that nobody
// staged. Running in the file's own directory makes rustfmt resolve the same
// `rustfmt.toml` it would for the path (`crates/fsevent-stream` pins its own).
//
// A file rustfmt can't parse is left as it is: a commit of broken Rust is the
// compiler's to report.
func formatRustFile(file, edition string) {
	original, err := os.ReadFile(file)
	if err != nil {
		return
	}
	formatted, code, _ := runTool(filepath.Dir(file), original, "rustfmt", "--edition", edition)
	if code != 0 || len(formatted) == 0 || bytes.Equal(formatted, original) {
		return
	}
	info, err := os.Stat(file)
	if err != nil {
		return
	}
	_ = os.WriteFile(file, formatted, info.Mode().Perm())
}

// goDirs are the directories holding Go code, mirroring `GetGoDirectories` in
// `scripts/check/checks/common.go`, which is what the `gofmt` check lane walks.
var goDirs = []string{"scripts", "apps/desktop/scripts"}

// gofmt formats the repo's Go tooling.
type gofmt struct {
	root string
}

func (gofmt) name() string { return "gofmt" }

func (g gofmt) unformatted() ([]string, error) {
	var dirs []string
	for _, dir := range goDirs {
		if isDir(filepath.Join(g.root, filepath.FromSlash(dir))) {
			dirs = append(dirs, dir)
		}
	}
	if len(dirs) == 0 {
		return nil, nil
	}
	out, code, err := runTool(g.root, nil, "gofmt", append([]string{"-s", "-l"}, dirs...)...)
	if code != 0 {
		return nil, err
	}
	var files []string
	for _, line := range splitLines(string(out)) {
		files = append(files, filepath.ToSlash(line))
	}
	return files, nil
}

func (g gofmt) format(paths []string) error {
	var goFiles []string
	for _, p := range paths {
		if path.Ext(p) == ".go" && inGoDirs(p) {
			goFiles = append(goFiles, p)
		}
	}
	return chunked(goFiles, func(chunk []string) error {
		_, _, err := runTool(g.root, nil, "gofmt", append([]string{"-s", "-w"}, chunk...)...)
		return err
	})
}

func inGoDirs(file string) bool {
	for _, dir := range goDirs {
		if strings.HasPrefix(file, dir+"/") {
			return true
		}
	}
	return false
}
