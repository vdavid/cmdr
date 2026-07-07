import { test, expect, type Page, type Locator } from '@playwright/test'

/**
 * Visual-regression (golden screenshot) safety net for the marketing site.
 *
 * Captures full-page baselines of the key pages in both light and dark themes, so an Astro
 * major-version upgrade (or any refactor) can be diff-checked against a known-good render.
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

/** Navigate, settle (network + fonts), freeze animations, and full-page screenshot. */
async function shoot(page: Page, url: string, name: string): Promise<void> {
  await page.goto(url, { waitUntil: 'networkidle' })
  await page.evaluate(() => document.fonts.ready)
  await page.addStyleTag({ content: FREEZE_CSS })
  await expect(page).toHaveScreenshot(name, {
    fullPage: true,
    mask: volatileMasks(page),
    maxDiffPixelRatio: 0.01,
  })
}

// Reduced motion is applied per-page via `emulateMedia` in `preparePage`; `test.use` has no
// `reducedMotion` option in the installed Playwright types, so it stays out of here.
test.use({ viewport: VIEWPORT, deviceScaleFactor: 1 })

const pages: Array<{ path: string; slug: string }> = [
  { path: '/', slug: 'home' },
  { path: '/features', slug: 'features' },
  { path: '/pricing', slug: 'pricing' },
  { path: '/blog', slug: 'blog-index' },
  // The critical post: exercises every custom markdown plugin (arch download dropdown,
  // {theme} light/dark images, before/after comparison slider at its default 50%, inline
  // :icon: tokens).
  { path: '/blog/total-commander-for-macos', slug: 'blog-tc' },
  // Download dropdown + inline icons.
  { path: '/blog/35-years-of-file-managers', slug: 'blog-35years' },
]

const themes: Theme[] = ['light', 'dark']

for (const theme of themes) {
  for (const { path, slug } of pages) {
    test(`${slug} (${theme})`, async ({ page }) => {
      await preparePage(page, theme)
      await shoot(page, path, `${slug}-${theme}.png`)
    })
  }

  // Interaction state: the arch download dropdown opened on the Total Commander post.
  test(`blog-tc dropdown open (${theme})`, async ({ page }) => {
    await preparePage(page, theme)
    await page.goto('/blog/total-commander-for-macos', { waitUntil: 'networkidle' })
    await page.evaluate(() => document.fonts.ready)

    // Open the first visible arch dropdown (the chevron is the only <button> in a split-btn).
    const chevron = page.locator('[data-download-split-btn] button').filter({ visible: true }).first()
    await chevron.click()
    await expect(page.locator('[data-download-dropdown]:not([hidden])').first()).toBeVisible()

    await page.addStyleTag({ content: FREEZE_CSS })
    await expect(page).toHaveScreenshot(`blog-tc-dropdown-${theme}.png`, {
      fullPage: true,
      mask: volatileMasks(page),
      maxDiffPixelRatio: 0.01,
    })
  })
}
