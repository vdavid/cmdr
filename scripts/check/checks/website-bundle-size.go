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

// Website bundle-size growth warning (warn-only, never fails): compares the
// built `apps/website/dist/` total size against a committed baseline and
// warns when the total grows past the budget. Mirrors file-length's
// allowlist discipline: the baseline only ratchets down automatically (local
// runs); raising it is a deliberate manual act (delete the baseline file and
// re-run the check to regenerate it, with David's OK).
const (
	// websiteBundleGrowthWarnPct is the growth budget: warn when dist/ total
	// exceeds the baseline by more than this. The same percentage is the
	// downward ratchet band (shrink past it and a local run rewrites the
	// baseline), mirroring file-length's symmetric buffer.
	websiteBundleGrowthWarnPct = 10

	// websiteBundleTopAssetCount is how many of the largest assets the
	// baseline records and the warn message lists.
	websiteBundleTopAssetCount = 10
)

// websiteBundleBaseline is the on-disk shape of
// website-bundle-size-baseline.json.
type websiteBundleBaseline struct {
	Comment    string `json:"$comment,omitempty"`
	TotalBytes int64  `json:"totalBytes"`
	// TopAssets maps hash-normalized asset paths (see normalizeAssetName) to
	// bytes, for the largest assets at baseline time. Informational: the warn
	// trigger is the total, the per-asset deltas point at what grew.
	TopAssets map[string]int64 `json:"topAssets,omitempty"`
}

func websiteBundleBaselinePath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", "website-bundle-size-baseline.json")
}

const websiteBundleBaselineComment = "Baseline for the website-bundle-size check (warn-only). " +
	"Asset names are content-hash-normalized (About.DvK3R9p1.css → About.*.css) so rebuilds compare stably. " +
	"A local run ratchets totalBytes down when dist/ shrinks; raising it needs David's OK: " +
	"delete this file and run `pnpm check bundle-size` against a fresh build to regenerate."

// astroContentHashRE matches the content-hash segment Astro/Vite inject into
// emitted asset names (`About.DvK3R9p1.css`): exactly eight base64url chars
// between two dots, at the end of the name right before the extension. Eight
// chars with a mixed-case/digit requirement (checked separately) keeps
// version-ish segments like `favicon.16.png` untouched.
var astroContentHashRE = regexp.MustCompile(`\.([A-Za-z0-9_-]{8})(\.[a-z0-9]+)$`)

// normalizeAssetName replaces the content-hash segment of a built asset path
// with `*`, so the same logical asset keeps one identity across rebuilds.
func normalizeAssetName(relPath string) string {
	m := astroContentHashRE.FindStringSubmatch(relPath)
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

type websiteDistScan struct {
	totalBytes int64
	fileCount  int
	// assets maps normalized asset paths to bytes; files sharing a normalized
	// name (chunks of one logical asset under different hashes) sum up.
	assets map[string]int64
}

// scanWebsiteDist walks the built dist/ directory and totals file sizes.
func scanWebsiteDist(distDir string) (websiteDistScan, error) {
	scan := websiteDistScan{assets: map[string]int64{}}
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
		scan.assets[normalizeAssetName(filepath.ToSlash(relPath))] += info.Size()
		return nil
	})
	return scan, err
}

// topWebsiteAssets returns the n largest assets as a map.
func topWebsiteAssets(assets map[string]int64, n int) map[string]int64 {
	keys := sortedKeys(assets)
	sort.SliceStable(keys, func(i, j int) bool { return assets[keys[i]] > assets[keys[j]] })
	top := map[string]int64{}
	for _, key := range keys[:min(n, len(keys))] {
		top[key] = assets[key]
	}
	return top
}

