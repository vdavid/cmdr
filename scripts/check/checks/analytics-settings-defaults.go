package checks

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

// The manifest the analytics dashboard resolves absent heartbeat config keys against,
// and the generator that owns it.
const (
	settingsDefaultsManifest  = "apps/analytics-dashboard/src/lib/server/settings-defaults.gen.json"
	settingsDefaultsGenerator = "scripts/gen-analytics-defaults.ts"
)

// settingsDefaultsFile is the committed manifest, as much of it as this check reads.
type settingsDefaultsFile struct {
	Versions map[string]map[string]any `json:"versions"`
	Next     map[string]any            `json:"next"`
}

// RunAnalyticsSettingsDefaults keeps the settings-defaults manifest pinned to the settings
// registry, and keeps its version history honest.
//
// Why it's worth a check. `settings.json` persists only what a user explicitly changed, so the
// heartbeat's config shape carries deviation and never adoption. The dashboard fills the gap by
// resolving an absent key against the defaults that shipped in that install's app version, which
// works only while the manifest still describes the registry. Let it rot and nothing breaks
// loudly: every "how many people use X" number just quietly becomes wrong, including the ones
// already screenshotted into a decision.
//
// Two things are asserted:
//
//   - `next` matches the working tree. Same regenerate-and-diff shape as `native-strings-fresh`:
//     outside `--ci` the rewrite is kept so the fix is already staged, and in `--ci` the original is
//     restored and drift fails.
//   - A release never shipped defaults nobody recorded. `release.sh` promotes `next` into
//     `versions` under the new version number; if that step is skipped, the newest `versions` entry
//     falls behind `package.json` while `next` has already moved on, and the installs running that
//     release get resolved against a predecessor's defaults.
func RunAnalyticsSettingsDefaults(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	manifestPath := filepath.Join(ctx.RootDir, settingsDefaultsManifest)

	original, err := os.ReadFile(manifestPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", settingsDefaultsManifest, err)
	}

	if ctx.CI {
		defer func() {
			_ = os.WriteFile(manifestPath, original, 0o644)
		}()
	}

	regenCmd := exec.Command("node", settingsDefaultsGenerator)
	regenCmd.Dir = desktopDir
	output, regenErr := RunCommand(regenCmd, true)
	if regenErr != nil {
		if !ctx.CI {
			_ = os.WriteFile(manifestPath, original, 0o644)
		}
		return CheckResult{}, fmt.Errorf("`node %s` failed:\n%s", settingsDefaultsGenerator, indentOutput(output))
	}

	regenerated, err := os.ReadFile(manifestPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read the regenerated manifest: %w", err)
	}
	changed := !bytes.Equal(regenerated, original)

	if ctx.CI && changed {
		return CheckResult{}, fmt.Errorf(
			"the settings-defaults manifest is stale: a setting was added, removed, or had its default changed "+
				"without regenerating %s. Run `node %s` from `apps/desktop/` and commit the rewrite. Until it's "+
				"regenerated, the dashboard resolves absent config keys against the wrong defaults and every "+
				"adoption number for this release is wrong",
			settingsDefaultsManifest, settingsDefaultsGenerator)
	}

	var manifest settingsDefaultsFile
	if err := json.Unmarshal(regenerated, &manifest); err != nil {
		return CheckResult{}, fmt.Errorf("couldn't parse %s: %w", settingsDefaultsManifest, err)
	}
	released, err := desktopPackageVersion(desktopDir)
	if err != nil {
		return CheckResult{}, err
	}
	if problem := unrecordedRelease(manifest, released); problem != "" {
		return CheckResult{}, fmt.Errorf("%s", problem)
	}

	summary := fmt.Sprintf("%d setting defaults across %d version %s",
		len(manifest.Next), len(manifest.Versions), Pluralize(len(manifest.Versions), "entry", "entries"))
	if changed {
		return SuccessWithChanges(summary + ", manifest regenerated"), nil
	}
	return Success(summary), nil
}

