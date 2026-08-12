package checks

import (
	"errors"
	"fmt"
	"os"
	"strings"
)

// The Playwright E2E lane compiles a release Tauri binary before it can run a
// single spec, which costs 2-3 minutes. The lane's own `Inputs` cover the whole
// desktop app, so it re-runs whenever anything under `apps/desktop/**` changes —
// including the specs themselves, which Playwright reads from disk at run time and
// which no compiler ever sees. A debugging loop that edits one spec and re-runs the
// suite therefore paid a full rebuild per iteration.
//
// So the build gets its own, narrower fingerprint, stamped beside the binary it
// produced. Same mechanism as the check-level cache (`fingerprint.go`), one level
// down: `e2eBinaryInputs` instead of the lane's `Inputs`, a file beside the binary
// instead of the cache JSON.

// e2eBuildStampSuffix names the stamp file's relation to the binary rather than
// giving it a fixed name, so the stamp shares the binary's fate: a `cargo clean` or
// a wiped `target/` takes both, and a rebuild can never inherit a stamp describing
// a binary that no longer exists.
const e2eBuildStampSuffix = ".build-fingerprint"

// e2eBinaryInputs is everything `pnpm test:e2e:playwright:build` compiles into the
// binary: the Rust workspace that cargo builds and the Svelte frontend that Vite
// bundles into it, plus the configs and lockfiles that decide what either produces.
//
// Deliberately NOT `apps/desktop/test/**`. Playwright's specs, its config, and the
// shared fixture helpers are read from disk when the suite runs; editing one changes
// what the suite ASSERTS, never what it asserts against. `apps/desktop/test/smb-servers/**`
// is the same: container configs the running suite talks to.
//
// Conservative everywhere else, on the cache's usual policy — too wide only costs a
// rebuild, too narrow runs the whole suite against a binary that no longer matches
// the tree. `TestE2EBinaryInputsCoverTheBuildAndNothingElse` pins both directions.
func e2eBinaryInputs() []string {
	return inputs([]string{
		// Vite's side: the frontend `beforeBuildCommand` bundles into the binary.
		"apps/desktop/src/**",
		"apps/desktop/static/**",
		"apps/desktop/package.json",
		"apps/desktop/svelte.config.js",
		"apps/desktop/vite.config.js",
		"apps/desktop/tsconfig.json",
		// The wrapper and the model downloader that drive the build itself.
		"apps/desktop/scripts/**",
		// Cargo's side.
		"apps/desktop/src-tauri/**",
		"crates/**",
		"tools/**",
		"Cargo.toml",
		"Cargo.lock",
		"rust-toolchain.toml",
		"pnpm-lock.yaml",
		// `whats_new` pulls the changelog in with `include_str!`.
		"CHANGELOG.md",
	}, agentDocExclusions)
}

// e2eBuildFingerprint hashes the tree the binary would be built from, using the
// same git-aware pass every check's fingerprint uses.
func e2eBuildFingerprint(rootDir string) (string, error) {
	data, err := CollectRepoFingerprintData(rootDir)
	if err != nil {
		return "", err
	}
	def := CheckDefinition{Inputs: e2eBinaryInputs()}
	return data.FingerprintFor(&def), nil
}

// e2eBinaryIsCurrent reports whether the binary on disk is the one this exact
// fingerprint was stamped onto. Every uncertainty answers "no": a missing binary, a
// missing or unreadable stamp, and an empty fingerprint (which is what a failed
// fingerprint pass hands over) all mean rebuild. The expensive answer is the safe
// one here, because this lane carries `NotInCI` — nothing downstream would catch a
// suite that passed against a stale binary.
func e2eBinaryIsCurrent(binaryPath, fingerprint string) bool {
	if fingerprint == "" {
		return false
	}
	identity, err := e2eBuildStampFor(binaryPath, fingerprint)
	if err != nil {
		return false
	}
	stamped, err := os.ReadFile(binaryPath + e2eBuildStampSuffix)
	if err != nil {
		return false
	}
	return strings.TrimSpace(string(stamped)) == identity
}

// recordE2EBuild stamps a freshly built binary with the fingerprint it was built
// from. Call it only after the build succeeded: a stamp is a claim that the binary
// beside it matches the tree.
func recordE2EBuild(binaryPath, fingerprint string) error {
	if fingerprint == "" {
		return errors.New("refusing to stamp an E2E binary with an empty fingerprint")
	}
	identity, err := e2eBuildStampFor(binaryPath, fingerprint)
	if err != nil {
		return err
	}
	return os.WriteFile(binaryPath+e2eBuildStampSuffix, []byte(identity+"\n"), 0o644)
}

// e2eBuildStampFor is the stamp's content: the tree fingerprint plus the binary's
// own size and modification time.
//
// The fingerprint alone would vouch for whatever file happens to sit at that path,
// and it isn't ours exclusively: a plain `pnpm tauri build` in the same worktree
// writes the same `target/<triple>/release/Cmdr` without the `playwright-e2e`
// feature. Binding the stamp to the file's identity means anything that replaces
// the binary invalidates the stamp, so the swap costs a rebuild rather than a run
// against a binary the E2E harness can't drive.
func e2eBuildStampFor(binaryPath, fingerprint string) (string, error) {
	info, err := os.Stat(binaryPath)
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("%s %d %d", fingerprint, info.Size(), info.ModTime().UnixNano()), nil
}
