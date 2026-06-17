/**
 * Tier 3 a11y tests for `AdvancedSection.svelte`.
 *
 * Auto-generated setting rows for every `section: ['Advanced']` setting,
 * grouped into `SectionCard`s by `cardKey`. Covers default and search-filtered
 * states, the card structure, and the per-row search highlight (which only
 * works because Advanced rows are in the global search index).
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AdvancedSection from './AdvancedSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { clearSearchIndex } from '$lib/settings/settings-search'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => 100),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: vi.fn(() => Promise.resolve(false)),
}))

vi.mock('$lib/tauri-commands', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

describe('AdvancedSection a11y', () => {
  it('default (no search) has no a11y violations', async () => {
    clearSearchIndex()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AdvancedSection, { target, props: { searchQuery: '' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders rows grouped into labeled SectionCards (no flat list)', async () => {
    clearSearchIndex()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AdvancedSection, { target, props: { searchQuery: '' } })
    await tick()
    // Multiple cards, each with a heading and at least one row.
    const cards = target.querySelectorAll('.section-card-wrap')
    expect(cards.length).toBeGreaterThan(1)
    const headings = Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
    expect(headings).toContain('Performance')
    expect(target.querySelectorAll('.advanced-setting-row').length).toBeGreaterThan(0)
  })

  it('shows only the matching card and highlights the matched row under search', async () => {
    clearSearchIndex()
    const target = document.createElement('div')
    document.body.appendChild(target)
    // "prefetch" matches only `advanced.prefetchBufferSize` (in the Performance card).
    mount(AdvancedSection, { target, props: { searchQuery: 'prefetch' } })
    await tick()
    // Exactly one card frame (no empty frames from the other groups).
    expect(target.querySelectorAll('.section-card-wrap').length).toBe(1)
    // The matched label is highlighted (only possible because Advanced rows are
    // in the global index now; pre-un-exclusion this was always empty).
    const highlights = target.querySelectorAll('.search-highlight')
    expect(highlights.length).toBeGreaterThan(0)
    await expectNoA11yViolations(target)
  })
})
