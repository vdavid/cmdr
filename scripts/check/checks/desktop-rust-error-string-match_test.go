package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runErrorStringMatchOn writes the supplied files into a temporary repo layout
// (rooted at the conventional `apps/desktop/src-tauri/src/` path) and runs the
// check against it, returning the result and any error.
func runErrorStringMatchOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	if err := os.MkdirAll(srcDir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	for rel, body := range files {
		full := filepath.Join(srcDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunErrorStringMatch(&CheckContext{RootDir: root})
}

func TestErrorStringMatch_FlagsMessageContains(t *testing.T) {
	_, err := runErrorStringMatchOn(t, map[string]string{
		"file_system/volume/smb.rs": `
fn handle(err: VolumeError) {
    match err {
        VolumeError::IoError { ref message, .. } if message.contains("FILE_IS_A_DIRECTORY") => {}
        _ => {}
    }
}
`,
	})
	if err == nil {
		t.Fatal("expected violation, got success")
	}
	if !strings.Contains(err.Error(), "smb.rs:4") {
		t.Errorf("expected violation at smb.rs:4, got: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "FILE_IS_A_DIRECTORY") {
		t.Errorf("expected message excerpt, got: %s", err.Error())
	}
}

func TestErrorStringMatch_FlagsStderrContains(t *testing.T) {
	_, err := runErrorStringMatchOn(t, map[string]string{
		"network/smb_smbutil.rs": `
fn classify(stderr: &str) {
    if stderr.contains("Authentication error") {
        // ...
    }
}
`,
		"file_system/git/status.rs": `
fn detect(stderr: &str) {
    let kind = if stderr.contains("index.lock") { 1 } else { 0 };
}
`,
	})
	if err == nil {
		t.Fatal("expected violations, got success")
	}
	if !strings.Contains(err.Error(), "smb_smbutil.rs") {
		t.Errorf("expected smb_smbutil hit, got: %s", err.Error())
	}
	if !strings.Contains(err.Error(), "status.rs") {
		t.Errorf("expected status.rs hit, got: %s", err.Error())
	}
}

func TestErrorStringMatch_FlagsToStringContains(t *testing.T) {
	_, err := runErrorStringMatchOn(t, map[string]string{
		"foo.rs": `
fn classify(err: SomeError) -> Kind {
    if err.to_string().contains("not found") { Kind::A } else { Kind::B }
}
`,
	})
	if err == nil {
		t.Fatal("expected violation, got success")
	}
}

func TestErrorStringMatch_AllowsOptOutOnPreviousLine(t *testing.T) {
	res, err := runErrorStringMatchOn(t, map[string]string{
		"foo.rs": `
fn classify(stderr: &str) {
    // allowed-error-string-match: smbutil exit codes aren't granular, see classify_smbutil_stderr
    if stderr.contains("Authentication error") {}
}
`,
	})
	if err != nil {
		t.Fatalf("expected success with opt-out, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestErrorStringMatch_AllowsTrailingOptOut(t *testing.T) {
	_, err := runErrorStringMatchOn(t, map[string]string{
		"foo.rs": `
fn classify(stderr: &str) {
    if stderr.contains("Authentication error") {} // allowed-error-string-match: see notes
}
`,
	})
	if err != nil {
		t.Fatalf("expected success with trailing opt-out, got: %v", err)
	}
}

func TestErrorStringMatch_SkipsDedicatedTestFiles(t *testing.T) {
	res, err := runErrorStringMatchOn(t, map[string]string{
		"foo_test.rs": `
#[test]
fn t() {
    assert!(err.message.contains("oops"));
}
`,
		"bar_tests.rs": `
fn _x() { let _ = stderr.contains("x"); }
`,
		"tests.rs": `
fn _x() { let _ = msg.to_string().contains("x"); }
`,
	})
	if err != nil {
		t.Fatalf("expected success on test files, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestErrorStringMatch_FlagsInlineCfgTestModSinceWeWantAssertionsTyped(t *testing.T) {
	// In-file test mods are intentionally scanned: `assert!(err.message.contains(...))`
	// is exactly the kind of stringly-typed assertion we want to push toward
	// `matches!(err, Variant { .. })`.
	_, err := runErrorStringMatchOn(t, map[string]string{
		"commands/rename.rs": `
pub fn rename() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t() {
        let res = rename();
        assert!(res.unwrap_err().message.contains("doesn't exist"));
    }
}
`,
	})
	if err == nil {
		t.Fatal("expected the in-file test mod's stringly-typed assertion to be flagged")
	}
	if !strings.Contains(err.Error(), "rename.rs") {
		t.Errorf("expected rename.rs hit, got: %s", err.Error())
	}
}

func TestErrorStringMatch_IgnoresCommentsAndDocs(t *testing.T) {
	res, err := runErrorStringMatchOn(t, map[string]string{
		"foo.rs": `
// Don't write code like ` + "`" + `if message.contains("foo")` + "`" + `.
//! ` + "`" + `stderr.contains("...")` + "`" + ` is bad too.
fn ok() {}
`,
	})
	if err != nil {
		t.Fatalf("expected success when only comments mention the patterns, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestErrorStringMatch_FlagsLowercaseContainsInline(t *testing.T) {
	// Regression for the May 2026 audit: write_operations/types.rs had an
	// inline `.to_string().to_lowercase()` followed by `.contains(...)`-style
	// classification. The new pattern flags this canonical chain even
	// without a `let lower = ...` binding.
	_, err := runErrorStringMatchOn(t, map[string]string{
		"write_classify.rs": `
fn classify(e: &io::Error) {
    if e.to_string().to_lowercase().contains("disconnect") {
        return WriteOperationError::DeviceDisconnected { path };
    }
}
`,
	})
	if err == nil {
		t.Fatal("expected violation for to_lowercase().contains chain")
	}
	if !strings.Contains(err.Error(), "to_lowercase") {
		t.Errorf("expected the chain text in the error, got: %s", err.Error())
	}
}

func TestErrorStringMatch_PassesOnCleanCode(t *testing.T) {
	res, err := runErrorStringMatchOn(t, map[string]string{
		"foo.rs": `
fn classify(err: &VolumeError) -> Kind {
    match err {
        VolumeError::NotFound(_) => Kind::A,
        VolumeError::PermissionDenied(_) => Kind::B,
        _ => Kind::C,
    }
}
`,
	})
	if err != nil {
		t.Fatalf("expected success on clean code, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "1 Rust file scanned") {
		t.Errorf("expected scanned count in success message, got: %s", res.Message)
	}
}
