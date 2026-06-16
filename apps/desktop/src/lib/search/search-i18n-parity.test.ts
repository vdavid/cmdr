/**
 * Base-locale (en) parity net for the search-area i18n migration.
 *
 * Every user-facing string born in `lib/search/` (the SearchDialog config, the
 * search-results toast, the searchable-folder tooltip, snapshot labels) moved from
 * hardcoded English into the `search.*` catalog, resolved through `t()` /
 * `tString()`. This is a behavior-preserving MOVE: every rendered en string must be
 * byte-identical to the pre-migration copy.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { tString } from '$lib/intl/messages.svelte'
import { SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from './capabilities'
import { resolveSearchableFolder } from './searchable-folder'
import { buildSnapshotLabel } from './snapshot-label'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('search dialog config copy parity (en)', () => {
  it('resolves the dialog title, run hint, and footer actions', () => {
    expect(tString('search.dialog.title')).toBe('Search')
    expect(tString('search.runHint')).toBe('Press Enter to search')
    expect(tString('search.action.showAll.label')).toBe('Show all in main window')
    expect(tString('search.action.showAll.tooltip')).toBe('Open the search results in the active pane')
    expect(tString('search.action.goToFile.label')).toBe('Go to file')
    expect(tString('search.action.goToFile.tooltip')).toBe('Open the file in the active pane')
  })

  it('resolves the recent-search aria label', () => {
    expect(tString('search.recent.runAria', { mode: 'Filename', query: '*.pdf' })).toBe(
      'Run recent Filename search: *.pdf',
    )
  })

  it('resolves the system-dir-exclude tooltip copy', () => {
    expect(tString('search.systemDirExclude.default')).toBe('Excludes common system and build folders')
    expect(tString('search.systemDirExclude.heading')).toBe('These folders are hidden:')
  })
})

describe('search toast and tooltip parity (en)', () => {
  it('preserves the not-a-folder toast (apostrophe hazard)', () => {
    expect(SEARCH_RESULTS_NOT_A_FOLDER_TOAST).toBe("Search results aren't a folder. Paste into a real folder instead.")
  })

  it('preserves the searchable-folder disabled tooltip', () => {
    const result = resolveSearchableFolder({ currentPath: 'search-results://sr-1', history: ['search-results://sr-1'] })
    expect(result.disabled).toBe(true)
    expect(result.disabledReason).toBe(
      "Current folder is search results, which isn't searchable. Open a real folder first.",
    )
  })
})

describe('snapshot label parity (en)', () => {
  it('falls back to the default label when there is no query', () => {
    expect(buildSnapshotLabel({ mode: 'ai', query: '', aiPrompt: null, aiLabel: null })).toBe('Search')
    expect(buildSnapshotLabel({ mode: 'filename', query: '' })).toBe('Search')
  })

  it('renders the capped-result label', () => {
    expect(tString('search.snapshot.cappedLabel', { label: '*.pdf', capText: '1,000', totalText: '5,432' })).toBe(
      '*.pdf (first 1,000 of 5,432)',
    )
  })
})
