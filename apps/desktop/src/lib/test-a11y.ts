/**
 * Tier 3 accessibility test helper.
 *
 * Runs axe-core against a mounted DOM subtree in Vitest/jsdom and asserts
 * zero violations. Covers structural a11y (ARIA, labels, focus, keyboard
 * semantics) — NOT color contrast. Contrast is checked at design time by
 * `scripts/check-a11y-contrast/` (tier 1). Full-page interaction checks
 * (focus traps, Escape return-focus) live in the Playwright suite (tier 2).
 *
 * Usage (from any `*.a11y.test.ts` file):
 *     import { expectNoA11yViolations } from '$lib/test-a11y'
 *
 *     it('has no a11y violations when disabled', async () => {
 *         const target = document.createElement('div')
 *         mount(Button, { target, props: { disabled: true, children: snip('Save') } })
 *         await tick()
 *         await expectNoA11yViolations(target)
 *     })
 *
 * See `apps/desktop/src/lib/CLAUDE.md` § "Adding a component-level a11y test"
 * and `docs/design-system.md` § a11y for the full three-tier strategy.
 */

import axe, { type AxeResults, type RunOptions } from 'axe-core'
import { expect } from 'vitest'

/**
 * Axe options matching the E2E suite (WCAG 2.0/2.1/2.2 AA) minus `color-contrast`
 * which is handled deterministically at design time by `scripts/check-a11y-contrast`.
 *
 * Kept in sync with `apps/desktop/test/e2e-playwright/accessibility.spec.ts`.
 */
const AXE_OPTIONS: RunOptions = {
  runOnly: {
    type: 'tag',
    values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'],
  },
  rules: {
    // Contrast is checked at design time via `scripts/check-a11y-contrast`
    // (deterministic, no engine-dependent color-mix() resolution quirks).
    'color-contrast': { enabled: false },
    // jsdom doesn't implement computed layout — many region/landmark rules
    // misfire on synthetic fragments detached from a full document. The E2E
    // tier covers landmark structure against the real app.
    region: { enabled: false },
  },
}

/**
 * Runs axe-core on the given DOM element (or root document) and throws a
 * descriptive assertion failure listing each violation.
 *
 * @param element - A mounted container element. Pass the `target` you gave to
 *                  Svelte's `mount()`. Defaults to `document.body`.
 */
export async function expectNoA11yViolations(element: Element | Document = document.body): Promise<void> {
  const results: AxeResults = await axe.run(element, AXE_OPTIONS)

  if (process.env.CMDR_A11Y_DEBUG) {
    // eslint-disable-next-line no-console
    console.log(
      `[a11y] ran ${String(results.passes.length + results.violations.length + results.incomplete.length)} checks: ${String(results.passes.length)} pass, ${String(results.violations.length)} violations, ${String(results.incomplete.length)} incomplete`,
    )
  }

  if (results.violations.length === 0) return

  // Build a readable failure message the way jest-axe does — axe's native
  // output is nested and hard to scan in a Vitest diff.
  const message =
    `Expected no a11y violations, got ${String(results.violations.length)}:\n\n` +
    results.violations
      .map((v) => {
        const nodeList = v.nodes.map((n) => `    - ${n.html}\n      ${n.failureSummary ?? ''}`).join('\n')
        return `  [${v.impact ?? 'n/a'}] ${v.id}: ${v.description}\n  Help: ${v.helpUrl}\n${nodeList}`
      })
      .join('\n\n')

  expect.fail(message)
}

/** Re-export so callers can build one-off custom audits if needed. */
export { axe }
