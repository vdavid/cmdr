/**
 * Tier-3 tests for `SearchSection.svelte`.
 *
 * Pins the M6 + M9 contract:
 *   - The auto-apply switch renders (canonical home).
 *   - The recent-searches max-count row renders here too (mirror; canonical is Advanced).
 *   - The recent-selections max-count row renders here too (mirror; canonical is Advanced; added M9).
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
    if (key === 'selection.recentSelections.maxCount') return 1000
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('SearchSection', () => {
  it('renders all three rows when no search filter is active', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchSection, { target, props: { searchQuery: '' } })
    await tick()
    const labels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(labels).toContain('Auto-apply searches')
    expect(labels).toContain('Recent searches to remember')
    expect(labels).toContain('Recent selections to remember')
    target.remove()
  })

  it('filters rows by the active search query', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // "debounce" is in the autoApply keywords but not in the recent-max definitions, so only
    // the first row should render.
    mount(SearchSection, { target, props: { searchQuery: 'debounce' } })
    await tick()
    const labels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(labels).toContain('Auto-apply searches')
    expect(labels).not.toContain('Recent searches to remember')
    expect(labels).not.toContain('Recent selections to remember')
    target.remove()
  })

  it('hides the recent-selections row when a search filter is active (mirrors showInAdvanced behavior)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // `selection.recentSelections.maxCount` has `showInAdvanced: true`, so `buildSearchIndex`
    // excludes it. Any non-empty search query causes `shouldShow` to return false for it — the
    // mirror is only visible when browsing with no filter active.
    mount(SearchSection, { target, props: { searchQuery: 'selection' } })
    await tick()
    const labels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(labels).not.toContain('Recent selections to remember')
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
