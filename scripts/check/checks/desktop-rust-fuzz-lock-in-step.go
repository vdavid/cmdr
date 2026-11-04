package checks

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// RunFuzzLockInStep fails when a crate both lockfiles resolve sits at a different
// version in `fuzz/Cargo.lock` than in the root `Cargo.lock`.
//
// `fuzz/` lives outside the workspace, so its lockfile is its own and nothing
// (Renovate included) moves it when the root one moves. A lagging copy makes the
// lane fuzz code the app doesn't ship: `twox-hash` 2.1.2 there against the root's
// 2.1.5 crashed the `adb_lz4` lane on an overflow fixed upstream in 2.1.3
// (`fuzz/DETAILS.md` § Findings). Ahead is wrong the same way.
//
// Versions compare per semver line, because a crate can legitimately resolve on
// two lines at once (`zip` 4 under `tauri-plugin-updater` beside the vendored
// `zip` 9). A line only one lock has passes: the fuzz crate's own dependencies
// (`libfuzzer-sys`, `arbitrary`) or app code no target links pull it, and there's
// no root version to be in step with.
func RunFuzzLockInStep(ctx *CheckContext) (CheckResult, error) {
	fuzzPath := filepath.Join(ctx.RootDir, "fuzz", "Cargo.lock")
	if _, err := os.Stat(fuzzPath); errors.Is(err, fs.ErrNotExist) {
		return Skipped("no fuzz/Cargo.lock to compare"), nil
	}
	root, err := lockedVersionsByLine(filepath.Join(ctx.RootDir, "Cargo.lock"))
	if err != nil {
		return CheckResult{}, err
	}
	fuzz, err := lockedVersionsByLine(fuzzPath)
	if err != nil {
		return CheckResult{}, err
	}

	names := make([]string, 0, len(fuzz))
	for name := range fuzz {
		if _, shared := root[name]; shared {
			names = append(names, name)
		}
	}
	sort.Strings(names)

	var problems []string
	for _, name := range names {
		fuzzVersionCount := 0
		for _, versions := range fuzz[name] {
			fuzzVersionCount += len(versions)
		}
		for line, fuzzVersions := range fuzz[name] {
			rootVersions, ok := root[name][line]
			if !ok {
				continue
			}
			for _, fuzzVersion := range fuzzVersions {
				if slices.Contains(rootVersions, fuzzVersion) {
					continue
				}
				want := highestVersion(rootVersions)
				spec := name
				if fuzzVersionCount > 1 {
					spec = name + "@" + fuzzVersion
				}
				problems = append(problems, fmt.Sprintf(
					"%s is %s in fuzz/Cargo.lock but %s in Cargo.lock. Fix: cd fuzz && cargo update -p %s --precise %s",
					name, fuzzVersion, strings.Join(rootVersions, ", "), spec, want))
			}
		}
	}
	sort.Strings(problems)

	if len(problems) > 0 {
		return CheckResult{}, fmt.Errorf("fuzz/Cargo.lock fell out of step with the root Cargo.lock, so the fuzz "+
			"lane tests different code than the app ships\n%s", indentOutput(strings.Join(problems, "\n")))
	}
	return Success(fmt.Sprintf("%s %s the root lock", FormatThousands(len(names)),
		Pluralize(len(names), "shared crate matches", "shared crates match"))), nil
}

// lockedVersionsByLine reads a Cargo.lock into crate name → semver line → versions.
func lockedVersionsByLine(path string) (map[string]map[string][]string, error) {
	var lock cargoLock
	if _, err := toml.DecodeFile(path, &lock); err != nil {
		return nil, fmt.Errorf("couldn't read %s: %w", path, err)
	}
	out := map[string]map[string][]string{}
	for _, pkg := range lock.Package {
		if out[pkg.Name] == nil {
			out[pkg.Name] = map[string][]string{}
		}
		line := semverLine(pkg.Version)
		if !slices.Contains(out[pkg.Name][line], pkg.Version) {
			out[pkg.Name][line] = append(out[pkg.Name][line], pkg.Version)
		}
	}
	return out, nil
}

func highestVersion(versions []string) string {
	best := versions[0]
	for _, v := range versions[1:] {
		if compareVersions(v, best) > 0 {
			best = v
		}
	}
	return best
}
