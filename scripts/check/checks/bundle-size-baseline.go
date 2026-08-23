package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Shared machinery for the bundle-size gauges (`website-bundle-size`,
// `desktop-bundle-size`): walk a built output directory, total it, compare
// against a committed baseline, and ratchet that baseline DOWN on a local run.
//
// Warn-only by design, and the ratchet is one-directional: a bundle may shrink
// freely, and growth past the budget is a warning nobody can silence by editing
// a number (raising a baseline means deleting it and regenerating, with David's
// OK, per `.claude/rules/file-length-allowlist.md`).
//
// One implementation rather than one per app: the two lanes differ only in which
// directory they measure, what produces it, and what the message tells you to
// run. Those are the `bundleSizeSpec` fields; everything else is here.

const (
	// bundleGrowthWarnPct is the growth budget: warn when the total exceeds the
	// baseline by more than this. The same percentage is the downward ratchet
	// band (shrink past it and a local run rewrites the baseline), mirroring
	// file-length's symmetric buffer.
	bundleGrowthWarnPct = 10

	// bundleTopAssetCount is how many of the largest assets the baseline records
	// and a warn message lists.
	bundleTopAssetCount = 10
)

// bundleBaseline is the on-disk shape of a `*-bundle-size-baseline.json`.
type bundleBaseline struct {
	Comment    string `json:"$comment,omitempty"`
	TotalBytes int64  `json:"totalBytes"`
	// TopAssets maps hash-normalized asset paths (see normalizeAssetName) to
	// bytes, for the largest assets at baseline time. Informational: the warn
	// trigger is the total, the per-asset deltas point at what grew.
	TopAssets map[string]int64 `json:"topAssets,omitempty"`
}

// bundleSizeSpec is everything that differs between one bundle-size lane and
// another.
type bundleSizeSpec struct {
	// label names the measured directory in messages ("dist/", "build/").
	label string
	// distDir is the absolute path of the built output to walk.
	distDir string
	// baselineRel is the baseline's repo-relative path, used both to read/write
	// it and to name it in the refresh instruction.
	baselineRel string
	// comment is the `$comment` written into the baseline file.
	comment string
	// refreshCmd is the check invocation that regenerates the baseline.
	refreshCmd string
	// prepare, when set, produces distDir (a build step). A lane whose output is
	// produced by another check leaves this nil and self-skips instead.
	prepare func(ctx *CheckContext) error
	// missingHint tells a local run how to produce distDir when prepare is nil.
	missingHint string
	// normalize maps a built asset's relative path to the stable identity the
	// baseline records. nil means `normalizeAssetName`, which handles the
	// `name.hash.ext` shape; a lane whose emitter names files differently
	// supplies its own.
	normalize func(relPath string) string
}

// contentHashRE matches the content-hash segment Astro/Vite inject into emitted
// asset names (`About.DvK3R9p1.css`): exactly eight base64url chars between two
// dots, at the end of the name right before the extension. Eight chars with a
// mixed-case/digit requirement (checked separately) keeps version-ish segments
// like `favicon.16.png` untouched.
var contentHashRE = regexp.MustCompile(`\.([A-Za-z0-9_-]{8})(\.[a-z0-9]+)$`)

// normalizeAssetName replaces the content-hash segment of a built asset path
// with `*`, so the same logical asset keeps one identity across rebuilds.
func normalizeAssetName(relPath string) string {
	m := contentHashRE.FindStringSubmatch(relPath)
	if m == nil {
		return relPath
	}
	// Require at least one letter in the hash candidate: a purely numeric
	// segment (favicon.16.png style sizes) is not a content hash.
	if !strings.ContainsAny(m[1], "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ") {
		return relPath
	}
	return relPath[:len(relPath)-len(m[0])] + ".*" + m[2]
}

type bundleScan struct {
	totalBytes int64
	fileCount  int
	// assets maps normalized asset paths to bytes; files sharing a normalized
	// name (chunks of one logical asset under different hashes) sum up.
	assets map[string]int64
}

// scanBundleDir walks a built output directory and totals file sizes, giving
// each file the stable identity `normalize` assigns it.
func scanBundleDir(distDir string, normalize func(string) string) (bundleScan, error) {
	if normalize == nil {
		normalize = normalizeAssetName
	}
	scan := bundleScan{assets: map[string]int64{}}
	err := filepath.WalkDir(distDir, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		relPath, err := filepath.Rel(distDir, path)
		if err != nil {
			return err
		}
		scan.totalBytes += info.Size()
		scan.fileCount++
		scan.assets[normalize(filepath.ToSlash(relPath))] += info.Size()
		return nil
	})
	return scan, err
}

// topBundleAssets returns the n largest assets as a map.
func topBundleAssets(assets map[string]int64, n int) map[string]int64 {
	keys := sortedKeys(assets)
	sort.SliceStable(keys, func(i, j int) bool { return assets[keys[i]] > assets[keys[j]] })
	top := map[string]int64{}
	for _, key := range keys[:min(n, len(keys))] {
		top[key] = assets[key]
	}
	return top
}

