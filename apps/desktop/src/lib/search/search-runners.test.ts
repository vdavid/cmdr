/**
 * Search's two run paths and its pager: `loadMore` asks the question the last run SENT, so
 * a bar edited since without re-running can't splice another query's rows into the list.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SearchQuery } from '#lib/tauri-commands/index.js'
import { createSearchRunners } from './search-runners'
import { clearSearchState, setQuery } from './search-state.svelte'

const searchFiles = vi.fn((_query: SearchQuery) =>
  Promise.resolve({ entries: [], totalCount: 0, targetVolumeId: 'root' }),
)

vi.mock('#lib/tauri-commands/index.js', () => ({
  searchFiles: (query: SearchQuery) => searchFiles(query),
  parseSearchScope: () => Promise.resolve({ includePaths: [], excludePatterns: [] }),
  trackEvent: () => Promise.resolve(),
}))

// The live transport is Tauri events all the way down, and nothing here walks.
vi.mock('./live-search-source', () => ({
  createLiveSearchSource: () => ({ start: () => Promise.resolve(() => {}), cancel: () => {} }),
}))

function makeRunners() {
  return createSearchRunners({ getDefaultScopePath: () => '/Users/test', onRunState: () => {} })
}

beforeEach(() => {
  clearSearchState()
  searchFiles.mockClear()
})

describe('loadMore', () => {
  it('has nothing to extend before a run', async () => {
    expect(await makeRunners().loadMore({ limit: 230 })).toBeNull()
    expect(searchFiles).not.toHaveBeenCalled()
  })

  it('asks the last run’s question again with the larger limit, not what the bar says now', async () => {
    const runners = makeRunners()
    setQuery('*.pdf')
    await runners.runQuery({ limit: 30 })
    // Edited, not re-run (auto-apply off, or still inside its debounce).
    setQuery('*.jpg')

    await runners.loadMore({ limit: 230 })

    expect(searchFiles).toHaveBeenCalledTimes(2)
    const [run, page] = searchFiles.mock.calls.map(([query]) => query)
    expect(run.limit).toBe(30)
    expect(page).toEqual({ ...run, limit: 230 })
    expect(page.namePattern).toBe('*.pdf')
  })
})
