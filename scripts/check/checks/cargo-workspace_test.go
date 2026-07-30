package checks

import (
	"os"
	"path/filepath"
	"testing"
)

// writeFixtureWorkspace builds a throwaway cargo workspace on disk. `members` maps a
// path relative to the root onto that member's package name; a name suffixed with
// "!macos" declares the member macOS-only the way `crates/fsevent-stream` does.
func writeFixtureWorkspace(t *testing.T, memberList string, members map[string]string) string {
	t.Helper()
	root := t.TempDir()
	mustWrite(t, filepath.Join(root, "Cargo.toml"), "[workspace]\nmembers = "+memberList+"\nresolver = \"2\"\n")
	for dir, name := range members {
		manifest := "[package]\nname = \"" + name + "\"\nversion = \"0.0.0\"\n"
		if len(name) > 6 && name[len(name)-6:] == "!macos" {
			name = name[:len(name)-6]
			manifest = "[package]\nname = \"" + name + "\"\nversion = \"0.0.0\"\n\n" +
				"[package.metadata.cmdr]\nplatforms = [\"macos\"]\n"
		}
		mustWrite(t, filepath.Join(root, dir, "Cargo.toml"), manifest)
		mustWrite(t, filepath.Join(root, dir, "src", "lib.rs"), "// fixture\n")
	}
	return root
}

func mustWrite(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir %s: %v", path, err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

func TestWorkspaceMembersReadsNamesAndDirs(t *testing.T) {
	root := writeFixtureWorkspace(t, `["app", "crates/leaf"]`, map[string]string{
		"app":         "the-app",
		"crates/leaf": "the-leaf",
	})

	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	if len(members) != 2 {
		t.Fatalf("expected 2 members, got %d: %+v", len(members), members)
	}
	// Sorted by name, so the order is stable for callers that build command lines.
	if members[0].Name != "the-app" || members[1].Name != "the-leaf" {
		t.Fatalf("expected sorted names [the-app the-leaf], got [%s %s]", members[0].Name, members[1].Name)
	}
	if members[0].SrcDir != filepath.Join(root, "app", "src") {
		t.Errorf("SrcDir = %s, want %s", members[0].SrcDir, filepath.Join(root, "app", "src"))
	}
	if members[0].ManifestPath != filepath.Join(root, "app", "Cargo.toml") {
		t.Errorf("ManifestPath = %s", members[0].ManifestPath)
	}
	for _, m := range members {
		if len(m.Platforms) != 0 {
			t.Errorf("%s: expected no platform restriction, got %v", m.Name, m.Platforms)
		}
	}
}

func TestWorkspaceMembersExpandsGlobs(t *testing.T) {
	root := writeFixtureWorkspace(t, `["crates/*"]`, map[string]string{
		"crates/one": "one",
		"crates/two": "two",
	})

	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	if len(members) != 2 {
		t.Fatalf("glob member list should expand to 2, got %d: %+v", len(members), members)
	}
}

func TestWorkspaceMembersReadsPlatformRestriction(t *testing.T) {
	root := writeFixtureWorkspace(t, `["app", "crates/mac-only"]`, map[string]string{
		"app":             "the-app",
		"crates/mac-only": "mac-only!macos",
	})

	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	byName := map[string][]string{}
	for _, m := range members {
		byName[m.Name] = m.Platforms
	}
	if got := byName["mac-only"]; len(got) != 1 || got[0] != "macos" {
		t.Fatalf("mac-only platforms = %v, want [macos]", got)
	}
	if got := byName["the-app"]; len(got) != 0 {
		t.Fatalf("the-app platforms = %v, want none", got)
	}
}

func TestCargoSelectionExcludesMembersTheTargetCantCompile(t *testing.T) {
	members := []WorkspaceMember{
		{Name: "the-app"},
		{Name: "mac-only", Platforms: []string{"macos"}},
	}

	onMac := CargoSelectionArgs(members, "macos")
	if len(onMac) != 1 || onMac[0] != "--workspace" {
		t.Fatalf("on macOS the selection should be a bare --workspace, got %v", onMac)
	}

	onLinux := CargoSelectionArgs(members, "linux")
	want := []string{"--workspace", "--exclude", "mac-only"}
	if len(onLinux) != len(want) {
		t.Fatalf("on Linux the selection = %v, want %v", onLinux, want)
	}
	for i := range want {
		if onLinux[i] != want[i] {
			t.Fatalf("on Linux the selection = %v, want %v", onLinux, want)
		}
	}
}

func TestHostCargoOSMapsGoNamesToCargoNames(t *testing.T) {
	if got := cargoOSName("darwin"); got != "macos" {
		t.Errorf("cargoOSName(darwin) = %q, want macos", got)
	}
	if got := cargoOSName("linux"); got != "linux" {
		t.Errorf("cargoOSName(linux) = %q, want linux", got)
	}
}

// The real workspace has to satisfy the same contract the fixtures do, or every
// caller downstream is reasoning about a shape that doesn't exist.
func TestRealWorkspaceMembersResolve(t *testing.T) {
	// Tests run with the package dir as cwd; the repo root is three levels up.
	root, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatalf("failed to resolve repo root: %v", err)
	}
	if _, err := os.Stat(filepath.Join(root, "Cargo.toml")); err != nil {
		t.Skipf("repo layout not found from %s: %v", root, err)
	}

	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers on the real workspace: %v", err)
	}
	if len(members) < 3 {
		t.Fatalf("expected at least 3 workspace members, got %d", len(members))
	}
	names := map[string]WorkspaceMember{}
	for _, m := range members {
		names[m.Name] = m
	}
	if _, ok := names["cmdr"]; !ok {
		t.Errorf("the app package `cmdr` is missing from the member list: %v", names)
	}
	fork, ok := names["cmdr-fsevent-stream"]
	if !ok {
		t.Fatalf("`cmdr-fsevent-stream` is missing from the member list: %v", names)
	}
	// It wraps CoreServices; `#[link(kind = "framework")]` is an E0455 compile error
	// off Apple targets, so every non-macOS lane has to drop it from the selection.
	if len(fork.Platforms) != 1 || fork.Platforms[0] != "macos" {
		t.Errorf("cmdr-fsevent-stream platforms = %v, want [macos]", fork.Platforms)
	}
}

// seedAppFixtureWorkspace makes a temp dir look like this repo to the workspace
// reader: a root manifest with one member, the app crate. Fixtures for the Rust
// scanners need it because those scanners derive their source roots from the
// member list instead of hardcoding a path, so a fixture with no manifest is a
// fixture with no source roots at all.
func seedAppFixtureWorkspace(t *testing.T, root string) {
	t.Helper()
	mustWrite(t, filepath.Join(root, "Cargo.toml"),
		"[workspace]\nmembers = [\"apps/desktop/src-tauri\"]\nresolver = \"2\"\n")
	mustWrite(t, filepath.Join(root, "apps", "desktop", "src-tauri", "Cargo.toml"),
		"[package]\nname = \"cmdr\"\nversion = \"0.0.0\"\n")
}

func removeOrFail(t *testing.T, path string) {
	t.Helper()
	if err := os.Remove(path); err != nil {
		t.Fatalf("remove %s: %v", path, err)
	}
}
