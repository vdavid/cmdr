/**
 * Tier 3 a11y tests for `OperationLogSection.svelte`.
 *
 * The section renders the intro paragraph plus the two retention pickers
 * (`operationLog.maxAge` duration select, `operationLog.maxSize` byte select),
 * both gated by the section's search-query filter. Covered states: default, and
 * filter-matched.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import OperationLogSection from './OperationLogSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'operationLog.maxAge') return 0 // Forever
    if (key === 'operationLog.maxSize') return 3221225472 // 3 GB
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('OperationLogSection a11y', () => {
  it('default (no filter) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationLogSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('filtered by "size" has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(OperationLogSection, { target, props: { searchQuery: 'size' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
