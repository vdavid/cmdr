/**
 * Base-locale (en) parity net for the search-area i18n migration.
 *
 * Every user-facing string born in `lib/search/` (the SearchDialog config, the
 * search-results toast, the searchable-folder tooltip, snapshot labels) moved from
 * hardcoded English into the `search.*` catalog, resolved through `t()` /
 * `tString()`. This is a behavior-preserving MOVE: every rendered en string must be
 * byte-identical to the pre-migration copy.
 */

import { beforeEach, describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { searchResultsNotAFolderToast } from './capabilities'
import { resolveSearchScope } from './searchable-folder'
import { buildSnapshotLabel } from './snapshot-label'
import { withPlatform } from '#lib/platform/test-platform.js'

// These goldens pin the English a Mac shows, so they read the macOS branch of platform-variant copy.
beforeEach(() => withPlatform('macos'))

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
    expect(searchResultsNotAFolderToast()).toBe('Search results aren’t a folder. Paste into a real folder instead.')
  })

  it('preserves the unavailable-current-folder tooltip', () => {
    const result = resolveSearchScope({
      currentPath: 'search-results://sr-1',
      history: ['search-results://sr-1'],
      volumeRoots: ['/'],
    })
    expect(result.currentFolder).toBeNull()
    expect(result.currentFolderUnavailableReason).toBe(
      'Current folder is search results, which isn’t searchable. Open a real folder first.',
    )
  })
})

describe('coverage note parity (en)', () => {
  it('says a local drive has no index, names the drive, and counts what it skipped', () => {
    expect(tString('search.coverage.line.uncovered.local', { count: 1, countText: '1', drive: 'Macintosh HD' })).toBe(
      'Cmdr hasn’t indexed Macintosh HD yet, so this search skipped one folder.',
    )
  })

  it('gives a network drive its own voice, with no nudge to index it', () => {
    expect(tString('search.coverage.line.uncovered.network', { count: 2, countText: '2', drive: 'Testnas' })).toBe(
      'Cmdr doesn’t index network drives unless you ask, so this search skipped 2 folders.',
    )
  })

  it('says what the index knows about an unresolved path, never that the folder is gone', () => {
    expect(tString('search.coverage.line.unresolved', { count: 1, countText: '1' })).toBe(
      'Cmdr’s index doesn’t cover one folder yet, so this search skipped it.',
    )
    expect(tString('search.coverage.line.unresolved', { count: 1497, countText: '1,497' })).toBe(
      'Cmdr’s index doesn’t cover 1,497 folders yet, so this search skipped them.',
    )
  })

  it('says a folder nothing can open is closed, and offers nothing for it', () => {
    expect(tString('search.coverage.line.offLimits', { count: 18, countText: '18' })).toBe(
      'The system keeps 18 folders closed to Cmdr, so nothing from them is in the results.',
    )
  })

  it('ends a capped folder list with how many it left out', () => {
    expect(tString('search.coverage.list.more', { count: 3, countText: '3' })).toBe('+3 more')
  })

  it('resolves the offer, its dismissal, and the unnamed-drive stand-in', () => {
    expect(tString('search.coverage.indexDrive')).toBe('Index this drive')
    expect(tString('search.coverage.dontAskAgain')).toBe('Don’t ask again')
    expect(tString('search.coverage.unnamedDrive')).toBe('this drive')
  })

  it('resolves the three outcomes of pressing the offer', () => {
    expect(tString('search.coverage.toast.started', { drive: 'Backups' })).toBe(
      'Indexing Backups now. Searches here will be instant once it finishes.',
    )
    expect(tString('search.coverage.toast.indexingOff')).toBe(
      'Drive indexing is off in Settings, so nothing indexes. Turn it on under Indexing > Drive indexing.',
    )
    expect(tString('search.coverage.toast.notStarted', { drive: 'Backups' })).toBe(
      'Cmdr can’t index Backups right now. You can try again from the drive menu.',
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