// describeTopAssets renders the current largest assets with their baseline size
// where known, so a warn message points at what grew.
func describeTopAssets(assets, baselineAssets map[string]int64) []string {
	top := topBundleAssets(assets, bundleTopAssetCount)
	keys := sortedKeys(top)
	sort.SliceStable(keys, func(i, j int) bool { return top[keys[i]] > top[keys[j]] })
	lines := make([]string, 0, len(keys))
	for _, key := range keys {
		suffix := "(new since baseline)"
		if baseBytes, ok := baselineAssets[key]; ok {
			suffix = fmt.Sprintf("(baseline %s)", formatBundleBytes(baseBytes))
		}
		lines = append(lines, fmt.Sprintf("%s %s %s", key, formatBundleBytes(top[key]), suffix))
	}
	return lines
}

// formatBundleBytes renders byte counts as B / kB / MB with one decimal.
func formatBundleBytes(bytes int64) string {
	switch {
	case bytes >= 1000*1000:
		return fmt.Sprintf("%.1f MB", float64(bytes)/1e6)
	case bytes >= 1000:
		return fmt.Sprintf("%.1f kB", float64(bytes)/1e3)
	default:
		return fmt.Sprintf("%d B", bytes)
	}
}

// runBundleSizeCheck is the whole lane: produce or find the output, scan it,
// and compare against the baseline. Warn-only; it never fails the suite.
func runBundleSizeCheck(ctx *CheckContext, spec bundleSizeSpec) (CheckResult, error) {
	if spec.prepare != nil {
		if err := spec.prepare(ctx); err != nil {
			return CheckResult{}, err
		}
	}
	if _, err := os.Stat(spec.distDir); os.IsNotExist(err) {
		return Skipped(spec.missingHint), nil
	}

	scan, err := scanBundleDir(spec.distDir, spec.normalize)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan %s: %w", spec.label, err)
	}

	baselinePath := filepath.Join(ctx.RootDir, filepath.FromSlash(spec.baselineRel))
	data, err := os.ReadFile(baselinePath)
	if os.IsNotExist(err) {
		return createBundleBaseline(ctx, spec, scan, baselinePath)
	}
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to read baseline: %w", err)
	}
	var baseline bundleBaseline
	if err := json.Unmarshal(data, &baseline); err != nil {
		return CheckResult{}, fmt.Errorf("failed to parse %s: %w", baselinePath, err)
	}

	warnCeiling := baseline.TotalBytes * (100 + bundleGrowthWarnPct) / 100
	ratchetFloor := baseline.TotalBytes * (100 - bundleGrowthWarnPct) / 100
	growthPct := float64(scan.totalBytes-baseline.TotalBytes) * 100 / float64(baseline.TotalBytes)

	switch {
	case scan.totalBytes > warnCeiling:
		msg := fmt.Sprintf("%s grew %+.1f%% over baseline: %s vs %s (warn-only)\nLargest assets:",
			spec.label, growthPct, formatBundleBytes(scan.totalBytes), formatBundleBytes(baseline.TotalBytes))
		for _, line := range describeTopAssets(scan.assets, baseline.TopAssets) {
			msg += "\n  - " + line
		}
		msg += fmt.Sprintf("\nIf the growth is intended, refresh the baseline: delete %s and run `%s`. No need to ask first.",
			spec.baselineRel, spec.refreshCmd)
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: -1, Changes: -1}, nil

	case scan.totalBytes < ratchetFloor && ctx.CI:
		msg := fmt.Sprintf("%s total %s is well under the %s baseline; a local run ratchets the baseline down",
			spec.label, formatBundleBytes(scan.totalBytes), formatBundleBytes(baseline.TotalBytes))
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: -1, Changes: -1}, nil

	case scan.totalBytes < ratchetFloor:
		if err := writeBundleBaseline(ctx, spec, scan, baselinePath); err != nil {
			return CheckResult{}, err
		}
		return SuccessWithChanges(fmt.Sprintf("%s shrank to %s; ratcheted baseline down from %s",
			spec.label, formatBundleBytes(scan.totalBytes), formatBundleBytes(baseline.TotalBytes))), nil

	default:
		return Success(fmt.Sprintf("%s total %s across %d files (baseline %s, %+.1f%%)",
			spec.label, formatBundleBytes(scan.totalBytes), scan.fileCount,
			formatBundleBytes(baseline.TotalBytes), growthPct)), nil
	}
}

// createBundleBaseline handles the missing-baseline case: local runs generate it
// (the deliberate refresh path), CI warns that none is committed.
func createBundleBaseline(ctx *CheckContext, spec bundleSizeSpec, scan bundleScan, baselinePath string) (CheckResult, error) {
	if ctx.CI {
		msg := fmt.Sprintf("no committed baseline (%s); run `%s` locally after a build to create it",
			spec.baselineRel, spec.refreshCmd)
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: -1, Changes: -1}, nil
	}
	if err := writeBundleBaseline(ctx, spec, scan, baselinePath); err != nil {
		return CheckResult{}, err
	}
	return SuccessWithChanges(fmt.Sprintf("created baseline: %s total %s across %d files",
		spec.label, formatBundleBytes(scan.totalBytes), scan.fileCount)), nil
}

func writeBundleBaseline(ctx *CheckContext, spec bundleSizeSpec, scan bundleScan, baselinePath string) error {
	baseline := bundleBaseline{
		Comment:    spec.comment,
		TotalBytes: scan.totalBytes,
		TopAssets:  topBundleAssets(scan.assets, bundleTopAssetCount),
	}
	if err := writeJSONAllowlist(baselinePath, baseline); err != nil {
		return err
	}
	reformatWithOxfmt(ctx.RootDir, spec.baselineRel)
	return nil
}
