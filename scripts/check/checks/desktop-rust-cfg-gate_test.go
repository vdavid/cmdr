package checks

import (
	"os"
	"path/filepath"
	"strings"
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

// --- findCfgGatedModules ---

func TestFindCfgGatedModules_BasicMod(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
mod foo;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 1 || result[0] != "foo" {
		t.Errorf("expected [foo], got %v", result)
	}
}

func TestFindCfgGatedModules_PubMod(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
pub mod bar;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 1 || result[0] != "bar" {
		t.Errorf("expected [bar], got %v", result)
	}
}

func TestFindCfgGatedModules_PubCrateMod(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
pub(crate) mod baz;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 1 || result[0] != "baz" {
		t.Errorf("expected [baz], got %v", result)
	}
}

func TestFindCfgGatedModules_RegularModNotFound(t *testing.T) {
	lines := strings.Split(`mod regular;
pub mod also_regular;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 0 {
		t.Errorf("expected empty result for ungated modules, got %v", result)
	}
}

func TestFindCfgGatedModules_BlankLineBetweenCfgAndMod(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]

mod spaced;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 1 || result[0] != "spaced" {
		t.Errorf("expected [spaced], got %v", result)
	}
}

func TestFindCfgGatedModules_MultipleStackedAttributes(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod stacked;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 1 || result[0] != "stacked" {
		t.Errorf("expected [stacked], got %v", result)
	}
}

func TestFindCfgGatedModules_MultipleModules(t *testing.T) {
	lines := strings.Split(`mod ungated;
#[cfg(target_os = "macos")]
mod gated_one;
pub mod also_ungated;
#[cfg(target_os = "macos")]
pub mod gated_two;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 2 {
		t.Fatalf("expected 2 gated modules, got %d: %v", len(result), result)
	}
	if result[0] != "gated_one" || result[1] != "gated_two" {
		t.Errorf("expected [gated_one, gated_two], got %v", result)
	}
}

// --- hasMacOSCfgAttribute ---

func TestHasMacOSCfgAttribute_DirectPreviousLine(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
use core_foundation::base;`, "\n")

	if !hasMacOSCfgAttribute(lines, 1) {
		t.Error("expected true for cfg gate on previous line")
	}
}

func TestHasMacOSCfgAttribute_BlankLineBetween(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]

use core_foundation::base;`, "\n")

	if !hasMacOSCfgAttribute(lines, 2) {
		t.Error("expected true for cfg gate with blank line between")
	}
}

func TestHasMacOSCfgAttribute_NoCfgGate(t *testing.T) {
	lines := strings.Split(`use serde::Serialize;
use core_foundation::base;`, "\n")

	if hasMacOSCfgAttribute(lines, 1) {
		t.Error("expected false when no cfg gate is present")
	}
}

func TestHasMacOSCfgAttribute_OtherAttributesBetween(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
#[allow(unused_imports)]
#[doc = "macOS-specific"]
use core_foundation::base;`, "\n")

	if !hasMacOSCfgAttribute(lines, 3) {
		t.Error("expected true for cfg gate with other attributes between")
	}
}

func TestHasMacOSCfgAttribute_InsideCfgGatedBlock(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
fn macos_only() {
    use core_foundation::base;
}`, "\n")

	if !hasMacOSCfgAttribute(lines, 2) {
		t.Error("expected true for use inside cfg-gated block")
	}
}

func TestHasMacOSCfgAttribute_FirstLine(t *testing.T) {
	lines := strings.Split(`use core_foundation::base;`, "\n")

	if hasMacOSCfgAttribute(lines, 0) {
		t.Error("expected false for first line with no preceding attributes")
	}
}

func TestHasMacOSCfgAttribute_NegatedCfgGate(t *testing.T) {
	lines := strings.Split(`#[cfg(not(target_os = "macos"))]
use fallback::thing;`, "\n")

	if hasMacOSCfgAttribute(lines, 1) {
		t.Error("expected false for negated cfg gate")
	}
}

// --- isMacOSGateAttribute ---

