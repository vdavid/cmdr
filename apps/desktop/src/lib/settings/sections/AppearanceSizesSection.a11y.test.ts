/**
 * Tier 3 a11y tests for `AppearanceSizesSection.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AppearanceSizesSection from './AppearanceSizesSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'listing.sizeDisplay') return 'smart'
    if (key === 'listing.humanFriendlySizeUnits') return true
    if (key === 'appearance.fileSizeFormat') return 'binary'
    if (key === 'listing.sizeMismatchWarning') return true
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('AppearanceSizesSection a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AppearanceSizesSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
