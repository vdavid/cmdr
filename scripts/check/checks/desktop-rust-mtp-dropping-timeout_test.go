package checks

import (
	"strings"
	"testing"
)

// runMtpDroppingTimeoutOn lays out both MTP trees and runs the check.
func runMtpDroppingTimeoutOn(t *testing.T, appFiles, crateFiles map[string]string) (CheckResult, error) {
	t.Helper()
	root := seedMtpFixtureWorkspace(t, appFiles, crateFiles)
	return RunMtpDroppingTimeout(&CheckContext{RootDir: root})
}

// The whole session layer lives in `crates/cmdr-mtp` now, and it is where every
// PTP transaction is issued. A check that still walked the app tree alone would
// have nothing left to guard.
func TestMtpDroppingTimeout_ReachesTheCrate(t *testing.T) {
	_, err := runMtpDroppingTimeoutOn(t,
		map[string]string{"watcher.rs": "fn f() {}\n"},
		map[string]string{"connection/file_ops.rs": "async fn f() { tokio::time::timeout(d, read).await; }\n"},
	)
	if err == nil {
		t.Fatal("expected a violation inside crates/cmdr-mtp, got success")
	}
	if !strings.Contains(err.Error(), "crates/cmdr-mtp/src/connection/file_ops.rs") {
		t.Errorf("expected the crate path in the message, got: %s", err.Error())
	}
}

// The app half still issues mtp-rs calls (the hotplug watcher, the registrar
// wiring), so it stays in scope beside the crate.
func TestMtpDroppingTimeout_StillReachesTheAppTree(t *testing.T) {
	_, err := runMtpDroppingTimeoutOn(t,
		map[string]string{"watcher.rs": "fn f() { handle.abort(); }\n"},
		map[string]string{"lib.rs": "//! x\n"},
	)
	if err == nil {
		t.Fatal("expected a violation in the app's src/mtp/, got success")
	}
	if !strings.Contains(err.Error(), "apps/desktop/src-tauri/src/mtp/watcher.rs") {
		t.Errorf("expected the app path in the message, got: %s", err.Error())
	}
}

// A reasoned opt-out still excuses a site, wherever it lives.
func TestMtpDroppingTimeout_HonorsTheDirectiveInTheCrate(t *testing.T) {
	res, err := runMtpDroppingTimeoutOn(t,
		map[string]string{"watcher.rs": "fn f() {}\n"},
		map[string]string{"connection/mod.rs": "async fn f() {\n    // allowed-dropping-timeout: a Mutex wait holds nothing on the wire\n    tokio::time::timeout(d, lock).await;\n}\n"},
	)
	if err != nil {
		t.Fatalf("expected the directive to excuse the site, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

// Test files are skipped in both trees: a test asserting with a tight timeout
// drives no real device.
func TestMtpDroppingTimeout_SkipsTestFiles(t *testing.T) {
	res, err := runMtpDroppingTimeoutOn(t,
		map[string]string{"watcher.rs": "fn f() {}\n"},
		map[string]string{
			"lib.rs":                       "//! x\n",
			"connection/host_seam_test.rs": "async fn t() { tokio::time::timeout(d, x).await; }\n",
		},
	)
	if err != nil {
		t.Fatalf("expected test files to be skipped, got: %v", err)
	}
	if !strings.Contains(res.Message, "2 MTP Rust files scanned") {
		t.Errorf("expected only the two non-test files counted, got: %s", res.Message)
	}
}