func TestIsMacOSGateAttribute(t *testing.T) {
	tests := []struct {
		name     string
		attr     string
		expected bool
	}{
		{
			name:     "basic cfg gate",
			attr:     `#[cfg(target_os = "macos")]`,
			expected: true,
		},
		{
			name:     "compound all gate",
			attr:     `#[cfg(all(test, target_os = "macos"))]`,
			expected: true,
		},
		{
			name:     "negated gate",
			attr:     `#[cfg(not(target_os = "macos"))]`,
			expected: false,
		},
		{
			name:     "unrelated attribute",
			attr:     `#[allow(unused)]`,
			expected: false,
		},
		{
			name:     "no target_os at all",
			attr:     `#[cfg(feature = "some-feature")]`,
			expected: false,
		},
		{
			name:     "target_os linux",
			attr:     `#[cfg(target_os = "linux")]`,
			expected: false,
		},
		{
			name:     "any gate with macos",
			attr:     `#[cfg(any(target_os = "macos", target_os = "ios"))]`,
			expected: true,
		},
		{
			name:     "all with not containing something else then macos",
			attr:     `#[cfg(all(not(target_os = "windows"), target_os = "macos"))]`,
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := isMacOSGateAttribute(tt.attr); got != tt.expected {
				t.Errorf("isMacOSGateAttribute(%q) = %v, want %v", tt.attr, got, tt.expected)
			}
		})
	}
}

// --- RunCfgGate integration tests ---

