package checks

import (
	"path/filepath"
)

// Website bundle-size growth warning (warn-only, never fails): compares the
// built `apps/website/dist/` total against a committed baseline and warns when
// it grows past the budget. The machinery is shared with the desktop lane; see
// `bundle-size-baseline.go` for the ratchet discipline.

const websiteBundleBaselineRel = "scripts/check/checks/website-bundle-size-baseline.json"

const websiteBundleBaselineComment = "Baseline for the website-bundle-size check (warn-only). " +
	"Asset names are content-hash-normalized (About.DvK3R9p1.css → About.*.css) so rebuilds compare stably. " +
	"A local run ratchets totalBytes down when dist/ shrinks; raising it needs David's OK: " +
	"delete this file and run `pnpm check bundle-size` against a fresh build to regenerate."

func websiteBundleSpec(rootDir string) bundleSizeSpec {
	return bundleSizeSpec{
		label:       "dist/",
		distDir:     filepath.Join(rootDir, "apps", "website", "dist"),
		baselineRel: websiteBundleBaselineRel,
		comment:     websiteBundleBaselineComment,
		refreshCmd:  "pnpm check bundle-size",
		missingHint: "dist/ not found (run website-build first)",
	}
}

// RunWebsiteBundleSize compares the built website's dist/ size to the committed
// baseline. Self-skips when dist/ is absent (run website-build first), like
// website-html-validate.
func RunWebsiteBundleSize(ctx *CheckContext) (CheckResult, error) {
	return runBundleSizeCheck(ctx, websiteBundleSpec(ctx.RootDir))
}
