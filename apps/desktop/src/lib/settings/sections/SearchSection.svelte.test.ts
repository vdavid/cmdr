/**
 * Tier-3 tests for `SearchSection.svelte`.
 *
 * Pins the M6 contract:
 *   - The auto-apply switch renders (canonical home).
 *   - The recent-searches max-count row renders here too (mirror; canonical is Advanced).
 *   - Rows respect the search filter (`shouldShow`).
 *   - No a11y violations.
 */

import { describe, it, expect, vi } from 'vitest'
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

describe('SearchSection', () => {
  it('renders both rows when no search filter is active', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchSection, { target, props: { searchQuery: '' } })
    await tick()
    const labels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(labels).toContain('Auto-apply searches')
    expect(labels).toContain('Recent searches to remember')
    target.remove()
  })

  it('filters rows by the active search query', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // "debounce" is in the autoApply keywords but not in the recent-max definition, so only the
    // first row should render.
    mount(SearchSection, { target, props: { searchQuery: 'debounce' } })
    await tick()
    const labels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(labels).toContain('Auto-apply searches')
    expect(labels).not.toContain('Recent searches to remember')
    target.remove()
  })

  it('has no a11y violations in the default state', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