func TestRunCfgGate_ProperlyGatedPasses(t *testing.T) {
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0755); err != nil {
		t.Fatal(err)
	}

	// Write Cargo.toml with a macOS dep
	cargoDir := filepath.Join(root, "apps", "desktop", "src-tauri")
	cargoContent := `
[package]
name = "test-app"

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10.1"
`
	if err := os.WriteFile(filepath.Join(cargoDir, "Cargo.toml"), []byte(cargoContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write a lib.rs with no cfg-gated modules
	libContent := `mod something;
`
	if err := os.WriteFile(filepath.Join(srcDir, "lib.rs"), []byte(libContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write a properly gated .rs file
	rsContent := `#[cfg(target_os = "macos")]
use core_foundation::base::TCFType;

fn main() {}
`
	if err := os.WriteFile(filepath.Join(srcDir, "something.rs"), []byte(rsContent), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: root}
	result, err := RunCfgGate(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success result, got %v: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "1 gated use") {
		t.Errorf("expected message to mention gated uses, got: %s", result.Message)
	}
}

func TestRunCfgGate_UngatedReportsViolation(t *testing.T) {
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0755); err != nil {
		t.Fatal(err)
	}

	// Write Cargo.toml with a macOS dep
	cargoDir := filepath.Join(root, "apps", "desktop", "src-tauri")
	cargoContent := `
[package]
name = "test-app"

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10.1"
`
	if err := os.WriteFile(filepath.Join(cargoDir, "Cargo.toml"), []byte(cargoContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write lib.rs
	libContent := `mod ungated;
`
	if err := os.WriteFile(filepath.Join(srcDir, "lib.rs"), []byte(libContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write an ungated .rs file (missing cfg attribute)
	rsContent := `use core_foundation::base::TCFType;

fn main() {}
`
	if err := os.WriteFile(filepath.Join(srcDir, "ungated.rs"), []byte(rsContent), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: root}
	_, err := RunCfgGate(ctx)
	if err == nil {
		t.Fatal("expected error for ungated use of macOS crate")
	}
	if !strings.Contains(err.Error(), "core_foundation") {
		t.Errorf("expected error to mention core_foundation, got: %v", err)
	}
	if !strings.Contains(err.Error(), "ungated") {
		t.Errorf("expected error to mention the violating file, got: %v", err)
	}
}

func TestRunCfgGate_ModuleGatedFileSkipped(t *testing.T) {
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0755); err != nil {
		t.Fatal(err)
	}

	// Write Cargo.toml with a macOS dep
	cargoDir := filepath.Join(root, "apps", "desktop", "src-tauri")
	cargoContent := `
[package]
name = "test-app"

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10.1"
`
	if err := os.WriteFile(filepath.Join(cargoDir, "Cargo.toml"), []byte(cargoContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write lib.rs with a cfg-gated module
	libContent := `#[cfg(target_os = "macos")]
mod macos_only;
`
	if err := os.WriteFile(filepath.Join(srcDir, "lib.rs"), []byte(libContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write macos_only.rs with ungated use of macOS crate. Since the module itself
	// is gated in lib.rs, this file should be skipped entirely.
	rsContent := `use core_foundation::base::TCFType;

pub fn do_macos_stuff() {}
`
	if err := os.WriteFile(filepath.Join(srcDir, "macos_only.rs"), []byte(rsContent), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: root}
	result, err := RunCfgGate(ctx)
	if err != nil {
		t.Fatalf("expected success (module-gated file should be skipped), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success result, got %v: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "1 file skipped via module-level gating") {
		t.Errorf("expected message to mention skipped files, got: %s", result.Message)
	}
}

func TestRunCfgGate_DirectoryModuleGated(t *testing.T) {
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	macosModDir := filepath.Join(srcDir, "macos_mod")
	if err := os.MkdirAll(macosModDir, 0755); err != nil {
		t.Fatal(err)
	}

	// Write Cargo.toml
	cargoDir := filepath.Join(root, "apps", "desktop", "src-tauri")
	cargoContent := `
[package]
name = "test-app"

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10.1"
`
	if err := os.WriteFile(filepath.Join(cargoDir, "Cargo.toml"), []byte(cargoContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write lib.rs with a cfg-gated directory module
	libContent := `#[cfg(target_os = "macos")]
mod macos_mod;
`
	if err := os.WriteFile(filepath.Join(srcDir, "lib.rs"), []byte(libContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write mod.rs inside the directory module
	modContent := `mod inner;
`
	if err := os.WriteFile(filepath.Join(macosModDir, "mod.rs"), []byte(modContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write inner.rs with ungated use (should be skipped because parent module is gated)
	innerContent := `use core_foundation::base::TCFType;

pub fn inner_fn() {}
`
	if err := os.WriteFile(filepath.Join(macosModDir, "inner.rs"), []byte(innerContent), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: root}
	result, err := RunCfgGate(ctx)
	if err != nil {
		t.Fatalf("expected success (directory module gated), got error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success result, got %v: %s", result.Code, result.Message)
	}
	// Both mod.rs and inner.rs should be skipped
	if !strings.Contains(result.Message, "2 files skipped via module-level gating") {
		t.Errorf("expected 2 files skipped, got: %s", result.Message)
	}
}

func TestRunCfgGate_NoMacOSDepsEarlyReturn(t *testing.T) {
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0755); err != nil {
		t.Fatal(err)
	}

	// Write Cargo.toml with no macOS deps
	cargoDir := filepath.Join(root, "apps", "desktop", "src-tauri")
	cargoContent := `
[package]
name = "test-app"

[dependencies]
serde = "1.0"
`
	if err := os.WriteFile(filepath.Join(cargoDir, "Cargo.toml"), []byte(cargoContent), 0644); err != nil {
		t.Fatal(err)
	}

	// Write a .rs file (shouldn't matter since no macOS deps)
	if err := os.WriteFile(filepath.Join(srcDir, "lib.rs"), []byte("fn main() {}"), 0644); err != nil {
		t.Fatal(err)
	}

	ctx := &CheckContext{RootDir: root}
	result, err := RunCfgGate(ctx)
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if !strings.Contains(result.Message, "No macOS-only dependencies found") {
		t.Errorf("expected early return message, got: %s", result.Message)
	}
}

func TestRunCfgGate_MissingCargoToml(t *testing.T) {
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0755); err != nil {
		t.Fatal(err)
	}
	// Intentionally do not create Cargo.toml

	ctx := &CheckContext{RootDir: root}
	_, err := RunCfgGate(ctx)
	if err == nil {
		t.Fatal("expected error for missing Cargo.toml")
	}
	if !strings.Contains(err.Error(), "failed to parse Cargo.toml") {
		t.Errorf("expected parse error, got: %v", err)
	}
}

// --- collectAttribute ---

func TestCollectAttribute_SingleLine(t *testing.T) {
	lines := []string{`#[cfg(target_os = "macos")]`, `use foo::bar;`}
	result := collectAttribute(lines, 0)
	if !strings.Contains(result, `target_os = "macos"`) {
		t.Errorf("expected attribute text to contain target_os, got: %s", result)
	}
}

func TestCollectAttribute_MultiLine(t *testing.T) {
	lines := []string{
		`#[cfg(all(`,
		`    target_os = "macos",`,
		`    feature = "gui"`,
		`))]`,
		`use foo::bar;`,
	}
	result := collectAttribute(lines, 0)
	if !strings.Contains(result, `target_os = "macos"`) {
		t.Errorf("expected multi-line attribute to contain target_os, got: %s", result)
	}
	if !strings.Contains(result, `feature = "gui"`) {
		t.Errorf("expected multi-line attribute to contain feature, got: %s", result)
	}
}

// --- isAttributeContinuation ---

func TestIsAttributeContinuation(t *testing.T) {
	tests := []struct {
		name     string
		line     string
		expected bool
	}{
		{"closing bracket", "]", true},
		{"closing paren bracket", ")]", true},
		{"closing paren comma", "),", true},
		{"closing bracket comma", "],", true},
		{"quoted string", `"NSWindow",`, true},
		{"trailing comma", `something,`, true},
		{"plain code", "let x = 5;", false},
		{"function call", "foo()", false},
		{"empty string", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := isAttributeContinuation(tt.line); got != tt.expected {
				t.Errorf("isAttributeContinuation(%q) = %v, want %v", tt.line, got, tt.expected)
			}
		})
	}
}

// --- buildModuleGatedFileSet ---

func TestBuildModuleGatedFileSet_SingleFileModule(t *testing.T) {
	dir := t.TempDir()

	libContent := `#[cfg(target_os = "macos")]
mod macos_stuff;
`
	if err := os.WriteFile(filepath.Join(dir, "lib.rs"), []byte(libContent), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "macos_stuff.rs"), []byte("// macos code"), 0644); err != nil {
		t.Fatal(err)
	}

	gated, err := buildModuleGatedFileSet(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	expectedPath := filepath.Join(dir, "macos_stuff.rs")
	if !gated[expectedPath] {
		t.Errorf("expected %s in gated files, got %v", expectedPath, gated)
	}
}

func TestBuildModuleGatedFileSet_DirectoryModule(t *testing.T) {
	dir := t.TempDir()
	subDir := filepath.Join(dir, "platform")
	if err := os.MkdirAll(subDir, 0755); err != nil {
		t.Fatal(err)
	}

	libContent := `#[cfg(target_os = "macos")]
mod platform;
`
	if err := os.WriteFile(filepath.Join(dir, "lib.rs"), []byte(libContent), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(subDir, "mod.rs"), []byte("mod helpers;"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(subDir, "helpers.rs"), []byte("// helper code"), 0644); err != nil {
		t.Fatal(err)
	}

	gated, err := buildModuleGatedFileSet(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	modPath := filepath.Join(subDir, "mod.rs")
	helpersPath := filepath.Join(subDir, "helpers.rs")

	if !gated[modPath] {
		t.Errorf("expected %s in gated files", modPath)
	}
	if !gated[helpersPath] {
		t.Errorf("expected %s in gated files", helpersPath)
	}
}

func TestBuildModuleGatedFileSet_UngatedModuleNotIncluded(t *testing.T) {
	dir := t.TempDir()

	libContent := `mod regular;
#[cfg(target_os = "macos")]
mod macos_stuff;
`
	if err := os.WriteFile(filepath.Join(dir, "lib.rs"), []byte(libContent), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "regular.rs"), []byte("// regular code"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "macos_stuff.rs"), []byte("// macos code"), 0644); err != nil {
		t.Fatal(err)
	}

	gated, err := buildModuleGatedFileSet(dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	regularPath := filepath.Join(dir, "regular.rs")
	if gated[regularPath] {
		t.Errorf("ungated module %s should not be in gated files", regularPath)
	}

	macosPath := filepath.Join(dir, "macos_stuff.rs")
	if !gated[macosPath] {
		t.Errorf("gated module %s should be in gated files", macosPath)
	}
}

// --- scanForUngatedUses ---

func TestScanForUngatedUses_DetectsUngatedUse(t *testing.T) {
	dir := t.TempDir()

	rsContent := `use core_foundation::base::TCFType;

fn main() {}
`
	if err := os.WriteFile(filepath.Join(dir, "test.rs"), []byte(rsContent), 0644); err != nil {
		t.Fatal(err)
	}

	macOSModules := map[string]bool{"core_foundation": true}
	gatedFiles := map[string]bool{}

	violations, gatedCount, err := scanForUngatedUses(dir, macOSModules, gatedFiles)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(violations) != 1 {
		t.Fatalf("expected 1 violation, got %d", len(violations))
	}
	if violations[0].crateName != "core_foundation" {
		t.Errorf("expected crate name core_foundation, got %s", violations[0].crateName)
	}
	if violations[0].line != 1 {
		t.Errorf("expected line 1, got %d", violations[0].line)
	}
	if gatedCount != 0 {
		t.Errorf("expected 0 gated uses, got %d", gatedCount)
	}
}

func TestScanForUngatedUses_CountsGatedUses(t *testing.T) {
	dir := t.TempDir()

	rsContent := `#[cfg(target_os = "macos")]
use core_foundation::base::TCFType;

fn main() {}
`
	if err := os.WriteFile(filepath.Join(dir, "test.rs"), []byte(rsContent), 0644); err != nil {
		t.Fatal(err)
	}

	macOSModules := map[string]bool{"core_foundation": true}
	gatedFiles := map[string]bool{}

	violations, gatedCount, err := scanForUngatedUses(dir, macOSModules, gatedFiles)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(violations) != 0 {
		t.Fatalf("expected 0 violations, got %d", len(violations))
	}
	if gatedCount != 1 {
		t.Errorf("expected 1 gated use, got %d", gatedCount)
	}
}

func TestScanForUngatedUses_SkipsGatedFiles(t *testing.T) {
	dir := t.TempDir()

	rsContent := `use core_foundation::base::TCFType;
`
	filePath := filepath.Join(dir, "gated.rs")
	if err := os.WriteFile(filePath, []byte(rsContent), 0644); err != nil {
		t.Fatal(err)
	}

	macOSModules := map[string]bool{"core_foundation": true}
	gatedFiles := map[string]bool{filePath: true}

	violations, _, err := scanForUngatedUses(dir, macOSModules, gatedFiles)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(violations) != 0 {
		t.Fatalf("expected 0 violations (file should be skipped), got %d", len(violations))
	}
}

func TestScanForUngatedUses_IgnoresNonMacOSCrates(t *testing.T) {
	dir := t.TempDir()

	rsContent := `use serde::Serialize;
use tokio::runtime;
`
	if err := os.WriteFile(filepath.Join(dir, "test.rs"), []byte(rsContent), 0644); err != nil {
		t.Fatal(err)
	}

	macOSModules := map[string]bool{"core_foundation": true}
	gatedFiles := map[string]bool{}

	violations, gatedCount, err := scanForUngatedUses(dir, macOSModules, gatedFiles)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(violations) != 0 {
		t.Fatalf("expected 0 violations, got %d", len(violations))
	}
	if gatedCount != 0 {
		t.Errorf("expected 0 gated uses, got %d", gatedCount)
	}
}

// --- findCfgGatedModules edge cases ---

func TestFindCfgGatedModules_PubSuperMod(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
pub(super) mod internal;`, "\n")

	result := findCfgGatedModules(lines)
	if len(result) != 1 || result[0] != "internal" {
		t.Errorf("expected [internal], got %v", result)
	}
}

func TestFindCfgGatedModules_InlineModIgnored(t *testing.T) {
	// Inline mod (with block, no semicolon) should not match modDeclPattern
	lines := strings.Split(`#[cfg(target_os = "macos")]
mod inline {
    fn stuff() {}
}`, "\n")

	result := findCfgGatedModules(lines)
	// modDeclPattern requires `mod <name>;` (with semicolon), so inline blocks are not matched
	if len(result) != 0 {
		t.Errorf("expected no matches for inline mod block, got %v", result)
	}
}

func TestFindCfgGatedModules_EmptyInput(t *testing.T) {
	result := findCfgGatedModules([]string{})
	if len(result) != 0 {
		t.Errorf("expected empty result for empty input, got %v", result)
	}
}

// --- hasMacOSCfgAttribute edge cases ---

func TestHasMacOSCfgAttribute_CompoundAllGate(t *testing.T) {
	lines := strings.Split(`#[cfg(all(test, target_os = "macos"))]
use core_foundation::base;`, "\n")

	if !hasMacOSCfgAttribute(lines, 1) {
		t.Error("expected true for compound all() cfg gate")
	}
}

func TestHasMacOSCfgAttribute_NestedBlock(t *testing.T) {
	lines := strings.Split(`#[cfg(target_os = "macos")]
impl Foo {
    fn bar() {
        use core_foundation::base;
    }
}`, "\n")

	// Line 3 has the use statement. Walking back: line 2 is code with {,
	// which recursively checks line 1 (impl Foo {), which has the cfg on line 0.
	if !hasMacOSCfgAttribute(lines, 3) {
		t.Error("expected true for use inside nested cfg-gated block")
	}
}
