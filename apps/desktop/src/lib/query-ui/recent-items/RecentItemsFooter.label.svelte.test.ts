/**
 * D5: the recent-searches strip carries a "Recent searches:" label prefix so
 * the user can read what those chips are without needing context.
 */
import { describe, expect, it } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import RecentSearchesFooterRaw from './RecentItemsFooter.svelte'
import type { HistoryEntry } from '$lib/tauri-commands'
import type { RecentItemAdapter, RecentItemKey } from './recent-items-types'
import { chipTooltip, modeName, formatAge } from './recent-items-utils'

// Svelte 5 generics+mount type roundtrip: cast through unknown to avoid unsafe-argument errors.
// See `RecentItemsFooter.svelte.test.ts` for the full explanation.
const RecentSearchesFooter = RecentSearchesFooterRaw as unknown as Component<Record<string, unknown>>

const searchAdapter: RecentItemAdapter<HistoryEntry> = (e) => ({
  label: e.query,
  tooltip: chipTooltip(e),
  mode: e.mode,
  ageLabel: formatAge(e.timestamp),
  ariaLabel: `Run recent ${modeName(e.mode)} search: ${e.query}`,
})
const searchKey: RecentItemKey<HistoryEntry> = (e) => e.id

function entry(query: string): HistoryEntry {
  return {
    id: query,
    timestamp: 0,
    mode: 'filename',
    query,
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 0,
  }
}

describe('RecentSearchesFooter D5: label', () => {
  it('renders a "Recent searches:" label when there are entries', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [entry('*.pdf'), entry('*.jpg')],
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    expect(target.textContent).toContain('Recent searches:')
  })
})
