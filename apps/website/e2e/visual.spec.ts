import { test, expect, type Page, type Locator } from '@playwright/test'

/**
 * Visual-regression (golden screenshot) safety net for the marketing site.
 *
 * The committed set is deliberately small and churn-free. Two rules keep it that way:
 *
 * 1. **Shoot machinery, never content.** Every custom markdown transform is exercised by
 *    `/visual-fixture` (`src/fixtures/visual-fixture.md`), a frozen page that isn't editorial, so a
 *    baseline moves only when a plugin, a prose style, or the layout moves. Real posts and the
 *    marketing pages are NOT shot full-page: their diffs were always intentional copy changes, and
 *    at 4,000-5,700 px tall each one cost ~1 MB per revision.
 * 2. **Shoot regions, not pages.** Structural regressions (collapsed grids, font fallbacks, broken
 *    stacking contexts) show in the first fold and in specific components. Mid-page prose is most of
 *    the bytes and none of the signal, and it's the part that grows as the site gets written.
 *
 * Result: 6 committed PNGs that stay still when you publish a post, edit marketing copy, or ship a
 * release that touches `feature-status.json`.
 *
 * **Baselines are Linux-only.** CI runs ubuntu-latest, and macOS renders different font
 * antialiasing, so a `-darwin` set would be a second copy of every byte that CI never checks. This
 * file skips on other platforms rather than silently writing baselines that nothing verifies;
 * `scripts/update-visual-baselines.sh` shoots the Linux set in the pinned Playwright container.
 *
 * **Full-page mode.** The one thing region shots can't do is diff a whole page through an Astro
 * major upgrade. `VISUAL_FULL=1` adds full-page shots of every real page into the gitignored
 * `e2e/visual-full-snapshots/` (redirected by `snapshotPathTemplate` in playwright.config.ts):
 * capture before the upgrade, upgrade, compare, discard. Full coverage at the moment it matters,
 * zero committed bytes. See `scripts/update-visual-baselines.sh --full`.
 *
 * Determinism notes:
 * - Fixed 1280x800 viewport, deviceScaleFactor 1 (see `test.use` below).
 * - All animation/transition/scroll-behavior zeroed and the caret hidden via injected CSS;
 *   `prefers-reduced-motion: reduce` is emulated too.
 * - The theme is forced deterministically by seeding `localStorage.theme` + `data-theme` on
 *   `<html>` before any page script runs (the same mechanism `ThemeToggle.astro` /
 *   `Layout.astro` use), so we don't depend on the OS/browser color scheme.
 * - Every non-`localhost` request is aborted. Nothing visible on these pages is cross-origin
 *   (system fonts, local `_astro`/`blog` images), but analytics (PostHog, Umami), Remark42
 *   comments, and the `api.getcmdr.com/r-codes.json` acquisition fetch are. Blocking them
 *   guarantees `networkidle` fires and keeps the render offline-stable.
 * - Volatile download version/size text (baked in from `public/latest.json` at build time) is
 *   masked so a release that refreshes `latest.json` between baseline and comparison can't cause
 *   a spurious diff.
 */

const VIEWPORT = { width: 1280, height: 800 }
const VIEWPORT_PORT = 18473 // matches playwright.config.ts baseURL / webServer

const FULL_PAGE_MODE = !!process.env.VISUAL_FULL

// Zero out anything time-dependent so a shot is a pure function of layout + theme.
const FREEZE_CSS = `*,*::before,*::after{animation-duration:0s!important;animation-delay:0s!important;transition-duration:0s!important;transition-delay:0s!important;scroll-behavior:auto!important;caret-color:transparent!important}`

// Version + download-size text comes from public/latest.json at build time; mask so a refreshed
// release JSON between baseline capture and post-upgrade comparison can't trip a diff.
// `.split-btn__sub` holds the "<version> · <size>" line; `.split-btn__option-size` the per-arch
// sizes inside the dropdown.
function volatileMasks(page: Page): Locator[] {
  return [page.locator('.split-btn__sub'), page.locator('.split-btn__option-size')]
}

