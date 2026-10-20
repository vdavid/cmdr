/**
 * Tier 3 accessibility test helper.
 *
 * Runs axe-core against a mounted DOM subtree in Vitest/jsdom and asserts
 * zero violations. Covers structural a11y (ARIA, labels, focus, keyboard
 * semantics), not color contrast. Contrast is checked at design time by
 * `scripts/check-a11y-contrast/` (tier 1). Full-page interaction checks
 * (focus traps, Escape return-focus) live in the Playwright suite (tier 2).
 *
 * Usage (from any `*.a11y.test.ts` file):
 *     import { expectNoA11yViolations } from '#lib/test-a11y.js'
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
import { expect, vi } from 'vitest'

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
    // jsdom doesn't implement computed layout, so many region/landmark rules
    // misfire on synthetic fragments detached from a full document. The E2E
    // tier covers landmark structure against the real app.
    region: { enabled: false },
  },
}

/**
 * A 2D canvas context that draws every glyph as solid ink, for the length of one audit.
 *
 * axe's `label-content-name-mismatch` asks a canvas whether each text node is an icon-font
 * ligature before it compares the visible text with the accessible name, and the unit DOM's
 * canvas returns no pixels, so the rule threw, landed in `incomplete`, and passed for free
 * while the E2E audit failed on the same rows. Solid, width-proportional ink reads as plain
 * text, which is what our text nodes are.
 */
async function withTextCanvas<T>(run: () => Promise<T>): Promise<T> {
  const canvas = document.createElement('canvas')
  const context = {
    canvas,
    measureText: (text: string) => ({ width: 10 * text.length }),
    fillText: () => {},
    clearRect: () => {},
    getImageData: (_x: number, _y: number, width: number, height: number) => ({
      data: new Uint8ClampedArray(Math.max(1, Math.ceil(width) * Math.ceil(height)) * 4).fill(255),
    }),
  } as unknown as CanvasRenderingContext2D
  const spy = vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(context)
  try {
    return await run()
  } finally {
    spy.mockRestore()
  }
}

/** Per-audit opt-outs. Each one names the issue that removes it. */
export interface A11yAuditOptions {
  /**
   * Skip `label-content-name-mismatch` (WCAG 2.5.3) for a KNOWN gap, given as the issue that
   * fixes it (`'#450'`). Every other audit runs the rule.
   */
  skipLabelInName?: string
}

function runAxe(element: Element | Document, { skipLabelInName }: A11yAuditOptions): Promise<AxeResults> {
  if (skipLabelInName === undefined) return withTextCanvas(() => axe.run(element, AXE_OPTIONS))
  return axe.run(element, {
    ...AXE_OPTIONS,
    rules: { ...AXE_OPTIONS.rules, 'label-content-name-mismatch': { enabled: false } },
  })
}

/**
 * Runs axe-core on the given DOM element (or root document) and throws a
 * descriptive assertion failure listing each violation.
 *
 * @param element - A mounted container element. Pass the `target` you gave to
 *                  Svelte's `mount()`. Defaults to `document.body`.
 */
export async function expectNoA11yViolations(
  element: Element | Document = document.body,
  options: A11yAuditOptions = {},
): Promise<void> {
  const results: AxeResults = await runAxe(element, options)

  if (process.env.CMDR_A11Y_DEBUG) {
    console.log(
      `[a11y] ran ${String(results.passes.length + results.violations.length + results.incomplete.length)} checks: ${String(results.passes.length)} pass, ${String(results.violations.length)} violations, ${String(results.incomplete.length)} incomplete`,
    )
  }

  if (results.violations.length === 0) return

  // Build a readable failure message the way jest-axe does (axe's native
  // output is nested and hard to scan in a Vitest diff).
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