// unrecordedRelease reports the release whose defaults were never written into the manifest, or
// "" when the history is intact.
//
// The rule: an entry is written only where a release actually changed a default, so `next` equal to
// the newest entry means nothing has moved since that entry and any later release is faithfully
// described by it. Once `next` differs, the newest entry has to be at least the last released
// version, because otherwise that release shipped a state no entry describes.
func unrecordedRelease(manifest settingsDefaultsFile, released string) string {
	newest := ""
	for version := range manifest.Versions {
		if newest == "" || compareVersions(version, newest) > 0 {
			newest = version
		}
	}
	if newest == "" {
		return fmt.Sprintf("%s has no version entries at all, so the dashboard can't resolve any install's "+
			"defaults. Rebuild it with `node %s --backfill` from `apps/desktop/`",
			settingsDefaultsManifest, settingsDefaultsGenerator)
	}
	if unreleased := versionsAbove(manifest.Versions, released); len(unreleased) > 0 {
		return fmt.Sprintf("%s records %s, which %s newer than the released v%s. Entries describe SHIPPED "+
			"releases; an entry for an unreleased version would be resolved against installs that never ran it",
			settingsDefaultsManifest, strings.Join(unreleased, ", "),
			Pluralize(len(unreleased), "is", "are"), released)
	}
	if snapshotsMatch(manifest.Next, manifest.Versions[newest]) || compareVersions(newest, released) >= 0 {
		return ""
	}
	return fmt.Sprintf("v%s shipped settings defaults that %s never recorded: its newest entry is v%s, and the "+
		"working tree has moved on since. Installs running v%s are being resolved against v%s's defaults. Run "+
		"`node %s --promote %s` from `apps/desktop/`, which is what `scripts/release.sh` does for a normal release",
		released, settingsDefaultsManifest, newest, released, newest, settingsDefaultsGenerator, released)
}

// versionsAbove lists the manifest entries newer than the last released version, sorted.
func versionsAbove(versions map[string]map[string]any, released string) []string {
	var above []string
	for version := range versions {
		if compareVersions(version, released) > 0 {
			above = append(above, "v"+version)
		}
	}
	sort.Slice(above, func(i, j int) bool { return compareVersions(above[i][1:], above[j][1:]) < 0 })
	return above
}

// snapshotsMatch compares two default snapshots by their JSON encoding, which is stable because the
// generator writes keys sorted.
func snapshotsMatch(a, b map[string]any) bool {
	encodedA, errA := json.Marshal(a)
	encodedB, errB := json.Marshal(b)
	return errA == nil && errB == nil && bytes.Equal(encodedA, encodedB)
}

// desktopPackageVersion reads the last released version off the desktop `package.json`, which
// `release.sh` bumps as part of cutting a release.
func desktopPackageVersion(desktopDir string) (string, error) {
	data, err := os.ReadFile(filepath.Join(desktopDir, "package.json"))
	if err != nil {
		return "", fmt.Errorf("couldn't read apps/desktop/package.json: %w", err)
	}
	var pkg struct {
		Version string `json:"version"`
	}
	if err := json.Unmarshal(data, &pkg); err != nil {
		return "", fmt.Errorf("couldn't parse apps/desktop/package.json: %w", err)
	}
	if pkg.Version == "" {
		return "", fmt.Errorf("apps/desktop/package.json has no version field")
	}
	return pkg.Version, nil
}

// compareVersions orders `MAJOR.MINOR.PATCH` numerically. Negative when a < b.
func compareVersions(a, b string) int {
	partsA, partsB := strings.Split(a, "."), strings.Split(b, ".")
	for i := 0; i < len(partsA) || i < len(partsB); i++ {
		if diff := versionPart(partsA, i) - versionPart(partsB, i); diff != 0 {
			return diff
		}
	}
	return 0
}

func versionPart(parts []string, index int) int {
	if index >= len(parts) {
		return 0
	}
	value, err := strconv.Atoi(parts[index])
	if err != nil {
		return 0
	}
	return value
}
