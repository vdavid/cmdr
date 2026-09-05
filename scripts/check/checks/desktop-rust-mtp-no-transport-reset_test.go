package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

// seedMtpFixtureWorkspace lays out a workspace with both MTP trees: the app's
// `src/mtp/` and the whole `cmdr-mtp` crate. Returns the repo root.
func seedMtpFixtureWorkspace(t *testing.T, appFiles, crateFiles map[string]string) string {
	t.Helper()
	root := t.TempDir()
	mustWrite(t, filepath.Join(root, "Cargo.toml"),
		"[workspace]\nmembers = [\"apps/desktop/src-tauri\", \"crates/cmdr-mtp\"]\nresolver = \"2\"\n")
	mustWrite(t, filepath.Join(root, "apps", "desktop", "src-tauri", "Cargo.toml"),
		"[package]\nname = \"cmdr\"\nversion = \"0.0.0\"\n")
	mustWrite(t, filepath.Join(root, "crates", "cmdr-mtp", "Cargo.toml"),
		"[package]\nname = \"cmdr-mtp\"\nversion = \"0.0.0\"\n")
	writeFixtureFiles(t, filepath.Join(root, "apps", "desktop", "src-tauri", "src", "mtp"), appFiles)
	writeFixtureFiles(t, filepath.Join(root, "crates", "cmdr-mtp", "src"), crateFiles)
	return root
}

// runMtpNoTransportResetOn writes the supplied files into the app's `src/mtp/`
// and runs the check.
func runMtpNoTransportResetOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := seedMtpFixtureWorkspace(t, files, map[string]string{"lib.rs": "//! nothing to see\n"})
	return RunMtpNoTransportReset(&CheckContext{RootDir: root})
}

// The reset guardrail followed the session layer into `crates/cmdr-mtp`. A check
// that still walked the app tree alone would pass over the very file that
// recovers from a session reset, which is where a reset is most tempting.
func TestMtpNoTransportReset_ReachesTheCrate(t *testing.T) {
	root := seedMtpFixtureWorkspace(t,
		map[string]string{"watcher.rs": "fn f() { /* clean */ }\n"},
		map[string]string{"connection/session_reset.rs": "async fn f() { MtpDevice::reset_by_serial(s).await; }\n"},
	)
	_, err := RunMtpNoTransportReset(&CheckContext{RootDir: root})
	if err == nil {
		t.Fatal("expected a violation inside crates/cmdr-mtp, got success")
	}
	if !strings.Contains(err.Error(), "crates/cmdr-mtp/src/connection/session_reset.rs") {
		t.Errorf("expected the crate path in the message, got: %s", err.Error())
	}
}

// Both trees are scanned in one pass, so a clean run's count covers both.
func TestMtpNoTransportReset_ScansBothTrees(t *testing.T) {
	root := seedMtpFixtureWorkspace(t,
		map[string]string{"watcher.rs": "fn f() {}\n", "volume_wiring.rs": "fn g() {}\n"},
		map[string]string{"lib.rs": "//! x\n", "connection/mod.rs": "fn h() {}\n"},
	)
	res, err := RunMtpNoTransportReset(&CheckContext{RootDir: root})
	if err != nil {
		t.Fatalf("expected success on two clean trees, got: %v", err)
	}
	if !strings.Contains(res.Message, "4 MTP Rust files scanned") {
		t.Errorf("expected all four files counted, got: %s", res.Message)
	}
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