type Theme = 'light' | 'dark'

/** Seed the forced theme + reduced motion, and block all cross-origin traffic, for this page. */
async function preparePage(page: Page, theme: Theme): Promise<void> {
  await page.route('**/*', (route) => {
    const host = new URL(route.request().url()).host
    if (host === `localhost:${VIEWPORT_PORT}`) return route.continue()
    return route.abort()
  })
  await page.emulateMedia({ colorScheme: theme, reducedMotion: 'reduce' })
  await page.addInitScript((t) => {
    try {
      localStorage.setItem('theme', t)
    } catch {
      /* localStorage unavailable */
    }
    // Backstop the inline FOUC script in case localStorage is ever unreadable.
    document.documentElement.dataset.theme = t
  }, theme)
}

/** Navigate and settle: network idle, fonts loaded, animations frozen. */
async function settle(page: Page, url: string): Promise<void> {
  await page.goto(url, { waitUntil: 'networkidle' })
  await page.evaluate(() => document.fonts.ready)
  await page.addStyleTag({ content: FREEZE_CSS })
}

// Baselines are Linux-only; see the header. Skipping (rather than shooting) is what stops a
// `-darwin` set from reappearing the next time someone runs the suite on a Mac.
test.skip(() => process.platform !== 'linux', 'Visual baselines are Linux-only (CI renders them)')

// Reduced motion is applied per-page via `emulateMedia` in `preparePage`; `test.use` has no
// `reducedMotion` option in the installed Playwright types, so it stays out of here.
test.use({ viewport: VIEWPORT, deviceScaleFactor: 1 })

// ---------------------------------------------------------------------------------------------
// Committed baselines (6 PNGs)
// ---------------------------------------------------------------------------------------------

test.describe('committed baselines', () => {
  test.skip(FULL_PAGE_MODE, 'VISUAL_FULL runs the full-page set instead')

  // The fixture is the only thing shot in both themes: it's the densest mix of themed components
  // (Shiki dual-theme, theme images, icon palette, dropdown surfaces), so the pair earns its bytes.
  // Layout is theme-independent everywhere else, so the rest are light-only.
  //
  // Scoped to `article.blog-content`, not the page: the header is covered by the home fold and the
  // footer has its own shot, so including them here would be a third copy of the same chrome and
  // would make this baseline churn on every header edit.
  for (const theme of ['light', 'dark'] as Theme[]) {
    test(`markdown fixture (${theme})`, async ({ page }) => {
      await preparePage(page, theme)
      await settle(page, '/visual-fixture')
      await expect(page.locator('article.blog-content')).toHaveScreenshot(`fixture-${theme}.png`, {
        mask: volatileMasks(page),
        maxDiffPixelRatio: 0.01,
      })
    })
  }

  // Interaction state: the arch download dropdown, clipped to itself rather than re-shooting the
  // whole page around it.
  //
  // Scoped INSIDE the article on purpose. `rehypeDownloadDropdown` emits the `split-btn--inline`
  // variant (a prose link with its own trigger styles), which is different markup from the header's
  // split button; an unscoped `.first()` matches the header one and silently covers the wrong thing.
  test('download dropdown open', async ({ page }) => {
    await preparePage(page, 'light')
    await settle(page, '/visual-fixture')

    const splitBtn = page.locator('article.blog-content [data-download-split-btn]').first()
    const chevron = splitBtn.locator('button').first()
    await chevron.click()
    const dropdown = splitBtn.locator('[data-download-dropdown]:not([hidden])').first()
    await expect(dropdown).toBeVisible()

    await expect(page).toHaveScreenshot('fixture-dropdown.png', {
      fullPage: true,
      clip: await unionBox(splitBtn, dropdown),
      mask: volatileMasks(page),
      maxDiffPixelRatio: 0.01,
    })
  })

  // The home fold: nav, hero, download split button, theme toggle. Everything below it is copy.
  test('home fold', async ({ page }) => {
    await preparePage(page, 'light')
    await settle(page, '/')
    await expect(page).toHaveScreenshot('home-fold.png', {
      mask: volatileMasks(page),
      maxDiffPixelRatio: 0.01,
    })
  })

  // The tier grid is the layout-heavy part of /pricing (equal-height columns, badge placement).
  test('pricing tiers', async ({ page }) => {
    await preparePage(page, 'light')
    await settle(page, '/pricing')
    await expect(page.locator('[data-visual="pricing-tiers"]')).toHaveScreenshot('pricing-tiers.png', {
      maxDiffPixelRatio: 0.01,
    })
  })

  // The footer is shared by every page, so one shot covers all of them.
  test('footer', async ({ page }) => {
    await preparePage(page, 'light')
    await settle(page, '/')
    await expect(page.locator('footer').first()).toHaveScreenshot('footer.png', {
      maxDiffPixelRatio: 0.01,
    })
  })
})

