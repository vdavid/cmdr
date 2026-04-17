/**
 * Tier 3 a11y tests for `ListingSection.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ListingSection from './ListingSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'listing.directorySortMode') return 'likeFiles'
    if (key === 'listing.sizeDisplay') return 'smart'
    if (key === 'listing.sizeMismatchWarning') return true
    if (key === 'listing.stripedRows') return false
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('ListingSection a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ListingSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
