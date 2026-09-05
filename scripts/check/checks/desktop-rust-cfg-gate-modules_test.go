// The `cfg-gate` check's MODULE lane: the crate's own `#[cfg(target_os = "macos")] mod x;`,
// as opposed to the macOS-only dependency crates covered in `desktop-rust-cfg-gate_test.go`.
//
// The shape it exists for: a `use` inserted directly under an existing `#[cfg]` line steals
// that attribute from the import below it, which compiles on a Mac and breaks the Linux build
// with nothing red locally.

package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

// writeCfgGateModuleWorkspace lays out a one-member workspace whose `lib.rs` declares a
// macOS-only `native_drag` module and a macOS-and-Linux `mtp` one, plus a `commands.rs`
// carrying the body under test.
func writeCfgGateModuleWorkspace(t *testing.T, commandsBody string) string {
	t.Helper()
	root := t.TempDir()

	mustWrite(t, filepath.Join(root, "Cargo.toml"),
		"[workspace]\nmembers = [\"apps/desktop/src-tauri\"]\nresolver = \"2\"\n")

	appDir := filepath.Join(root, "apps", "desktop", "src-tauri")
	mustWrite(t, filepath.Join(appDir, "Cargo.toml"), "[package]\nname = \"cmdr\"\nversion = \"0.0.0\"\n")
	mustWrite(t, filepath.Join(appDir, "src", "lib.rs"), `mod commands;
#[cfg(target_os = "macos")]
mod native_drag;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mtp;
`)
	mustWrite(t, filepath.Join(appDir, "src", "native_drag.rs"), "pub struct DragSessionLocality;\n")
	mustWrite(t, filepath.Join(appDir, "src", "mtp.rs"), "pub struct MtpDevice;\n")
	mustWrite(t, filepath.Join(appDir, "src", "commands.rs"), commandsBody)
	return root
}

// The exact shape that broke the Linux build: a new `use` inserted directly under an
// existing `#[cfg(target_os = "macos")]` takes that attribute for itself, leaving the
// import below it bare.
func TestRunCfgGate_FlagsAnImportOfAMacOSOnlyModuleThatLostItsGate(t *testing.T) {
	root := writeCfgGateModuleWorkspace(t, `#[cfg(target_os = "macos")]
use std::path::PathBuf;
use crate::native_drag::{self, DragSessionLocality};

fn main() {}
`)

	_, err := RunCfgGate(&CheckContext{RootDir: root})
	if err == nil {
		t.Fatal("expected the ungated import of the macOS-only `native_drag` module to fail the check")
	}
	if !strings.Contains(err.Error(), "macOS-only module 'crate::native_drag'") {
		t.Errorf("expected the offending module to be named, got: %v", err)
	}
	if !strings.Contains(err.Error(), "commands.rs:3") {
		t.Errorf("expected the line of the bare import, got: %v", err)
	}
}

func TestRunCfgGate_AcceptsAGatedImportOfAMacOSOnlyModule(t *testing.T) {
	root := writeCfgGateModuleWorkspace(t, `#[cfg(target_os = "macos")]
use crate::native_drag::DragSessionLocality;

fn main() {}
`)

	result, err := RunCfgGate(&CheckContext{RootDir: root})
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if !strings.Contains(result.Message, "1 gated use") {
		t.Errorf("expected the gated module use to be counted, got: %s", result.Message)
	}
}

// The module lane reads imports only. A macOS-only module of our own is named all over
// ordinary code (the `ipc.rs` command-registry macro, closure bodies, multi-line signatures),
// and deciding whether those lines sit under a gate needs a real parse. Flagging them from a
// line-based walk buries the finding in noise.
func TestRunCfgGate_IgnoresAPathQualifiedCallIntoAMacOSOnlyModule(t *testing.T) {
	root := writeCfgGateModuleWorkspace(t, "fn go() {\n    crate::native_drag::start_drag();\n}\n")

	if _, err := RunCfgGate(&CheckContext{RootDir: root}); err != nil {
		t.Fatalf("the module lane reads imports only, got: %v", err)
	}
}

