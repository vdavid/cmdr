package checks

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// RunDesktopBindingsFresh fails if `apps/desktop/src/lib/ipc/bindings.ts` is
// out of sync with what `pnpm bindings:regen` would produce, i.e. somebody
// edited a Rust command surface without regenerating the typed IPC bindings.
//
// Strategy: hash the inputs that could affect the generated bindings (every
// `.rs` file under `src-tauri/src` plus `Cargo.lock` and `Cargo.toml`) and the
// current `bindings.ts`. If both hashes match the marker from the last
// successful run, skip the regen entirely; that's the common case and turns a
// ~2-minute test-mode compile into a ~50 ms hash scan. Otherwise: snapshot the
// committed `bindings.ts`, run the regen, diff bytes, restore the snapshot
// regardless of outcome (so the working tree stays exactly as the caller left
// it), and update the marker on success.
//
// The marker lives at `<CARGO_TARGET_DIR>/.bindings-fresh-marker` (or
// `<workspace>/target/.bindings-fresh-marker` if the env var is unset), so it
// shares fate with cargo's build artifacts: a `cargo clean` or wholesale
// `target/` deletion auto-invalidates it. Mirrors the
// `node_modules/.pnpm-install-marker` pattern used by `EnsurePnpmDependencies`.
func RunDesktopBindingsFresh(ctx *CheckContext) (CheckResult, error) {
	bindingsPath := filepath.Join(ctx.RootDir, "apps", "desktop", "src", "lib", "ipc", "bindings.ts")
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	rustDir := filepath.Join(desktopDir, "src-tauri")
	markerPath := filepath.Join(cargoTargetDir(ctx.RootDir), ".bindings-fresh-marker")

	original, err := os.ReadFile(bindingsPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", bindingsPath, err)
	}

	inputHash, hashErr := hashBindingsInputs(rustDir)
	bindingsSha := sha256Bytes(original)

	if hashErr == nil && matchesBindingsMarker(markerPath, inputHash, bindingsSha) {
		return Success(fmt.Sprintf("bindings.ts in sync (%d lines, cached)", bytes.Count(original, []byte{'\n'}))), nil
	}

	// Ensure llama-server resources exist before the test-mode build kicks off.
	// On the warm path this is a marker-file check (sub-ms); on a fresh worktree
	// it downloads ~28 MB from GitHub once. Without it, the cargo build fails on
	// the `resources/ai/*` glob in src-tauri/build.rs.
	downloadCmd := exec.Command("go", "run", "scripts/download-llama-server.go")
	downloadCmd.Dir = desktopDir
	if output, err := RunCommand(downloadCmd, true); err != nil {
		return CheckResult{}, fmt.Errorf("failed to prepare llama-server binaries\n%s", indentOutput(output))
	}

	// Always restore before returning, even on regen failure.
	defer func() {
		_ = os.WriteFile(bindingsPath, original, 0o644)
	}()

	regenCmd := exec.Command("pnpm", "bindings:regen")
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		return CheckResult{}, fmt.Errorf("`pnpm bindings:regen` failed:\n%s", indentOutput(output))
	}

	regenerated, err := os.ReadFile(bindingsPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read regenerated bindings: %w", err)
	}

	if string(regenerated) != string(original) {
		return CheckResult{}, fmt.Errorf(
			"bindings.ts is stale. Run `pnpm bindings:regen` from `apps/desktop/` and commit the diff",
		)
	}

	if hashErr == nil {
		_ = writeBindingsMarker(markerPath, inputHash, bindingsSha)
	}

	return Success(fmt.Sprintf("bindings.ts in sync (%d lines)", bytes.Count(original, []byte{'\n'}))), nil
}

// hashBindingsInputs returns a stable hash of every input that could affect
// the generated bindings: all `.rs` files under `src-tauri/src` plus
// `Cargo.lock` and `Cargo.toml`. Hashing all source files (rather than only
// those with `#[tauri::command]` / `specta::Type`) costs ~tens of ms here and
// removes any "we added the attr to a new file but the watch list didn't pick
// it up" footgun.
func hashBindingsInputs(rustDir string) (string, error) {
	srcDir := filepath.Join(rustDir, "src")

	var paths []string
	err := filepath.Walk(srcDir, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if !info.Mode().IsRegular() {
			return nil
		}
		if strings.HasSuffix(path, ".rs") {
			paths = append(paths, path)
		}
		return nil
	})
	if err != nil {
		return "", err
	}
	paths = append(paths, filepath.Join(rustDir, "Cargo.lock"), filepath.Join(rustDir, "Cargo.toml"))
	sort.Strings(paths)

	h := sha256.New()
	for _, p := range paths {
		rel, _ := filepath.Rel(rustDir, p)
		// Include the relative path so adding/removing files changes the hash.
		_, _ = io.WriteString(h, rel+"\x00")
		f, err := os.Open(p)
		if err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return "", err
		}
		if _, err := io.Copy(h, f); err != nil {
			_ = f.Close()
			return "", err
		}
		_ = f.Close()
		_, _ = io.WriteString(h, "\x00")
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func sha256Bytes(b []byte) string {
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

func matchesBindingsMarker(markerPath, inputHash, bindingsSha string) bool {
	data, err := os.ReadFile(markerPath)
	if err != nil {
		return false
	}
	parts := strings.Fields(strings.TrimSpace(string(data)))
	if len(parts) != 2 {
		return false
	}
	return parts[0] == inputHash && parts[1] == bindingsSha
}

func writeBindingsMarker(markerPath, inputHash, bindingsSha string) error {
	if err := os.MkdirAll(filepath.Dir(markerPath), 0o755); err != nil {
		return err
	}
	return os.WriteFile(markerPath, []byte(inputHash+" "+bindingsSha+"\n"), 0o644)
}

// cargoTargetDir returns the cargo target directory for this workspace: the
// `CARGO_TARGET_DIR` env var if set, otherwise `<rootDir>/target`. Matches the
// path cargo would use, so anything dropped here gets cleaned by `cargo clean`
// and survives or vanishes alongside cargo's own artifacts.
func cargoTargetDir(rootDir string) string {
	if dir := os.Getenv("CARGO_TARGET_DIR"); dir != "" {
		return dir
	}
	return filepath.Join(rootDir, "target")
}
