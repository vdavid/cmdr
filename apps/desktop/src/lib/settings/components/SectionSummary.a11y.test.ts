/**
 * Tier 3 a11y tests for `SectionSummary.svelte`.
 *
 * Grid of subsection cards shown for top-level sections.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SectionSummary from './SectionSummary.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => undefined),
  setSetting: vi.fn(() => Promise.resolve()),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('SectionSummary a11y', () => {
  it('General section (multiple subsections) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SectionSummary, {
      target,
      props: { sectionName: 'General', onNavigate: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('unknown section (no subsections) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SectionSummary, {
      target,
      props: { sectionName: 'NonexistentSection', onNavigate: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
