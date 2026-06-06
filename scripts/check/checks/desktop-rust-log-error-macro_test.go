package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runLogErrorMacroOn writes the supplied files into a temp repo layout rooted
// at `apps/desktop/src-tauri/src/` and runs the check with the given allowlist.
func runLogErrorMacroOn(t *testing.T, files map[string]string, allowlist map[string]bool) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	for rel, body := range files {
		full := filepath.Join(srcDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	orig := allowlistedLogErrorSites
	allowlistedLogErrorSites = allowlist
	t.Cleanup(func() { allowlistedLogErrorSites = orig })
	return RunLogErrorMacro(&CheckContext{RootDir: root})
}

func TestLogErrorMacro_FlagsRawLogError(t *testing.T) {
	_, err := runLogErrorMacroOn(t, map[string]string{
		"foo.rs": "fn f() { log::error!(\"boom\"); }\n",
	}, map[string]bool{})
	if err == nil {
		t.Fatal("expected violation for raw log::error!, got success")
	}
	if !strings.Contains(err.Error(), "foo.rs") {
		t.Errorf("expected foo.rs in message, got: %s", err.Error())
	}
}

func TestLogErrorMacro_AllowlistedFileWithCallsPasses(t *testing.T) {
	res, err := runLogErrorMacroOn(t, map[string]string{
		"error_reporter/mod.rs": "fn f() { log::error!(\"the macro definition itself\"); }\n",
	}, map[string]bool{"apps/desktop/src-tauri/src/error_reporter/mod.rs": true})
	if err != nil {
		t.Fatalf("expected success for allowlisted file, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLogErrorMacro_FailsOnDeadAllowlistEntry(t *testing.T) {
	_, err := runLogErrorMacroOn(t, map[string]string{
		"foo.rs": "fn f() {}\n",
	}, map[string]bool{"apps/desktop/src-tauri/src/gone.rs": true})
	if err == nil {
		t.Fatal("expected failure for dead allowlist entry, got success")
	}
	if !strings.Contains(err.Error(), "gone.rs") || !strings.Contains(err.Error(), "no longer exists") {
		t.Errorf("expected dead-entry report naming gone.rs, got: %s", err.Error())
	}
}

func TestLogErrorMacro_FailsOnUnusedAllowlistEntry(t *testing.T) {
	_, err := runLogErrorMacroOn(t, map[string]string{
		"clean.rs": "fn f() { log::warn!(\"all migrated to log_error!\"); }\n",
	}, map[string]bool{"apps/desktop/src-tauri/src/clean.rs": true})
	if err == nil {
		t.Fatal("expected failure for unused allowlist entry, got success")
	}
	if !strings.Contains(err.Error(), "clean.rs") || !strings.Contains(err.Error(), "no `log::error!`") {
		t.Errorf("expected unused-entry report naming clean.rs, got: %s", err.Error())
	}
}
