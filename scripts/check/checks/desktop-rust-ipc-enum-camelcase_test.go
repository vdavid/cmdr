package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func scanFixture(t *testing.T, name string, rs string) []ipcEnumViolation {
	t.Helper()
	dir := t.TempDir()
	// Mirror the real layout: <root>/apps/desktop/src-tauri/src/<file>.rs
	srcDir := filepath.Join(dir, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(srcDir, name+".rs"), []byte(rs), 0644); err != nil {
		t.Fatal(err)
	}
	violations, _, err := scanIpcEnums(srcDir)
	if err != nil {
		t.Fatalf("scanIpcEnums: %v", err)
	}
	return violations
}

func TestScanIpcEnums_FlagsMissingRenameAllFields(t *testing.T) {
	rs := `
#[derive(serde::Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum UpgradeResult {
    CredentialsNeeded { display_name: String },
}
`
	got := scanFixture(t, "smb_upgrade", rs)
	if len(got) != 1 || got[0].enumName != "UpgradeResult" {
		t.Fatalf("expected 1 violation for UpgradeResult, got %#v", got)
	}
}

func TestScanIpcEnums_PassesWhenRenameAllFieldsPresent(t *testing.T) {
	rs := `
#[derive(serde::Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpgradeResult {
    CredentialsNeeded { display_name: String },
}
`
	got := scanFixture(t, "smb_upgrade", rs)
	if len(got) != 0 {
		t.Fatalf("expected no violations, got %#v", got)
	}
}

func TestScanIpcEnums_IgnoresSnakeCaseEnums(t *testing.T) {
	// MountError uses snake_case, so the rename_all_fields rule doesn't apply.
	rs := `
#[derive(serde::Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MountError {
    AuthFailed { message: String },
}
`
	got := scanFixture(t, "mount", rs)
	if len(got) != 0 {
		t.Fatalf("expected no violations for snake_case enum, got %#v", got)
	}
}

func TestScanIpcEnums_IgnoresUnitOnlyVariants(t *testing.T) {
	rs := `
#[derive(serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    Ready,
    Cancelled,
}
`
	got := scanFixture(t, "status", rs)
	if len(got) != 0 {
		t.Fatalf("expected no violations for unit-only enum, got %#v", got)
	}
}

func TestScanIpcEnums_IgnoresNonSpectaEnums(t *testing.T) {
	rs := `
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InternalThing {
    Foo { bar_baz: String },
}
`
	got := scanFixture(t, "internal", rs)
	if len(got) != 0 {
		t.Fatalf("expected no violations for non-specta enum, got %#v", got)
	}
}

func TestEnumHasStructVariant_BraceOnNextLine(t *testing.T) {
	rs := strings.Split(`pub enum Foo {
    Bar
    {
        x: u32,
    },
}`, "\n")
	if !enumHasStructVariant(rs, 0) {
		t.Fatal("expected struct variant with brace on next line to be detected")
	}
}

func TestEnumHasStructVariant_AllUnit(t *testing.T) {
	rs := strings.Split(`pub enum Foo {
    A,
    B,
    C,
}`, "\n")
	if enumHasStructVariant(rs, 0) {
		t.Fatal("expected unit-only enum to not be flagged")
	}
}
