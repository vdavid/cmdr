package checks

import (
	"os"
	"path/filepath"
	"testing"
)

// stampFixture creates a fake built binary and returns its path.
func stampFixture(t *testing.T) string {
	t.Helper()
	binaryPath := filepath.Join(t.TempDir(), "Cmdr")
	if err := os.WriteFile(binaryPath, []byte("not really a binary"), 0o755); err != nil {
		t.Fatalf("writing the fake binary: %v", err)
	}
	return binaryPath
}

func TestE2EBinaryIsCurrent(t *testing.T) {
	t.Run("no stamp means the binary's provenance is unknown", func(t *testing.T) {
		if e2eBinaryIsCurrent(stampFixture(t), "abc123") {
			t.Error("an unstamped binary was taken as current; a binary built before stamping existed must rebuild")
		}
	})

	t.Run("a matching stamp means the binary is current", func(t *testing.T) {
		binaryPath := stampFixture(t)
		if err := recordE2EBuild(binaryPath, "abc123"); err != nil {
			t.Fatalf("recordE2EBuild: %v", err)
		}
		if !e2eBinaryIsCurrent(binaryPath, "abc123") {
			t.Error("a binary stamped with the current fingerprint was rebuilt anyway")
		}
	})

	t.Run("a different fingerprint means a rebuild", func(t *testing.T) {
		binaryPath := stampFixture(t)
		if err := recordE2EBuild(binaryPath, "abc123"); err != nil {
			t.Fatalf("recordE2EBuild: %v", err)
		}
		if e2eBinaryIsCurrent(binaryPath, "def456") {
			t.Error("a stale binary was taken as current; the E2E suite would assert against the previous tree")
		}
	})

	t.Run("a stamp without its binary means a rebuild", func(t *testing.T) {
		binaryPath := stampFixture(t)
		if err := recordE2EBuild(binaryPath, "abc123"); err != nil {
			t.Fatalf("recordE2EBuild: %v", err)
		}
		if err := os.Remove(binaryPath); err != nil {
			t.Fatalf("removing the binary: %v", err)
		}
		if e2eBinaryIsCurrent(binaryPath, "abc123") {
			t.Error("a surviving stamp vouched for a binary that's gone")
		}
	})

	t.Run("a replaced binary invalidates the stamp", func(t *testing.T) {
		binaryPath := stampFixture(t)
		if err := recordE2EBuild(binaryPath, "abc123"); err != nil {
			t.Fatalf("recordE2EBuild: %v", err)
		}
		// What a plain `pnpm tauri build` in the same worktree does: same path,
		// different binary, no `playwright-e2e` feature in it.
		if err := os.WriteFile(binaryPath, []byte("a different binary entirely"), 0o755); err != nil {
			t.Fatalf("replacing the binary: %v", err)
		}
		if e2eBinaryIsCurrent(binaryPath, "abc123") {
			t.Error("a replaced binary kept its stamp; the suite would run against a build it can't drive")
		}
	})

	t.Run("an empty fingerprint never vouches for anything", func(t *testing.T) {
		binaryPath := stampFixture(t)
		if err := recordE2EBuild(binaryPath, ""); err == nil {
			t.Error("recordE2EBuild accepted an empty fingerprint; a failed fingerprint pass must not stamp the binary")
		}
		if e2eBinaryIsCurrent(binaryPath, "") {
			t.Error("an empty fingerprint matched; a failed fingerprint pass must force a rebuild")
		}
	})
}

// TestE2EBinaryInputsCoverTheBuildAndNothingElse pins the boundary the whole skip
// rests on: everything the binary is compiled from is in, and the Playwright suite's
// own specs and fixtures are out. Get the first half wrong and the suite asserts
// against a stale binary; get the second half wrong and every spec edit pays a
// 2-3 minute rebuild.
func TestE2EBinaryInputsCoverTheBuildAndNothingElse(t *testing.T) {
	patterns := inputs(e2eBinaryInputs(), GlobalInputs)

	for _, compiled := range []string{
		"apps/desktop/src/lib/ui/Button.svelte",
		"apps/desktop/src/routes/+layout.svelte",
		"apps/desktop/static/favicon.png",
		"apps/desktop/src-tauri/src/lib.rs",
		"apps/desktop/src-tauri/tauri.conf.json",
		"apps/desktop/scripts/tauri-wrapper.ts",
		"apps/desktop/package.json",
		"apps/desktop/vite.config.js",
		"crates/cmdr-fs/src/lib.rs",
		"Cargo.lock",
		"rust-toolchain.toml",
		"pnpm-lock.yaml",
		"CHANGELOG.md", // `whats_new` pulls it in with `include_str!`
		".mise.toml",   // a GlobalInput: a toolchain bump rebuilds
	} {
		if !matchesAny(compiled, patterns) {
			t.Errorf("`e2eBinaryInputs` misses %s, which the binary is built from: an edit to it would run the E2E suite against a stale binary", compiled)
		}
	}

	for _, runtimeOnly := range []string{
		"apps/desktop/test/e2e-playwright/app.spec.ts",
		"apps/desktop/test/e2e-playwright/playwright.config.ts",
		"apps/desktop/test/e2e-shared/fixtures.ts",
		"apps/desktop/test/smb-servers/start.sh",
		"apps/desktop/src/lib/ui/CLAUDE.md",
		"apps/desktop/src-tauri/src/file_system/CLAUDE.md",
	} {
		if matchesAny(runtimeOnly, patterns) {
			t.Errorf("`e2eBinaryInputs` covers %s, which Playwright reads at run time rather than compiles: editing it would cost a needless rebuild", runtimeOnly)
		}
	}
}
