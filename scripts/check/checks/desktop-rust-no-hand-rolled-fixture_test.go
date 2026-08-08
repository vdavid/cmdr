package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

// runNoHandRolledFixtureOn writes the supplied files under
// `apps/desktop/src-tauri/src/` and runs the check.
func runNoHandRolledFixtureOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	writeFixtureFiles(t, filepath.Join(root, "apps", "desktop", "src-tauri", "src"), files)
	return RunNoHandRolledFixture(&CheckContext{RootDir: root})
}

func TestNoHandRolledFixture_FlagsALiteralInADedicatedTestFile(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"copy_tests.rs": `
fn fixture() {
    let cached = CachedScanResult {
        files: Vec::new(),
        per_path: Vec::new(),
    };
}
`,
	})
	if err == nil {
		t.Fatal("expected a violation, got success")
	}
	if !strings.Contains(err.Error(), "copy_tests.rs:3") {
		t.Errorf("expected copy_tests.rs:3, got: %s", err.Error())
	}
}

func TestNoHandRolledFixture_FlagsALiteralInAnInlineTestMod(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"scan_preview.rs": `
pub fn production() {}

#[cfg(test)]
mod tests {
    #[test]
    fn seeds() {
        let x = SourceHint { is_directory: false };
    }
}
`,
	})
	if err == nil {
		t.Fatal("expected a violation inside the inline test mod, got success")
	}
	if !strings.Contains(err.Error(), "scan_preview.rs:8") {
		t.Errorf("expected scan_preview.rs:8, got: %s", err.Error())
	}
}

// The named constructors and the production literals they wrap live in
// production files, which the test-code scoping excludes. That's what keeps the
// check off `preflight.rs`'s six `SourceHint {` sites.
func TestNoHandRolledFixture_IgnoresProductionLiterals(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"preflight.rs": `
fn build() -> VolumePreflight {
    let hint = SourceHint { is_directory: true };
    VolumePreflight { hints: vec![hint] }
}
`,
	})
	if err != nil {
		t.Fatalf("production literals are out of jurisdiction, got: %s", err.Error())
	}
}

// A return type and a type definition both end in `Type {` without constructing
// anything. Flagging `fn cached_for(...) -> CachedScanResult {` would fire on
// every helper in the tree.
func TestNoHandRolledFixture_IgnoresReturnTypesAndDefinitions(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"scan_cache_tests.rs": `
fn cached_for(sources: &[&str]) -> CachedScanResult {
    CachedScanResult::from_volume_batch(paths(sources), 1, 10, 10, Vec::new())
}

struct CachedScanResult {
    files: Vec<FileInfo>,
}

impl CachedScanResult {
    fn nothing() {}
}
`,
	})
	if err != nil {
		t.Fatalf("declarations and return positions construct nothing, got: %s", err.Error())
	}
}

func TestNoHandRolledFixture_HonorsTheDirectiveAbove(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"canary_tests.rs": `
fn incoherent() {
    // allowed-hand-rolled-fixture: the point IS the shape the constructors refuse
    let cached = CachedScanResult { files: Vec::new() };
}
`,
	})
	if err != nil {
		t.Fatalf("directive should excuse the literal, got: %s", err.Error())
	}
}

func TestNoHandRolledFixture_FailsOnAnOrphanDirective(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"clean_tests.rs": `
// allowed-hand-rolled-fixture: nothing here to excuse
fn nothing() {}
`,
	})
	if err == nil {
		t.Fatal("expected an orphan-directive failure, got success")
	}
	if !strings.Contains(err.Error(), "unused") {
		t.Errorf("expected an unused-directive report, got: %s", err.Error())
	}
}

// Only the allowlisted cross-boundary types are in scope; a test building its
// own local struct is nobody's business.
func TestNoHandRolledFixture_IgnoresOtherTypes(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"other_tests.rs": `
fn fixture() {
    let x = SomeOtherThing { a: 1 };
    let y = MyCachedScanResultWrapper { inner: 2 };
}
`,
	})
	if err != nil {
		t.Fatalf("only the allowlisted types are in scope, got: %s", err.Error())
	}
}

// Every current instance puts the type name and the brace on one line. Assert
// that assumption rather than leaving it implicit: a literal split across lines
// slips past a line scanner, and this is where a future reader finds out.
func TestNoHandRolledFixture_KnownLimitAMultiLineLiteralSlipsPast(t *testing.T) {
	_, err := runNoHandRolledFixtureOn(t, map[string]string{
		"split_tests.rs": `
fn fixture() {
    let cached = CachedScanResult
    {
        files: Vec::new(),
    };
}
`,
	})
	if err != nil {
		t.Fatalf("documenting the known limit: a split literal is not matched, got: %s", err.Error())
	}
}
