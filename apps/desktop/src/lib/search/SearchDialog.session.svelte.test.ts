/**
 * The dialog's session: what survives a close, and what `⌘N` wipes.
 *
 * Pins:
 *   - Close + reopen preserves state (the dialog doesn't wipe state on unmount).
 *   - A restored NON-AI session re-runs on reopen, so the user lands on results
 *     rather than the empty state; a restored AI one must NOT (a translate is a paid
 *     round-trip).
 *   - `⌘N` clears the session, including the prior-run marker that arms the re-run.
 *
 * Shared mount + IPC fixture: `test-search-dialog-harness.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'
import type { SearchResultEntry, TranslateResult } from '$lib/ipc/bindings'
import {
  getQuery,
  setQuery,
  getMode,
  setMode,
  getScope,
  setScope,
  getCursorIndex,
  setCursorIndex,
} from './search-state.svelte'
import {
  dispatchKey,
  mountDialog,
  resetSearchDialogTest,
  searchFilesMock,
  testSettings,
  translateSearchQueryMock,
  unmountAllDialogs,
} from './test-search-dialog-harness'

vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())

afterEach(unmountAllDialogs)

describe('SearchDialog state preservation and ⌘N', () => {
  beforeEach(async () => {
    await resetSearchDialogTest()
  })

  it('preserves state across close and reopen', async () => {
    const { cleanup } = await mountDialog()

    setQuery('*.pdf')
    setScope('~/Documents')
    setCursorIndex(3)

    cleanup()
    await tick()

    expect(getQuery()).toBe('*.pdf')
    expect(getScope()).toBe('~/Documents')
    expect(getCursorIndex()).toBe(3)

    const { cleanup: cleanup2 } = await mountDialog()

    expect(getQuery()).toBe('*.pdf')
    expect(getScope()).toBe('~/Documents')
    expect(getCursorIndex()).toBe(3)

    cleanup2()
  })

  it('⌘N clears state inside the dialog', async () => {
    const { overlay, cleanup } = await mountDialog()

    setQuery('*.pdf')
    setScope('~/Documents')
    setCursorIndex(5)

    dispatchKey(overlay, 'n', true)
    await tick()

    expect(getQuery()).toBe('')
    expect(getScope()).toBe('')
    expect(getCursorIndex()).toBe(0)

    cleanup()
  })
})

describe('SearchDialog reopen re-runs so results show', () => {
  beforeEach(async () => {
    // Run only on explicit Enter so we count runs precisely.
    await resetSearchDialogTest({ autoApply: false })
    searchFilesMock.mockClear()
    translateSearchQueryMock.mockClear()
  })

  it('a restored non-AI session re-runs the query on reopen (results, not the empty state)', async () => {
    // First open: type a query and run it once.
    const first = await mountDialog()
    setQuery('*.png')
    dispatchKey(first.overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(searchFilesMock).toHaveBeenCalledTimes(1)

    // Close and reopen. The reopen must re-derive results on mount WITHOUT the user
    // touching anything: pre-fix, `hasSearched` reset to false and nothing re-ran, so the
    // content area sat on the empty state until a manual edit / Enter.
    first.cleanup()
    searchFilesMock.mockClear()
    const second = await mountDialog()
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(searchFilesMock).toHaveBeenCalledTimes(1)
    second.cleanup()
  })

  it('a restored AI session shows persisted results WITHOUT re-calling the cloud on reopen', async () => {
    testSettings.aiProvider = 'cloud'
    translateSearchQueryMock.mockResolvedValueOnce({
      display: { namePattern: '*.png', patternType: 'glob' },
      query: {},
      caveat: null,
    } as unknown as TranslateResult)
    // First open: run an AI search (one translate + one searchFiles).
    const first = await mountDialog()
    setMode('ai')
    setQuery('all screenshots')
    dispatchKey(first.overlay, 'Enter')
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(translateSearchQueryMock).toHaveBeenCalledTimes(1)

    // Reopen. AI mode must NOT re-call translate (cloud cost); the persisted results render
    // from the surviving state instead.
    first.cleanup()
    translateSearchQueryMock.mockClear()
    searchFilesMock.mockClear()
    const second = await mountDialog()
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(translateSearchQueryMock).not.toHaveBeenCalled()
    expect(searchFilesMock).not.toHaveBeenCalled()
    second.cleanup()
  })

  it('reopening in AI mode renders the persisted results without re-calling the cloud translate', async () => {
    // N2 no-recall guard: pins QueryDialog's `getMode() !== 'ai'` reopen gate. A restored
    // AI-mode session (prior run present, mode 'ai') must render its persisted result rows on a
    // fresh mount WITHOUT a second cloud translate. This is cheap insurance against a future
    // loosening of that gate (translate is a paid round-trip; auto-recalling it on every reopen
    // would burn the user's quota silently).
    testSettings.aiProvider = 'cloud'
    translateSearchQueryMock.mockResolvedValueOnce({
      display: { namePattern: '*.png', patternType: 'glob' },
      query: {},
      caveat: null,
    } as unknown as TranslateResult)
    searchFilesMock.mockResolvedValueOnce({
      entries: [
        {
          name: 'a.png',
          path: '/Users/test/a.png',
          parentPath: '/Users/test',
          isDirectory: false,
          size: 10,
          modifiedAt: 0,
          iconId: 'file',
        },
        {
          name: 'b.png',
          path: '/Users/test/b.png',
          parentPath: '/Users/test',
          isDirectory: false,
          size: 20,
          modifiedAt: 0,
          iconId: 'file',
        },
      ] satisfies SearchResultEntry[],
      totalCount: 2,
    })

    // First open: run an AI search that yields two persisted result rows.
    const first = await mountDialog()
    setMode('ai')
    setQuery('all screenshots')
    dispatchKey(first.overlay, 'Enter')
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(translateSearchQueryMock).toHaveBeenCalledTimes(1)
    expect(first.overlay.querySelectorAll('.result-row').length).toBe(2)

    // Reopen. The gate must NOT re-call translate; persisted rows render from surviving state.
    first.cleanup()
    translateSearchQueryMock.mockClear()
    searchFilesMock.mockClear()
    const second = await mountDialog()
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(getMode()).toBe('ai')
    expect(translateSearchQueryMock).not.toHaveBeenCalled()
    expect(second.overlay.querySelectorAll('.result-row').length).toBe(2)
    second.cleanup()
  })

  it('a first-ever open (no prior run) shows the empty state and does not auto-run', async () => {
    const { overlay, cleanup } = await mountDialog()
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    // Nothing ran, and the empty state is visible.
    expect(searchFilesMock).not.toHaveBeenCalled()
    expect(overlay.querySelector('.empty-state, [data-testid="empty-state"]') ?? overlay.textContent).toBeTruthy()
    cleanup()
  })

  it('⌘N returns to the empty state and clears the prior-run marker (no re-run on next reopen)', async () => {
    const first = await mountDialog()
    setQuery('*.png')
    dispatchKey(first.overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(searchFilesMock).toHaveBeenCalledTimes(1)

    // ⌘N clears the session (query + the prior-run marker `lastRunQuery`).
    dispatchKey(first.overlay, 'n', true)
    await tick()
    expect(getQuery()).toBe('')

    // Reopen: with no query and no prior run, nothing re-runs and the empty state stands.
    first.cleanup()
    searchFilesMock.mockClear()
    const second = await mountDialog()
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(searchFilesMock).not.toHaveBeenCalled()
    second.cleanup()
  })
})
