/**
 * Tier-3 tests for `SearchSection.svelte`.
 *
 * Pins the contract:
 *   - The auto-apply switch renders in one unlabeled card (its single home).
 *   - The recent-searches / recent-selections caps do NOT render here: they live
 *     only in Advanced now.
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
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('SearchSection', () => {
  it('renders only the auto-apply row in one unlabeled card', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchSection, { target, props: { searchQuery: '' } })
    await tick()
    const labels = Array.from(target.querySelectorAll('.setting-label')).map((el) => el.textContent.trim())
    expect(labels).toContain('Auto-apply searches')
    // The former mirror rows live only in Advanced now.
    expect(labels).not.toContain('Recent searches to remember')
    expect(labels).not.toContain('Recent selections to remember')
    // One unlabeled card (no heading).
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    expect(target.querySelectorAll('.section-card-label')).toHaveLength(0)
    target.remove()
  })

  it('hides the card when the search matches nothing on this page', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // "selection" matches the recent-selections cap (now in Advanced) but nothing
    // on this page, so the whole card hides.
    mount(SearchSection, { target, props: { searchQuery: 'selection' } })
    await tick()
    expect(target.querySelectorAll('.section-card')).toHaveLength(0)
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
