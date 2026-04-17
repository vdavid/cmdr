/**
 * Tier 3 a11y tests for `SettingsSidebar.svelte`.
 *
 * The sidebar is a listbox with one or more sections + a search input.
 * These tests check the ARIA structure (role, aria-selected, aria-label
 * on the listbox and the search-clear button) across:
 *   - default (no search, first section selected)
 *   - with an active search query (clear button visible)
 *   - with a subsection selected
 *
 * The real settings tree is imported — we aren't mocking the registry
 * because it's a pure module with no IO.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingsSidebar from './SettingsSidebar.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// settings-store triggers a Tauri call at module load; silence it.
vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => undefined),
  setSetting: vi.fn(() => Promise.resolve()),
  onSettingChange: vi.fn(() => () => {}),
}))

describe('SettingsSidebar a11y', () => {
  it('default render (first section selected) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: '',
        matchingSections: new Set<string>(),
        selectedSection: ['General', 'Appearance'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with search query + clear button visible has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: 'theme',
        matchingSections: new Set<string>(['Themes']),
        selectedSection: ['General', 'Appearance'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a subsection selected has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: '',
        matchingSections: new Set<string>(),
        selectedSection: ['General', 'Listing'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with empty search results (no matches) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: 'zzznonexistent',
        matchingSections: new Set<string>(),
        selectedSection: ['General', 'Appearance'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
