/**
 * The scope ladder: one volume is the ceiling, and an empty box is the current folder.
 *
 * Pins:
 *   - An empty scope box sends the focused pane's folder (or its volume when the pane has
 *     no real folder behind it), resolved per run rather than frozen at open time.
 *   - A scope the user typed wins.
 *   - A DEFAULTED scope is never persisted into recent searches; a chosen one is.
 *
 * Shared mount + IPC fixture: `test-search-dialog-harness.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'
import { setQuery, setMode, setScope } from './search-state.svelte'
import {
  addRecentSearchMock,
  dispatchKey,
  mountDialog,
  parseSearchScopeMock,
  resetSearchDialogTest,
  searchFilesMock,
  seedResults,
  unmountAllDialogs,
} from './test-search-dialog-harness'

vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())

afterEach(unmountAllDialogs)

describe('SearchDialog scope ladder (one volume is the ceiling)', () => {
  beforeEach(async () => {
    // Run only on explicit Enter, so each run is countable.
    await resetSearchDialogTest({ autoApply: false })
    searchFilesMock.mockClear()
    addRecentSearchMock.mockClear()
    parseSearchScopeMock.mockClear()
  })

  /** Types a query and runs it, returning the `SearchQuery` that reached the backend. */
  async function runAndCaptureQuery(overlay: Element): Promise<{ includePaths?: string[] | null }> {
    setQuery('*.pdf')
    dispatchKey(overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(searchFilesMock).toHaveBeenCalledTimes(1)
    const call = searchFilesMock.mock.calls[0] as unknown[]
    return call[0] as { includePaths?: string[] | null }
  }

  it('an empty scope box searches the current folder, not everywhere', async () => {
    // The behavior change the one-volume ceiling makes: "no scope" used to fan out across every indexed
    // volume. It now means the one folder the user is standing in.
    const { overlay, cleanup } = await mountDialog()
    const query = await runAndCaptureQuery(overlay)
    expect(query.includePaths).toEqual(['/Users/test'])
    cleanup()
  })

  it('falls back to the volume when the focused pane has no real folder', async () => {
    const { overlay, cleanup } = await mountDialog({
      scopePresets: { currentFolder: null, currentFolderUnavailableReason: 'snapshot', volumeRoot: '/Volumes/naspi' },
    })
    const query = await runAndCaptureQuery(overlay)
    expect(query.includePaths).toEqual(['/Volumes/naspi'])
    cleanup()
  })

  it('a scope the user typed wins over the default', async () => {
    parseSearchScopeMock.mockResolvedValueOnce({ includePaths: ['/Users/test/docs'], excludePatterns: [] })
    const { overlay, cleanup } = await mountDialog()
    setScope('~/docs')
    const query = await runAndCaptureQuery(overlay)
    expect(query.includePaths).toEqual(['/Users/test/docs'])
    cleanup()
  })

  it('does NOT persist a defaulted scope into recent searches', async () => {
    // The default follows whichever pane you're in, so writing its resolved path into
    // saved history would bake a machine-specific folder nobody chose into every entry,
    // and split one search into a separate recent per folder visited.
    const { cleanup } = await mountDialog()
    setQuery('*.pdf')
    setMode('filename')
    await seedResults()
    await tick()

    const btn = document.body.querySelector('button[aria-label="Go to file"]') as HTMLButtonElement
    btn.click()
    await tick()
    await Promise.resolve()

    const entry = (addRecentSearchMock.mock.calls[0] as unknown[])[0] as { scope: string }
    expect(entry.scope).toBe('')
    cleanup()
  })

  it('persists a scope the user actually chose', async () => {
    const { cleanup } = await mountDialog()
    setQuery('*.pdf')
    setMode('filename')
    setScope('~/docs')
    await seedResults()
    await tick()

    const btn = document.body.querySelector('button[aria-label="Go to file"]') as HTMLButtonElement
    btn.click()
    await tick()
    await Promise.resolve()

    const entry = (addRecentSearchMock.mock.calls[0] as unknown[])[0] as { scope: string }
    expect(entry.scope).toBe('~/docs')
    cleanup()
  })
})
