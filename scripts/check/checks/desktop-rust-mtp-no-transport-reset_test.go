package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runMtpNoTransportResetOn writes the supplied files into a temp repo layout
// rooted at `apps/desktop/src-tauri/src/mtp/` and runs the check.
func runMtpNoTransportResetOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	mtpDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src", "mtp")
	for rel, body := range files {
		full := filepath.Join(mtpDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunMtpNoTransportReset(&CheckContext{RootDir: root})
}

func TestMtpNoTransportReset_FlagsResetByLocation(t *testing.T) {
	_, err := runMtpNoTransportResetOn(t, map[string]string{
		"connection/session_reset.rs": "async fn f() { let _ = MtpDeviceBuilder::new().reset_by_location(id).await; }\n",
	})
	if err == nil {
		t.Fatal("expected violation for reset_by_location, got success")
	}
	if !strings.Contains(err.Error(), "session_reset.rs") {
		t.Errorf("expected session_reset.rs in message, got: %s", err.Error())
	}
}

func TestMtpNoTransportReset_FlagsEveryResetEntryPoint(t *testing.T) {
	for _, call := range []string{"reset_by_serial(", "reset_by_location(", "reset_first("} {
		_, err := runMtpNoTransportResetOn(t, map[string]string{
			"connection/mod.rs": "async fn f() { MtpDevice::" + call + ").await; }\n",
		})
		if err == nil {
			t.Errorf("expected violation for %s, got success", call)
		}
	}
}

// A test file gets flagged too: the point is that Cmdr never sends the reset,
// and a test that sends one still sends one.
func TestMtpNoTransportReset_FlagsTestCode(t *testing.T) {
	_, err := runMtpNoTransportResetOn(t, map[string]string{
		"connection/tests.rs": "#[cfg(test)]\nmod tests {\n    async fn t() { MtpDevice::reset_first().await; }\n}\n",
	})
	if err == nil {
		t.Fatal("expected violation inside a test module, got success")
	}
}

func TestMtpNoTransportReset_PassesOnCleanTree(t *testing.T) {
	res, err := runMtpNoTransportResetOn(t, map[string]string{
		"connection/session_reset.rs": "async fn f() { reopen_after_session_reset().await; }\n",
		"watcher.rs":                  "// The word reset appears here in prose, which is fine.\n",
	})
	if err != nil {
		t.Fatalf("expected success on a clean tree, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}
