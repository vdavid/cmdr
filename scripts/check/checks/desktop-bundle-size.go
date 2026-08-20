package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
)

// Desktop frontend bundle-size growth warning (warn-only, never fails).
//
// The app embeds the built SvelteKit output, so every byte here rides in the
// `.app`, ships in every silent update, and is parsed at startup before first
// paint. Nothing guarded that until this check existed, which is how the locale
// catalogs came to carry ~2.8 MB of translator metadata nobody could read (see
// `apps/desktop/scripts/vite-strip-catalog-metadata.ts`).
//
// Ratchet discipline and the comparison itself: `bundle-size-baseline.go`.

const desktopBundleBaselineRel = "scripts/check/checks/desktop-bundle-size-baseline.json"

const desktopBundleBaselineComment = "Baseline for the desktop-bundle-size check (warn-only). " +
	"Measures the SvelteKit output the Tauri app embeds, built fresh into a private dir so it never touches apps/desktop/build/. " +
	"Asset names are content-hash-normalized (chunks/AbC12345.js → chunks/*.js) so rebuilds compare stably. " +
	"A local run ratchets totalBytes down when the bundle shrinks; raising it needs David's OK: " +
	"delete this file and run `pnpm check desktop-bundle-size` to regenerate."

// desktopBundleOutDir is where this check builds, deliberately NOT
// `apps/desktop/build/`. That directory is the one the Tauri build embeds and
// the E2E lane's binary is stamped against, and adapter-static rimrafs its
// output dir on every build, so measuring in place would race a concurrent
// `--include-slow` run and invalidate a cached E2E binary for nothing.
// `CMDR_FRONTEND_BUILD_DIR` is the existing seam for exactly this (see
// `apps/desktop/svelte.config.js`); it must live inside `.svelte-kit/` so it is
// never itself a mount point.
func desktopBundleOutDir(rootDir string) string {
	return filepath.Join(rootDir, "apps", "desktop", ".svelte-kit", "bundle-size-build")
}

// buildDesktopFrontend produces a PRODUCTION-shaped bundle: neither
// `CMDR_E2E_BUILD` nor `CMDR_I18N_CAPTURE_BUILD` is set, so the dialog gallery
// and the capture instrumentation drop out exactly as they do in a release.
// Roughly six seconds; the lane's `Inputs` keep it off the hot path.
func buildDesktopFrontend(ctx *CheckContext) error {
	outDir := desktopBundleOutDir(ctx.RootDir)
	cmd := exec.Command("pnpm", "--filter", "@cmdr/desktop", "build")
	cmd.Dir = ctx.RootDir
	cmd.Env = append(os.Environ(), "CMDR_FRONTEND_BUILD_DIR="+outDir)
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("desktop frontend build failed: %w\n%s", err, out)
	}
	return nil
}

// desktopImmutableDirRE matches a file directly inside SvelteKit's
// content-addressed output tree, capturing its directory and extension.
var desktopImmutableDirRE = regexp.MustCompile(`^(_app/immutable/[^/]+/)[^/]+(\.[a-z0-9]+)$`)

// normalizeDesktopAssetName gives a built asset one identity across rebuilds.
//
// The shared `normalizeAssetName` handles Astro's `name.hash.ext`, which covers
// SvelteKit's `nodes/5.DV2WG7G5.js` and `entry/start.DjVvJ3nX.js`. It does NOT
// cover `chunks/8ndMSAZW.js`, where the hash IS the whole basename, so every
// rebuild would retire ten topAssets entries and add ten new ones. Everything
// under `_app/immutable/` is content-addressed by construction (that is what
// makes it immutable), so collapsing a whole directory there is a structural
// rule rather than a guess about what looks like a hash, which matters because
// the emitters do not agree on hash length (8 and 9 chars both occur today).
func normalizeDesktopAssetName(relPath string) string {
	if normalized := normalizeAssetName(relPath); normalized != relPath {
		return normalized
	}
	if m := desktopImmutableDirRE.FindStringSubmatch(relPath); m != nil {
		return m[1] + "*" + m[2]
	}
	return relPath
}

func desktopBundleSpec(rootDir string) bundleSizeSpec {
	return bundleSizeSpec{
		label:       "frontend bundle",
		distDir:     desktopBundleOutDir(rootDir),
		baselineRel: desktopBundleBaselineRel,
		comment:     desktopBundleBaselineComment,
		refreshCmd:  "pnpm check desktop-bundle-size",
		prepare:     buildDesktopFrontend,
		missingHint: "frontend build produced no output",
		normalize:   normalizeDesktopAssetName,
	}
}

// RunDesktopBundleSize builds the desktop frontend into a private directory and
// compares its total against the committed baseline.
//
// ⚠️ A locally generated pseudolocale inflates this. `messages/en-XA/` is
// gitignored and absent from a clean checkout, but `pnpm i18n:pseudo` creates it
// and the catalog glob then bundles it like any other locale, adding roughly
// 250 kB. That sits inside the 10% band, so it warns nobody, but it is the first
// thing to check if the number moves without a code change.
func RunDesktopBundleSize(ctx *CheckContext) (CheckResult, error) {
	return runBundleSizeCheck(ctx, desktopBundleSpec(ctx.RootDir))
}
