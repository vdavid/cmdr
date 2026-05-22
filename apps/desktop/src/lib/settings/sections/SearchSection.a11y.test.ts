/**
 * Tier 3 a11y tests for `SearchSection.svelte`.
 *
 * The section renders the auto-apply switch plus the mirrored
 * `search.recentSearches.maxCount` number input, both gated by the section's
 * search-query filter. Covered states: default, and filter-matched.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchSection from './SearchSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'search.autoApply') return true
    if (key === 'search.recentSearches.maxCount') return 1000
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('SearchSection a11y', () => {
  it('default (no filter) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('filtered by "auto" has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchSection, { target, props: { searchQuery: 'auto' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
