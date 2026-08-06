import { describe, it, expect, beforeEach } from 'vitest'
import { applySearchPrefill, searchQueryState, setQuery, setResults } from './search-state.svelte'

/**
 * What an MCP `open_search_dialog` prefill does to the state the dialog mounts on.
 *
 * The rule these pin: a prefill REPLACES the session, and `autoRun` is the only
 * thing that decides whether it runs. `runOnMount` is a one-shot flag with three
 * producers (the prefill, the dialog's reopen-with-results path, and Search's
 * index-ready listener), so a prefill that leaves a restorable session behind gets
 * its run fired twice — the second one walks nothing, because the first one's walk
 * already claimed the ground, and the dialog renders the second. That's an empty
 * dialog under "another search is going through this folder right now".
 */
describe('applySearchPrefill', () => {
  beforeEach(() => {
    searchQueryState.setLastRunQuery(null)
    searchQueryState.setRunOnMount(false)
    setResults([])
  })

  it('leaves no previous session behind, so the dialog runs the prefill once and nothing else', () => {
    // A dialog that has already searched: results on screen, and the query they came from.
    setQuery('invoices')
    setResults([
      {
        name: 'invoice.pdf',
        path: '/tmp/invoice.pdf',
        parentPath: '/tmp',
        isDirectory: false,
        size: 10,
        modifiedAt: 0,
        iconId: 'ext:pdf',
      },
    ])
    searchQueryState.setLastRunQuery('invoices')

    applySearchPrefill({ query: 'reports', autoRun: true })

    expect(searchQueryState.getRunOnMount()).toBe(true)
    expect(searchQueryState.getResults()).toEqual([])
    expect(searchQueryState.getLastRunQuery()).toBeNull()
  })

  it('honors autoRun: false, rather than the reopen path running the prefill anyway', () => {
    setQuery('invoices')
    searchQueryState.setLastRunQuery('invoices')

    applySearchPrefill({ query: 'reports', autoRun: false })

    expect(searchQueryState.getRunOnMount()).toBe(false)
    expect(searchQueryState.getLastRunQuery()).toBeNull()
  })
})