// RunWebsiteBundleSize compares the built website's dist/ size to the
// committed baseline. Warn-only: growth past the budget reports a warning and
// never fails the suite. Self-skips when dist/ is absent (run website-build
// first), like website-html-validate.
func RunWebsiteBundleSize(ctx *CheckContext) (CheckResult, error) {
	distDir := filepath.Join(ctx.RootDir, "apps", "website", "dist")
	if _, err := os.Stat(distDir); os.IsNotExist(err) {
		return Skipped("dist/ not found (run website-build first)"), nil
	}

	scan, err := scanWebsiteDist(distDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan dist/: %w", err)
	}

	baselinePath := websiteBundleBaselinePath(ctx.RootDir)
	data, err := os.ReadFile(baselinePath)
	if os.IsNotExist(err) {
		return createWebsiteBundleBaseline(ctx, scan, baselinePath)
	}
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to read baseline: %w", err)
	}
	var baseline websiteBundleBaseline
	if err := json.Unmarshal(data, &baseline); err != nil {
		return CheckResult{}, fmt.Errorf("failed to parse %s: %w", baselinePath, err)
	}

	warnCeiling := baseline.TotalBytes * (100 + websiteBundleGrowthWarnPct) / 100
	ratchetFloor := baseline.TotalBytes * (100 - websiteBundleGrowthWarnPct) / 100
	growthPct := float64(scan.totalBytes-baseline.TotalBytes) * 100 / float64(baseline.TotalBytes)

	switch {
	case scan.totalBytes > warnCeiling:
		msg := fmt.Sprintf("dist/ grew %+.1f%% over baseline: %s vs %s (warn-only)\nLargest assets:",
			growthPct, formatBundleBytes(scan.totalBytes), formatBundleBytes(baseline.TotalBytes))
		for _, line := range describeTopAssets(scan.assets, baseline.TopAssets) {
			msg += "\n  - " + line
		}
		msg += "\nIf the growth is intended, refresh the baseline: delete scripts/check/checks/website-bundle-size-baseline.json and run `pnpm check bundle-size` (raising it needs David's OK)."
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: -1, Changes: -1}, nil

	case scan.totalBytes < ratchetFloor && ctx.CI:
		msg := fmt.Sprintf("dist/ total %s is well under the %s baseline; a local run ratchets the baseline down",
			formatBundleBytes(scan.totalBytes), formatBundleBytes(baseline.TotalBytes))
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: -1, Changes: -1}, nil

	case scan.totalBytes < ratchetFloor:
		if err := writeWebsiteBundleBaseline(ctx, scan, baselinePath); err != nil {
			return CheckResult{}, err
		}
		return SuccessWithChanges(fmt.Sprintf("dist/ shrank to %s; ratcheted baseline down from %s",
			formatBundleBytes(scan.totalBytes), formatBundleBytes(baseline.TotalBytes))), nil

	default:
		return Success(fmt.Sprintf("dist/ total %s across %d files (baseline %s, %+.1f%%)",
			formatBundleBytes(scan.totalBytes), scan.fileCount, formatBundleBytes(baseline.TotalBytes), growthPct)), nil
	}
}

// createWebsiteBundleBaseline handles the missing-baseline case: local runs
// generate it (the deliberate refresh path), CI warns that none is committed.
func createWebsiteBundleBaseline(ctx *CheckContext, scan websiteDistScan, baselinePath string) (CheckResult, error) {
	if ctx.CI {
		msg := "no committed baseline (scripts/check/checks/website-bundle-size-baseline.json); run `pnpm check bundle-size` locally after a build to create it"
		return CheckResult{Code: ResultWarning, Message: msg, Total: -1, Issues: -1, Changes: -1}, nil
	}
	if err := writeWebsiteBundleBaseline(ctx, scan, baselinePath); err != nil {
		return CheckResult{}, err
	}
	return SuccessWithChanges(fmt.Sprintf("created baseline: dist/ total %s across %d files",
		formatBundleBytes(scan.totalBytes), scan.fileCount)), nil
}

func writeWebsiteBundleBaseline(ctx *CheckContext, scan websiteDistScan, baselinePath string) error {
	baseline := websiteBundleBaseline{
		Comment:    websiteBundleBaselineComment,
		TotalBytes: scan.totalBytes,
		TopAssets:  topWebsiteAssets(scan.assets, websiteBundleTopAssetCount),
	}
	if err := writeJSONAllowlist(baselinePath, baseline); err != nil {
		return err
	}
	reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/website-bundle-size-baseline.json")
	return nil
}

// describeTopAssets renders the current largest assets with their baseline
// size where known, so the warn message points at what grew.
func describeTopAssets(assets, baselineAssets map[string]int64) []string {
	top := topWebsiteAssets(assets, websiteBundleTopAssetCount)
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
