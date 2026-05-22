/**
 * Tier-3 a11y tests for `RecentSearchesFooter.svelte`.
 *
 * The footer is a `role="region"` chip strip with up to 6 recent-search chips
 * plus a trailing "All searches…" affordance. Covered states: zero entries
 * (component renders nothing), one entry, many entries, and the disabled
 * variant (index not ready).
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import RecentSearchesFooter from './RecentSearchesFooter.svelte'
import type { HistoryEntry } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'

function makeEntry(overrides: Partial<HistoryEntry> = {}): HistoryEntry {
  return {
    id: 'id-' + (overrides.query ?? 'x'),
    timestamp: Date.now(),
    mode: 'filename',
    query: '*.pdf',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 0,
    ...overrides,
  }
}

describe('RecentSearchesFooter a11y', () => {
  it('zero entries (no DOM) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [],
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('one entry has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [makeEntry({ query: 'screenshots', mode: 'ai', id: 'a' })],
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('many entries (capped at 6 visible + All searches) has no a11y violations', async () => {
    const entries: HistoryEntry[] = [
      makeEntry({ query: 'one', mode: 'filename', id: '1' }),
      makeEntry({ query: 'two', mode: 'ai', id: '2' }),
      makeEntry({ query: 'three', mode: 'regex', id: '3' }),
      makeEntry({ query: 'four', mode: 'filename', id: '4' }),
      makeEntry({ query: 'five', mode: 'ai', id: '5' }),
      makeEntry({ query: 'six', mode: 'filename', id: '6' }),
      makeEntry({ query: 'seven', mode: 'ai', id: '7' }),
    ]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: { entries, disabled: false, onPick: () => {}, onRemove: () => {}, onOpenAll: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [makeEntry({ query: 'one', id: 'd' })],
        disabled: true,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
