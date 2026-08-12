package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The marker hash is the whole check on the warm path: when it matches, the check
// returns "in sync (cached)" without regenerating anything. So every input that can
// change `bindings.ts` has to be inside it. An input that isn't makes the check
// report success over stale bindings — silently, and `bindings-fresh` is `NotInCI`,
// so nothing downstream catches it either.

// writeBindingsFixture lays out a two-member workspace with a root lockfile.
func writeBindingsFixture(t *testing.T) (root string, members []WorkspaceMember) {
	t.Helper()
	root = t.TempDir()
	mustWrite(t, filepath.Join(root, "Cargo.toml"),
		"[workspace]\nmembers = [\"apps/desktop/src-tauri\", \"crates/cmdr-index\"]\nresolver = \"2\"\n")
	mustWrite(t, filepath.Join(root, "Cargo.lock"), "version = 4\n")

	appDir := filepath.Join(root, "apps", "desktop", "src-tauri")
	mustWrite(t, filepath.Join(appDir, "Cargo.toml"), "[package]\nname = \"cmdr\"\nversion = \"0.0.0\"\n")
	mustWrite(t, filepath.Join(appDir, "src", "lib.rs"), "pub mod ipc;\n")

	crateDir := filepath.Join(root, "crates", "cmdr-index")
	mustWrite(t, filepath.Join(crateDir, "Cargo.toml"), "[package]\nname = \"cmdr-index\"\nversion = \"0.0.0\"\n")
	mustWrite(t, filepath.Join(crateDir, "src", "types.rs"),
		"#[derive(specta::Type)]\npub struct DirStats { pub files: u64 }\n")

	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	return root, members
}

func hashOrFail(t *testing.T, root string, members []WorkspaceMember) string {
	t.Helper()
	h, err := hashBindingsInputs(root, members)
	if err != nil {
		t.Fatalf("hashBindingsInputs: %v", err)
	}
	return h
}

// A `specta::Type` in a crate reaches `bindings.ts` through the app's command
// signatures, so editing it has to invalidate the marker.
func TestBindingsHashCoversEveryWorkspaceMembersSources(t *testing.T) {
	root, members := writeBindingsFixture(t)
	before := hashOrFail(t, root, members)

	mustWrite(t, filepath.Join(root, "crates", "cmdr-index", "src", "types.rs"),
		"#[derive(specta::Type)]\npub struct DirStats { pub files: u64, pub bytes: u64 }\n")

	if after := hashOrFail(t, root, members); after == before {
		t.Fatal("editing a specta type in a crate left the marker hash unchanged, so the check would report `in sync (cached)` over stale bindings")
	}
}

// The lockfile is at the WORKSPACE root, not inside the app package. Hashing a path
// that never exists contributes only the path name, so a dependency bump — which can
// change a derived type — never invalidated the marker.
func TestBindingsHashCoversTheWorkspaceLockfile(t *testing.T) {
	root, members := writeBindingsFixture(t)
	before := hashOrFail(t, root, members)

	mustWrite(t, filepath.Join(root, "Cargo.lock"), "version = 4\n\n[[package]]\nname = \"specta\"\n")

	if after := hashOrFail(t, root, members); after == before {
		t.Fatal("a Cargo.lock change left the marker hash unchanged")
	}
}

// The root manifest carries `[workspace.lints]` and the member list, both of which
// change what compiles.
func TestBindingsHashCoversTheRootManifest(t *testing.T) {
	root, members := writeBindingsFixture(t)
	before := hashOrFail(t, root, members)

	mustWrite(t, filepath.Join(root, "Cargo.toml"),
		"[workspace]\nmembers = [\"apps/desktop/src-tauri\", \"crates/cmdr-index\"]\nresolver = \"2\"\n\n[workspace.lints.rust]\nunused = \"deny\"\n")

	if after := hashOrFail(t, root, members); after == before {
		t.Fatal("a root manifest change left the marker hash unchanged")
	}
}

// A missing required input must be loud. Skipping it silently is what let the
// wrong-path lockfile go unnoticed.
func TestBindingsHashFailsLoudlyOnAMissingRequiredInput(t *testing.T) {
	root, members := writeBindingsFixture(t)
	removeOrFail(t, filepath.Join(root, "Cargo.lock"))

	if _, err := hashBindingsInputs(root, members); err == nil {
		t.Fatal("a missing Cargo.lock must fail the hash rather than silently drop the input")
	}
}

// TestBindingsRegenAsksCargoTheSameQuestionAsTheOtherLanes is the guardrail for
// the one cargo invocation that lives outside Go. `bindings-fresh` shells out to
// `pnpm bindings:regen`, so the alignment every other lane gets from
// `HostCargoLaneArgs` has to be spelled out by hand in `package.json` — and the
// day it drifts, nothing goes red. It just gets slow again: the regen would ask
// cargo a different question, rebuild `cmdr` to answer it (measured at 100 s on a
// warm tree), and leave the next `desktop-rust-tests` a 20 s bill to rebuild it
// back.
func TestBindingsRegenAsksCargoTheSameQuestionAsTheOtherLanes(t *testing.T) {
	root := repoRootForTest(t)
	manifest, err := os.ReadFile(filepath.Join(root, "apps", "desktop", "package.json"))
	if err != nil {
		t.Fatalf("reading the desktop package.json: %v", err)
	}
	var pkg struct {
		Scripts map[string]string `json:"scripts"`
	}
	if err := json.Unmarshal(manifest, &pkg); err != nil {
		t.Fatalf("parsing the desktop package.json: %v", err)
	}
	script, ok := pkg.Scripts["bindings:regen"]
	if !ok {
		t.Fatal("the desktop package.json has no `bindings:regen` script; `bindings-fresh` shells out to it")
	}

	members, err := WorkspaceMembers(root)
	if err != nil {
		t.Fatalf("WorkspaceMembers: %v", err)
	}
	// macOS is the canonical platform for the bindings: the committed file is the
	// macOS command surface, which is why the check carries a `NotInCI` reason.
	want := append(CargoSelectionArgs(members, "macos"), SharedTargetFeatureArgs()...)
	for _, arg := range want {
		if !strings.Contains(script, arg) {
			t.Errorf("`bindings:regen` is missing %q, so it compiles against a different set of `target/` artifacts than the cargo lanes do\n  script: %s",
				arg, script)
		}
	}
	// A `cd` into the package dir is what made the regen a package-scoped run in
	// the first place, which is a different question again.
	if strings.Contains(script, "cd src-tauri") {
		t.Error("`bindings:regen` cds into the package dir, which scopes cargo to one package and re-resolves dependency features")
	}
}
