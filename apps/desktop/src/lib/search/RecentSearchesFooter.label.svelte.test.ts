/**
 * D5: the recent-searches strip carries a "Recent searches:" label prefix so
 * the user can read what those chips are without needing context.
 */
import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import RecentSearchesFooter from './RecentSearchesFooter.svelte'
import type { HistoryEntry } from '$lib/tauri-commands'

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
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    expect(target.textContent ?? '').toContain('Recent searches:')
  })
})