/** Page-coordinate bounding box covering both locators, padded, for a tight `clip`. */
async function unionBox(a: Locator, b: Locator): Promise<{ x: number; y: number; width: number; height: number }> {
  const [boxA, boxB] = await Promise.all([a.boundingBox(), b.boundingBox()])
  if (!boxA || !boxB) throw new Error('unionBox: a locator had no bounding box')
  const scrollY = await a.page().evaluate(() => window.scrollY)
  const scrollX = await a.page().evaluate(() => window.scrollX)
  const pad = 12
  // boundingBox() is viewport-relative; `clip` with fullPage is page-relative.
  const left = Math.min(boxA.x, boxB.x) + scrollX - pad
  const top = Math.min(boxA.y, boxB.y) + scrollY - pad
  const right = Math.max(boxA.x + boxA.width, boxB.x + boxB.width) + scrollX + pad
  const bottom = Math.max(boxA.y + boxA.height, boxB.y + boxB.height) + scrollY + pad
  return {
    x: Math.max(0, left),
    y: Math.max(0, top),
    width: right - Math.max(0, left),
    height: bottom - Math.max(0, top),
  }
}

// ---------------------------------------------------------------------------------------------
// Full-page mode (VISUAL_FULL=1) — gitignored, for diffing an Astro major upgrade
// ---------------------------------------------------------------------------------------------

// `/changelog` is deliberately absent: it renders the whole CHANGELOG at ~75,000 px, and consecutive
// full-page captures disagree on the height, so Playwright can never stabilize it. It has no committed
// baseline either, so nothing is lost; check it by eye after an upgrade.
const fullPages: Array<{ path: string; slug: string }> = [
  { path: '/', slug: 'home' },
  { path: '/features', slug: 'features' },
  { path: '/pricing', slug: 'pricing' },
  { path: '/roadmap', slug: 'roadmap' },
  { path: '/blog', slug: 'blog-index' },
  { path: '/blog/total-commander-for-macos', slug: 'blog-tc' },
  { path: '/blog/35-years-of-file-managers', slug: 'blog-35years' },
  { path: '/visual-fixture', slug: 'fixture' },
]

test.describe('full-page (upgrade diffing)', () => {
  test.skip(!FULL_PAGE_MODE, 'Set VISUAL_FULL=1 to capture the full-page set')

  for (const theme of ['light', 'dark'] as Theme[]) {
    for (const { path, slug } of fullPages) {
      test(`${slug} (${theme})`, async ({ page }) => {
        await preparePage(page, theme)
        await settle(page, path)
        await expect(page).toHaveScreenshot(`${slug}-${theme}.png`, {
          fullPage: true,
          mask: volatileMasks(page),
          maxDiffPixelRatio: 0.01,
        })
      })
    }
  }
})
