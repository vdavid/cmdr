package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// The website's baseline refresh script installs pnpm inside the Playwright image with `npm ci`
// from `apps/website/scripts/pnpm-bootstrap/` (that image's Node 26 has no corepack, and `npm ci`
// is hash-pinned where `npm install --global pnpm@<v>` isn't). That manifest is a second pnpm pin
// beside the root `packageManager`, so a bump to one alone would shoot baselines with a pnpm CI
// never runs. Renovate moves both in its `pnpm` group; this check catches a hand edit.
//
// Fails when the manifest's `pnpm` dependency, its lockfile's root entry, or its locked
// `node_modules/pnpm` version differs from `packageManager`.
const (
	pnpmBootstrapRootManifest = "package.json"
	pnpmBootstrapManifest     = "apps/website/scripts/pnpm-bootstrap/package.json"
	pnpmBootstrapLockfile     = "apps/website/scripts/pnpm-bootstrap/package-lock.json"
)

// pnpmBootstrapInputs are the three files the check compares.
var pnpmBootstrapInputs = []string{pnpmBootstrapRootManifest, pnpmBootstrapManifest, pnpmBootstrapLockfile}

func RunWebsitePnpmBootstrapInStep(ctx *CheckContext) (CheckResult, error) {
	var contents [3][]byte
	for i, rel := range pnpmBootstrapInputs {
		raw, err := os.ReadFile(filepath.Join(ctx.RootDir, rel))
		if err != nil {
			return CheckResult{}, fmt.Errorf("read %s: %w", rel, err)
		}
		contents[i] = raw
	}
	want, mismatches, err := pnpmBootstrapMismatches(contents[0], contents[1], contents[2])
	if err != nil {
		return CheckResult{}, err
	}
	if len(mismatches) > 0 {
		return CheckResult{}, fmt.Errorf("the pnpm bootstrap manifest drifted from `packageManager` (pnpm@%s)\n%s", want,
			indentOutput(strings.Join(mismatches, "\n")+
				"\nset the version in "+pnpmBootstrapManifest+", then run `npm install --package-lock-only` in its dir"))
	}
	return Success(fmt.Sprintf("pnpm bootstrap manifest and lockfile pin pnpm@%s, like `packageManager`", want)), nil
}

// pnpmBootstrapMismatches returns the `packageManager` pnpm version and one line per place in the
// bootstrap manifest or lockfile that names a different one.
func pnpmBootstrapMismatches(rootManifest, manifest, lockfile []byte) (string, []string, error) {
	var root struct {
		PackageManager string `json:"packageManager"`
	}
	if err := json.Unmarshal(rootManifest, &root); err != nil {
		return "", nil, fmt.Errorf("parse %s: %w", pnpmBootstrapRootManifest, err)
	}
	want, ok := strings.CutPrefix(root.PackageManager, "pnpm@")
	if !ok || want == "" {
		return "", nil, fmt.Errorf("%s's `packageManager` is %q, expected `pnpm@<version>`", pnpmBootstrapRootManifest, root.PackageManager)
	}
	// Corepack accepts a `+sha512.<hash>` suffix; the version is what precedes it.
	want, _, _ = strings.Cut(want, "+")

	var pkg struct {
		Dependencies map[string]string `json:"dependencies"`
	}
	if err := json.Unmarshal(manifest, &pkg); err != nil {
		return "", nil, fmt.Errorf("parse %s: %w", pnpmBootstrapManifest, err)
	}
	var lock struct {
		Packages map[string]struct {
			Version      string            `json:"version"`
			Integrity    string            `json:"integrity"`
			Dependencies map[string]string `json:"dependencies"`
		} `json:"packages"`
	}
	if err := json.Unmarshal(lockfile, &lock); err != nil {
		return "", nil, fmt.Errorf("parse %s: %w", pnpmBootstrapLockfile, err)
	}

	var mismatches []string
	check := func(where, got string) {
		if got != want {
			if got == "" {
				got = "nothing"
			}
			mismatches = append(mismatches, fmt.Sprintf("%s: %s", where, got))
		}
	}
	check(pnpmBootstrapManifest+" `dependencies.pnpm`", pkg.Dependencies["pnpm"])
	check(pnpmBootstrapLockfile+" root `dependencies.pnpm`", lock.Packages[""].Dependencies["pnpm"])
	locked := lock.Packages["node_modules/pnpm"]
	check(pnpmBootstrapLockfile+" `node_modules/pnpm`", locked.Version)
	if locked.Version != "" && locked.Integrity == "" {
		mismatches = append(mismatches, pnpmBootstrapLockfile+" `node_modules/pnpm` has no integrity hash, so `npm ci` wouldn't pin it")
	}
	return want, mismatches, nil
}
