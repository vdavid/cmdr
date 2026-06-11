/**
 * Tier 3 a11y test for the DateLabel catalog section. Stubs the reactive-settings
 * `formattedDate` helper the same way `DateLabel.a11y.test.ts` does so jsdom can
 * render without a live settings store. Catches regressions in the section
 * layout (caption ↔ value pairing) and the underlying DateLabel markup.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DateLabelSection from './DateLabelSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formattedDate: (t: number | null | undefined) =>
    t
      ? {
          text: '2026-05-28 10:30',
          segments: [
            { text: '2026', ageClass: 'age-fresh' as const },
            { text: '-05-28 ', ageClass: null },
            { text: '10:30', ageClass: null },
          ],
        }
      : { text: '', segments: [] },
}))

describe('DateLabelSection a11y', () => {
  it('renders without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DateLabelSection, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
