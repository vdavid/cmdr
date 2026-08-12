package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// The cargo workspace's geometry, read once and shared by every check that has to
// reach further than the app crate: the cargo lanes (which package to select), the
// Rust source scanners (which trees to walk), cfg-gate (which manifest pairs with
// which tree), bindings-fresh (which sources feed the generated bindings), and the
// member-coverage meta-check (which members a lane may legitimately skip).
//
// Read straight from the manifests rather than from `cargo metadata`: the scanners
// don't otherwise need a toolchain, and a `cargo metadata` invocation on a cold
// target dir costs more than every scanner combined.

// MemberKind says what a workspace member IS, which decides which scanners have
// jurisdiction over it. A member declares its own kind; the default is the common
// case, so only the exceptions carry the key.
type MemberKind string

const (
	// KindApp is first-party code linked into the Cmdr app process. Every house
	// rule applies. This is the default when a member declares no kind.
	KindApp MemberKind = "app"
	// KindTool is a first-party standalone developer CLI. It's a separate process
	// with its own stdout and its own SQLite connection, so the rules written for
	// the app process (route output through `log::`, open every connection through
	// the page-cache factory) describe a situation it isn't in.
	KindTool MemberKind = "tool"
	// KindVendored is a third-party crate we forked and carry. The house rules
	// aren't ours to impose on it, and the diff against upstream is what makes a
	// rebase reviewable.
	KindVendored MemberKind = "vendored"
)

// WorkspaceMember is one entry of the root manifest's `[workspace] members`.
type WorkspaceMember struct {
	// Name is the `[package] name`, i.e. what `-p` and `--exclude` take. It is NOT
	// always the directory name: `crates/fsevent-stream` is `cmdr-fsevent-stream`.
	Name string
	// Dir is the absolute path of the directory holding the member's Cargo.toml.
	Dir string
	// ManifestPath is the absolute path of the member's Cargo.toml.
	ManifestPath string
	// SrcDir is the member's `src/` tree. It always exists for our members; a
	// caller that walks it should tolerate absence anyway.
	SrcDir string
	// Kind decides which scanners have jurisdiction. Declared in the member's own
	// manifest, defaulting to KindApp:
	//
	//	[package.metadata.cmdr]
	//	kind = "vendored"
	Kind MemberKind
	// Platforms is the target-OS allowlist the member declares for ITSELF, in
	// cargo's `target_os` spelling ("macos", "linux"). Empty means portable.
	// Declared in the member's own manifest, so a new platform-locked crate
	// teaches every lane about itself with no Go edit:
	//
	//	[package.metadata.cmdr]
	//	platforms = ["macos"]
	Platforms []string
}

// RelDir returns the member's directory relative to the repo root, in slash form,
// for messages and path-keyed lookups.
func (m WorkspaceMember) RelDir(rootDir string) string {
	rel, err := filepath.Rel(rootDir, m.Dir)
	if err != nil {
		return m.Dir
	}
	return filepath.ToSlash(rel)
}

// BuildsOn reports whether the member compiles for the given cargo target OS.
func (m WorkspaceMember) BuildsOn(targetOS string) bool {
	if len(m.Platforms) == 0 {
		return true
	}
	for _, p := range m.Platforms {
		if p == targetOS {
			return true
		}
	}
	return false
}

// workspaceManifest is the slice of the root Cargo.toml we read.
type workspaceManifest struct {
	Workspace struct {
		Members []string `toml:"members"`
	} `toml:"workspace"`
}

// memberManifest is the slice of a member Cargo.toml we read.
type memberManifest struct {
	Package struct {
		Name     string `toml:"name"`
		Metadata struct {
			Cmdr struct {
				Kind      string   `toml:"kind"`
				Platforms []string `toml:"platforms"`
			} `toml:"cmdr"`
		} `toml:"metadata"`
	} `toml:"package"`
}