// A module gated on macOS AND Linux exists in the Linux lane, so naming it outside a gate
// compiles. Flagging it would make the check fail on correct code.
func TestRunCfgGate_IgnoresAModuleThatAlsoBuildsOnLinux(t *testing.T) {
	root := writeCfgGateModuleWorkspace(t, "use crate::mtp::MtpDevice;\n\nfn main() {}\n")

	if _, err := RunCfgGate(&CheckContext{RootDir: root}); err != nil {
		t.Fatalf("a module that also builds on Linux needs no gate, got: %v", err)
	}
}

// Prose naming the module is not a use of it.
func TestRunCfgGate_ModuleNamedInACommentIsNotAUse(t *testing.T) {
	root := writeCfgGateModuleWorkspace(t, "// use crate::native_drag::DragSessionLocality;\nfn main() {}\n")

	if _, err := RunCfgGate(&CheckContext{RootDir: root}); err != nil {
		t.Fatalf("a comment is not a use, got: %v", err)
	}
}

// A file pulled in by `#[path = "..."] mod x;` from a gated file is just as absent from the
// Linux build, so its imports need no gate of their own.
func TestRunCfgGate_SkipsAFileIncludedByPathFromAGatedModule(t *testing.T) {
	root := writeCfgGateModuleWorkspace(t, "fn main() {}\n")
	appSrc := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	mustWrite(t, filepath.Join(appSrc, "native_drag.rs"), `pub struct DragSessionLocality;
#[path = "native_drag_tests.rs"]
mod native_drag_tests;
`)
	mustWrite(t, filepath.Join(appSrc, "native_drag_tests.rs"), "use crate::native_drag::DragSessionLocality;\n")

	if _, err := RunCfgGate(&CheckContext{RootDir: root}); err != nil {
		t.Fatalf("a `#[path]` child of a gated module needs no gate, got: %v", err)
	}
}

func TestMacOSOnlyModulePaths_NestedModule(t *testing.T) {
	srcDir := t.TempDir()
	mustWrite(t, filepath.Join(srcDir, "lib.rs"), "#[cfg(any(target_os = \"macos\", target_os = \"linux\"))]\nmod mtp;\n")
	mustWrite(t, filepath.Join(srcDir, "mtp", "mod.rs"), "#[cfg(target_os = \"macos\")]\npub mod macos_workaround;\n")

	paths, err := macOSOnlyModulePaths(srcDir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(paths) != 1 || paths[0] != "mtp::macos_workaround" {
		t.Errorf("expected [mtp::macos_workaround], got %v", paths)
	}
}

func TestIsExclusivelyMacOSGateAttribute(t *testing.T) {
	cases := []struct {
		attr string
		want bool
	}{
		{`#[cfg(target_os = "macos")]`, true},
		{`#[cfg(all(test, target_os = "macos"))]`, true},
		{`#[cfg(any(target_os = "macos", target_os = "linux"))]`, false},
		{`#[cfg(not(target_os = "macos"))]`, false},
		{`#[cfg(target_os = "linux")]`, false},
	}
	for _, c := range cases {
		if got := isExclusivelyMacOSGateAttribute(c.attr); got != c.want {
			t.Errorf("isExclusivelyMacOSGateAttribute(%q) = %v, want %v", c.attr, got, c.want)
		}
	}
}

// Longest path first, so a nested macOS-only module reports itself rather than its parent.
func TestModuleRefPattern_PrefersTheLongestPath(t *testing.T) {
	re := moduleRefPattern([]string{"mtp", "mtp::macos_workaround"})
	got := macOSModulesReferencedOn("use crate::mtp::macos_workaround::suppress;", re)
	if len(got) != 1 || got[0] != "mtp::macos_workaround" {
		t.Errorf("expected [mtp::macos_workaround], got %v", got)
	}
	if moduleRefPattern(nil) != nil {
		t.Error("expected a nil pattern for an empty module set")
	}
}
