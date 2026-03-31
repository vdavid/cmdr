/**
 * Accessibility audit for the Cmdr website using axe-core.
 *
 * Runs WCAG 2 AA checks on every page in both light and dark mode.
 * Uses @axe-core/playwright for clean integration.
 */

import { test, expect } from '@playwright/test'
import AxeBuilder from '@axe-core/playwright'

/** Pages to audit. Each gets tested in both light and dark mode. */
const pages = [
  { name: 'Homepage', path: '/' },
  { name: 'Features', path: '/features' },
  { name: 'Pricing', path: '/pricing' },
  { name: 'Changelog', path: '/changelog' },
  { name: 'Roadmap', path: '/roadmap' },
  { name: 'Blog', path: '/blog' },
  { name: 'Privacy policy', path: '/privacy-policy' },
  { name: 'Terms and conditions', path: '/terms-and-conditions' },
  { name: 'Refund', path: '/refund' },
  { name: 'Renew', path: '/renew' },
]

for (const theme of ['light', 'dark'] as const) {
  test.describe(`Accessibility (${theme} mode)`, () => {
    for (const { name, path } of pages) {
      test(`${name} (${path})`, async ({ page }) => {
        await page.goto(path)

        // Set theme via localStorage + data-theme attribute (matches the site's ThemeToggle behavior)
        await page.evaluate((t) => {
          localStorage.setItem('theme', t)
          document.documentElement.setAttribute('data-theme', t)
        }, theme)

        // Brief wait for any theme-dependent styles to settle
        await page.waitForTimeout(200)

        const results = await new AxeBuilder({ page }).analyze()

        // Log violations for visibility
        for (const v of results.violations) {
          // eslint-disable-next-line no-console
          console.log(
            `[axe/${v.impact}] [${name} ${theme}] ${v.id}: ${v.description}\n` +
              `  Help: ${v.helpUrl}\n` +
              v.nodes
                .map((n: { html: string; failureSummary: string }) => `  - ${n.html}\n    ${n.failureSummary}`)
                .join('\n'),
          )
        }

        if (results.violations.length > 0) {
          // eslint-disable-next-line no-console
          console.log(`⚠ [${name} ${theme}] ${results.violations.length} violation(s)`)
        }

        expect(results.violations, `${name} (${theme}): ${results.violations.length} violation(s)`).toHaveLength(0)
      })
    }
  })
}
