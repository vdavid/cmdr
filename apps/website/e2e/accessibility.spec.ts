/**
 * Accessibility audit for the Cmdr website using axe-core.
 *
 * Runs WCAG 2 AA checks on every page in both light and dark mode.
 * Uses @axe-core/playwright for clean integration.
 */

import { test, expect } from '@playwright/test'
import AxeBuilder from '@axe-core/playwright'

/**
 * Pages to audit. Each gets tested in both light and dark mode. `slow` triples the test timeout, for a
 * page whose axe run outgrows the default (the changelog gains a section every release).
 */
const pages: { name: string; path: string; slow?: boolean }[] = [
  { name: 'Homepage', path: '/' },
  { name: 'Features', path: '/features' },
  { name: 'Pricing', path: '/pricing' },
  { name: 'Changelog', path: '/changelog', slow: true },
  { name: 'Roadmap', path: '/roadmap' },
  { name: 'Blog', path: '/blog' },
  { name: 'Privacy policy', path: '/privacy-policy' },
  { name: 'Terms and conditions', path: '/terms-and-conditions' },
  { name: 'Refund', path: '/refund' },
  { name: 'Data safety', path: '/data-safety' },
  { name: 'Trust and security', path: '/trust' },
  { name: 'How Cmdr is built', path: '/trust/development' },
  { name: 'Accessibility conformance report', path: '/trust/accessibility' },
  { name: 'Accessibility guide', path: '/trust/accessibility-guide' },
  { name: 'Renew', path: '/renew' },
]

for (const theme of ['light', 'dark'] as const) {
  test.describe(`Accessibility (${theme} mode)`, () => {
    for (const { name, path, slow } of pages) {
      test(`${name} (${path})`, async ({ page }) => {
        test.slow(slow === true)
        await page.goto(path)

        // Set theme via localStorage + data-theme attribute (matches the site's ThemeToggle behavior)
        await page.evaluate((t) => {
          localStorage.setItem('theme', t)
          document.documentElement.setAttribute('data-theme', t)
        }, theme)

        // Wait for the theme's color transitions and the entrance animations to finish. A fixed wait
        // raced them: the feature cards transition over 300 ms, and the hero's lines fade in over a
        // second, so axe sampled half-faded colors. Infinite decorative animations never settle, so
        // they're left out.
        await page.evaluate(async () => {
          const settling = document
            .getAnimations()
            .filter((a) => a instanceof CSSTransition || a.effect?.getComputedTiming().iterations !== Infinity)
          await Promise.all(settling.map((a) => a.finished.catch(() => undefined)))
        })

        // The home page's hero embeds the browser demo: that's the desktop app's markup, which the
        // desktop's own checks audit (`apps/desktop/src/lib/demo/CLAUDE.md`).
        const results = await new AxeBuilder({ page }).exclude('[data-live-demo-frame]').analyze()

        // Log violations for visibility
        for (const v of results.violations) {
          console.log(
            `[axe/${v.impact}] [${name} ${theme}] ${v.id}: ${v.description}\n` +
              `  Help: ${v.helpUrl}\n` +
              v.nodes
                .map((n: { html: string; failureSummary?: string }) => `  - ${n.html}\n    ${n.failureSummary ?? ''}`)
                .join('\n'),
          )
        }

        if (results.violations.length > 0) {
          console.log(`⚠ [${name} ${theme}] ${results.violations.length} violation(s)`)
        }

        expect(results.violations, `${name} (${theme}): ${results.violations.length} violation(s)`).toHaveLength(0)
      })
    }
  })
}