// WorkspaceMembers returns every member of the workspace rooted at rootDir, sorted
// by package name so command lines and messages are stable across runs. Member
// entries may be globs (`crates/*`), which cargo allows and this expands the same
// way. `[workspace] exclude` needs no handling: an excluded path is not a member,
// so it never appears in the list to begin with (that's what keeps `benchmarks/smb`
// out of the lanes).
func WorkspaceMembers(rootDir string) ([]WorkspaceMember, error) {
	var root workspaceManifest
	rootManifest := filepath.Join(rootDir, "Cargo.toml")
	if _, err := toml.DecodeFile(rootManifest, &root); err != nil {
		return nil, fmt.Errorf("couldn't read %s: %w", rootManifest, err)
	}

	seen := make(map[string]bool)
	var members []WorkspaceMember
	for _, entry := range root.Workspace.Members {
		dirs, err := expandMemberEntry(rootDir, entry)
		if err != nil {
			return nil, err
		}
		for _, dir := range dirs {
			if seen[dir] {
				continue
			}
			seen[dir] = true
			member, err := readMemberManifest(dir)
			if err != nil {
				return nil, err
			}
			members = append(members, member)
		}
	}

	sort.Slice(members, func(i, j int) bool { return members[i].Name < members[j].Name })
	return members, nil
}

// expandMemberEntry resolves one `members` entry to absolute directories, expanding
// a glob if the entry has one.
func expandMemberEntry(rootDir, entry string) ([]string, error) {
	pattern := filepath.Join(rootDir, filepath.FromSlash(entry))
	if !strings.ContainsAny(entry, "*?[") {
		return []string{pattern}, nil
	}
	matches, err := filepath.Glob(pattern)
	if err != nil {
		return nil, fmt.Errorf("couldn't expand workspace member glob %q: %w", entry, err)
	}
	var dirs []string
	for _, match := range matches {
		if info, statErr := os.Stat(match); statErr == nil && info.IsDir() {
			dirs = append(dirs, match)
		}
	}
	return dirs, nil
}

func readMemberManifest(dir string) (WorkspaceMember, error) {
	manifestPath := filepath.Join(dir, "Cargo.toml")
	var manifest memberManifest
	if _, err := toml.DecodeFile(manifestPath, &manifest); err != nil {
		return WorkspaceMember{}, fmt.Errorf("couldn't read %s: %w", manifestPath, err)
	}
	if manifest.Package.Name == "" {
		return WorkspaceMember{}, fmt.Errorf("%s declares no `[package] name`", manifestPath)
	}
	kind := MemberKind(manifest.Package.Metadata.Cmdr.Kind)
	switch kind {
	case "":
		kind = KindApp
	case KindApp, KindTool, KindVendored:
	default:
		return WorkspaceMember{}, fmt.Errorf(
			"%s declares `[package.metadata.cmdr] kind = %q`; the known kinds are %q, %q, and %q",
			manifestPath, kind, KindApp, KindTool, KindVendored)
	}
	return WorkspaceMember{
		Name:         manifest.Package.Name,
		Dir:          dir,
		ManifestPath: manifestPath,
		SrcDir:       filepath.Join(dir, "src"),
		Kind:         kind,
		Platforms:    manifest.Package.Metadata.Cmdr.Platforms,
	}, nil
}

// MembersOfKind returns the workspace members matching any of the given kinds.
// Each Rust scanner names the kinds it governs at its own call site rather than
// sharing one "the Rust source roots" list, because the answers genuinely differ:
// `sqlite-open-direct` protects a process-wide slab that a standalone CLI doesn't
// share, and `log-error-macro` points at a macro no crate can reach across the
// boundary.
func MembersOfKind(rootDir string, kinds ...MemberKind) ([]WorkspaceMember, error) {
	members, err := WorkspaceMembers(rootDir)
	if err != nil {
		return nil, err
	}
	var out []WorkspaceMember
	for _, m := range members {
		for _, k := range kinds {
			if m.Kind == k {
				out = append(out, m)
				break
			}
		}
	}
	return out, nil
}

// RustSrcRoots returns the `src/` tree of every member of the given kinds, dropping
// any that isn't on disk (a fresh worktree, a member with no sources yet). Walking a
// missing root is an error in `filepath.WalkDir`, so the filter has to happen here
// rather than in each scanner.
func RustSrcRoots(rootDir string, kinds ...MemberKind) ([]string, error) {
	members, err := MembersOfKind(rootDir, kinds...)
	if err != nil {
		return nil, err
	}
	roots := make([]string, 0, len(members))
	for _, m := range members {
		if info, statErr := os.Stat(m.SrcDir); statErr == nil && info.IsDir() {
			roots = append(roots, m.SrcDir)
		}
	}
	return roots, nil
}

