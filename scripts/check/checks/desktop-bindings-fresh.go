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
// `.rs` file under every workspace member's `src/`, every member manifest, and
// the workspace root's `Cargo.toml` + `Cargo.lock`) and the
// current `bindings.ts`. If both hashes match the marker from the last
// successful run, skip the regen entirely; that's the common case and turns a
// ~2-minute test-mode compile into a ~50 ms hash scan. Otherwise: run the
// regen and let it overwrite `bindings.ts`. In `--ci` mode we then restore
// the original (CI never modifies the tree) and fail if the regenerated
// content differs. Outside `--ci` we keep the regenerated file so the dev
// gets the same auto-fix UX as `oxfmt` / `gofmt` / clippy `--fix`; the dev
// reviews and commits the diff alongside the Rust change that caused it.
// Marker is updated on success either way.
//
// The marker lives at `<CARGO_TARGET_DIR>/.bindings-fresh-marker` (or
// `<workspace>/target/.bindings-fresh-marker` if the env var is unset), so it
// shares fate with cargo's build artifacts: a `cargo clean` or wholesale
// `target/` deletion auto-invalidates it. Mirrors the
// `node_modules/.pnpm-install-marker` pattern used by `EnsurePnpmDependencies`.
func RunDesktopBindingsFresh(ctx *CheckContext) (CheckResult, error) {
	bindingsPath := filepath.Join(ctx.RootDir, "apps", "desktop", "src", "lib", "ipc", "bindings.ts")
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	markerPath := filepath.Join(cargoTargetDir(ctx.RootDir), ".bindings-fresh-marker")

	// Every member, not just the app: `specta::Type` derives live wherever the data
	// types do, and `ipc.rs` collects them transitively through command signatures,
	// so a type edited in a crate reaches `bindings.ts` all the same.
	members, err := WorkspaceMembers(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	original, err := os.ReadFile(bindingsPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", bindingsPath, err)
	}

	inputHash, hashErr := hashBindingsInputs(ctx.RootDir, members)
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

	// In CI we never modify the working tree: restore on any exit path.
	// Outside CI we restore only on regen failure (so a half-written file
	// can't survive an error), then keep the regenerated content on success.
	if ctx.CI {
		defer func() {
			_ = os.WriteFile(bindingsPath, original, 0o644)
		}()
	}

	regenCmd := exec.Command("pnpm", "bindings:regen")
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		if !ctx.CI {
			_ = os.WriteFile(bindingsPath, original, 0o644)
		}
		return CheckResult{}, fmt.Errorf("`pnpm bindings:regen` failed:\n%s", indentOutput(output))
	}

	regenerated, err := os.ReadFile(bindingsPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read regenerated bindings: %w", err)
	}

	changed := !bytes.Equal(regenerated, original)

	if ctx.CI && changed {
		return CheckResult{}, fmt.Errorf(
			"bindings.ts is stale. Run `pnpm bindings:regen` from `apps/desktop/`",
		)
	}

	if hashErr == nil {
		// Hash the post-regen file so a follow-up run can short-circuit.
		_ = writeBindingsMarker(markerPath, inputHash, sha256Bytes(regenerated))
	}

	lineCount := bytes.Count(regenerated, []byte{'\n'})
	if changed {
		return SuccessWithChanges(fmt.Sprintf("bindings.ts regenerated (%d lines)", lineCount)), nil
	}
	return Success(fmt.Sprintf("bindings.ts in sync (%d lines)", lineCount)), nil
}

// hashBindingsInputs returns a stable hash of every input that could affect the
// generated bindings: all `.rs` files under each workspace member's `src/`, each
// member's manifest, and the workspace root's `Cargo.toml` and `Cargo.lock`.
// Hashing all source files (rather than only those with `#[tauri::command]` /
// `specta::Type`) costs ~tens of ms here and removes any "we added the attr to a new
// file but the watch list didn't pick it up" footgun.
//
// A required input that isn't on disk is an ERROR, not a skip. Skipping is what let
// the lockfile go unhashed: the path contributed its name but never its bytes, so no
// dependency bump ever invalidated the marker and nobody could see that from a green
// run.
func hashBindingsInputs(rootDir string, members []WorkspaceMember) (string, error) {
	// Required: absent means the workspace isn't what we think it is.
	required := []string{
		filepath.Join(rootDir, "Cargo.toml"),
		filepath.Join(rootDir, "Cargo.lock"),
	}
	// Discovered by walking: a file that vanishes mid-walk changes the hash, which
	// is the correct outcome, so those stay tolerant.
	var walked []string

	for _, m := range members {
		required = append(required, m.ManifestPath)
		err := filepath.WalkDir(m.SrcDir, func(path string, d os.DirEntry, walkErr error) error {
			if walkErr != nil {
				return walkErr
			}
			if !d.IsDir() && strings.HasSuffix(path, ".rs") {
				walked = append(walked, path)
			}
			return nil
		})
		if err != nil && !os.IsNotExist(err) {
			return "", err
		}
	}

	requiredSet := make(map[string]bool, len(required))
	for _, p := range required {
		requiredSet[p] = true
	}
	paths := append(append([]string{}, required...), walked...)
	sort.Strings(paths)

	h := sha256.New()
	for _, p := range paths {
		rel, relErr := filepath.Rel(rootDir, p)
		if relErr != nil {
			rel = p
		}
		// Include the relative path so adding/removing files changes the hash.
		_, _ = io.WriteString(h, filepath.ToSlash(rel)+"\x00")
		f, err := os.Open(p)
		if err != nil {
			if os.IsNotExist(err) && !requiredSet[p] {
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
