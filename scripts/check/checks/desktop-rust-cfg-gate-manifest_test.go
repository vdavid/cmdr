package checks

import (
	"os"
	"path/filepath"
	"testing"
)

// --- extractMacOSCrateModules ---

func TestExtractMacOSCrateModules_BasicStringDep(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10.1"
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !modules["core_foundation"] {
		t.Errorf("expected core_foundation in modules, got %v", modules)
	}
}

func TestExtractMacOSCrateModules_InlineTable(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[target.'cfg(target_os = "macos")'.dependencies]
objc2 = { version = "0.6", features = ["std"] }
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !modules["objc2"] {
		t.Errorf("expected objc2 in modules, got %v", modules)
	}
}

func TestExtractMacOSCrateModules_GitDep(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[target.'cfg(target_os = "macos")'.dependencies]
cmdr-fsevent-stream = { git = "https://example.com/repo", rev = "abc123" }
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !modules["cmdr_fsevent_stream"] {
		t.Errorf("expected cmdr_fsevent_stream in modules, got %v", modules)
	}
}

func TestExtractMacOSCrateModules_MultiLineFeatureArray(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[target.'cfg(target_os = "macos")'.dependencies]
objc2-app-kit = { version = "0.3", features = [
    "NSApplication",
    "NSWindow",
    "NSView",
] }
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !modules["objc2_app_kit"] {
		t.Errorf("expected objc2_app_kit in modules, got %v", modules)
	}
}

func TestExtractMacOSCrateModules_NoMacOSDeps(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[dependencies]
serde = "1.0"
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(modules) != 0 {
		t.Errorf("expected empty modules for no macOS deps, got %v", modules)
	}
}

// A crate declared macOS-only for production AND unconditionally for tests links on
// every target, so naming it outside a gate breaks nothing. `tar` is the live case.
func TestExtractMacOSCrateModules_AllTargetDeclarationWins(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[target.'cfg(target_os = "macos")'.dependencies]
tar = "0.4"
core-foundation = "0.10.1"

[dev-dependencies]
tar = "0.4"
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if modules["tar"] {
		t.Error("tar is reachable on every target through [dev-dependencies]; it must not count as macOS-only")
	}
	if !modules["core_foundation"] {
		t.Error("core_foundation is macOS-only and must still count")
	}
}

// A `cfg(unix)` declaration reaches Linux too, the only other target this check guards, so
// it wins the same way. `xattr` is the live case: macOS-only for production quarantine
// reads, a Unix dev-dependency for the metadata tests (it doesn't build for Windows).
func TestExtractMacOSCrateModules_UnixDeclarationWins(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[target.'cfg(target_os = "macos")'.dependencies]
xattr = "1"
objc2 = "0.6"

[target.'cfg(unix)'.dev-dependencies]
xattr = "1"
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if modules["xattr"] {
		t.Error("xattr is reachable on Linux through cfg(unix) dev-dependencies; it must not count as macOS-only")
	}
	if !modules["objc2"] {
		t.Error("objc2 is macOS-only and must still count")
	}
}

func TestExtractMacOSCrateModules_HyphenToUnderscore(t *testing.T) {
	dir := t.TempDir()
	cargoPath := filepath.Join(dir, "Cargo.toml")
	content := `
[package]
name = "test"

[target.'cfg(target_os = "macos")'.dependencies]
my-great-crate = "1.0"
another-one = "2.0"
simple = "0.1"
`
	if err := os.WriteFile(cargoPath, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}

	modules, err := extractMacOSCrateModules(cargoPath)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	expected := map[string]bool{
		"my_great_crate": true,
		"another_one":    true,
		"simple":         true,
	}
	if len(modules) != len(expected) {
		t.Fatalf("expected %d modules, got %d: %v", len(expected), len(modules), modules)
	}
	for name := range expected {
		if !modules[name] {
			t.Errorf("expected %s in modules", name)
		}
	}
}

func TestExtractMacOSCrateModules_InvalidFile(t *testing.T) {
	_, err := extractMacOSCrateModules("/nonexistent/Cargo.toml")
	if err == nil {
		t.Error("expected error for nonexistent file")
	}
}