// CargoSelectionArgs builds the package-selection flags for a cargo invocation that
// should cover the whole workspace on the given target OS.
//
// The exclusions are not a nicety. `cmdr-fsevent-stream` is only ever a
// macOS-conditional DEPENDENCY of `cmdr`, so before `--workspace` no non-macOS lane
// ever touched it: the dep dropped out of the graph and `cmd.Dir`-scoped selection
// never named it. `--workspace` moves it into the SELECTION set, where the target
// gate no longer applies, and it fails at `cargo check` — not at link — with
// `E0455: link kind 'framework' is only supported on Apple targets`. That's every
// compiling lane, including the ones named for macOS: CI runs `desktop-rust-clippy`
// and `desktop-rust-tests` on ubuntu.
func CargoSelectionArgs(members []WorkspaceMember, targetOS string) []string {
	args := []string{"--workspace"}
	for _, m := range members {
		if !m.BuildsOn(targetOS) {
			args = append(args, "--exclude", m.Name)
		}
	}
	return args
}

// HostCargoSelectionArgs is CargoSelectionArgs for a cargo run on this machine.
// Lanes that shell into a Linux container must NOT use it: the container's OS is
// what matters, not the host's.
func HostCargoSelectionArgs(rootDir string) ([]string, error) {
	members, err := WorkspaceMembers(rootDir)
	if err != nil {
		return nil, err
	}
	return CargoSelectionArgs(members, cargoOSName(runtime.GOOS)), nil
}

// SharedTargetFeatureArgs is the feature set every lane compiling into the shared
// `target/` passes. It is ONE value on purpose, because cargo keys its artifacts
// by feature set: two lanes that disagree take turns rebuilding `cmdr` and
// everything above it, over and over, for no other reason than that they asked
// differently.
//
// Measured on a warm `target/` (macOS, rustc 1.97.1, 2026-08-12): re-running an
// identical `cargo build --workspace --tests` costs 0.6-1.3 s, while the same
// build after one that dropped `cmdr/virtual-mtp` costs 20 s, and the dropping
// build itself costs 92 s. `docs/notes/cargo-lane-feature-thrash.md` has the runs.
//
// `virtual-mtp` is the set because `desktop-rust-tests` genuinely needs it (~29
// MTP tests only COMPILE under it) and nothing else needs it absent: it's
// test-only, never enters a production build, and no longer changes the exported
// IPC surface (`ipc_collectors.rs` keeps its commands out of the bindings).
// It MUST stay package-qualified: a bare `--features virtual-mtp` changes meaning
// once more than one package is selected.
func SharedTargetFeatureArgs() []string {
	return []string{"--features", "cmdr/virtual-mtp"}
}

// HostCargoLaneArgs is the whole "which packages, which features" prefix for a
// cargo lane running on this machine against the shared `target/`: the workspace
// selection plus SharedTargetFeatureArgs. A lane builds its command line from
// this rather than assembling its own, so a new lane inherits the alignment
// instead of rediscovering it.
func HostCargoLaneArgs(rootDir string) ([]string, error) {
	selection, err := HostCargoSelectionArgs(rootDir)
	if err != nil {
		return nil, err
	}
	return append(selection, SharedTargetFeatureArgs()...), nil
}

// cargoOSName maps a Go `GOOS` onto cargo's `target_os` spelling. They agree
// everywhere Cmdr builds except macOS, which Go calls "darwin".
func cargoOSName(goos string) string {
	if goos == "darwin" {
		return "macos"
	}
	return goos
}

// appRustSrcDir is the app crate's source tree. Only the scanners that are pinned to
// the app tree on purpose use it (see `rustScannerJurisdictions`); everything else
// derives its roots from the member list.
func appRustSrcDir(rootDir string) string {
	return filepath.Join(rootDir, "apps", "desktop", "src-tauri", "src")
}
