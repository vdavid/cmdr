/**
 * "Show all in main window" and "Go to file": the two ways a result gets acted on.
 *
 * Pins:
 *   - The primary action mints a snapshot, stores it, pins the last-attempt ref, hands the
 *     id to the host, and closes.
 *   - Both actions persist ONE recent-search entry (the only sanctioned add points), and an
 *     AI-mode snapshot is labelled with the user's prompt, not the translated pattern.
 *   - With no results the button stays visible and disabled, not yanked from the layout.
 *
 * Shared mount + IPC fixture: `test-search-dialog-harness.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'
import { setQuery, setMode } from './search-state.svelte'
import {
  addRecentSearchMock,
  mountDialog,
  resetSearchDialogTest,
  seedResults,
  testSettings,
  unmountAllDialogs,
} from './test-search-dialog-harness'

vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())

afterEach(unmountAllDialogs)

describe('SearchDialog "Open in pane" (M8b)', () => {
  beforeEach(async () => {
    await resetSearchDialogTest()
    addRecentSearchMock.mockClear()
    // Reset the snapshot store so each test sees a fresh `sr-1` id.
    const { _resetForTesting } = await import('./snapshot-store.svelte')
    _resetForTesting()
  })

  it('calls onOpenInPane with the new snapshot id, persists to recent searches, and closes the dialog', async () => {
    let openedId: string | null = null
    let closed = false
    const { cleanup } = await mountDialog({
      onClose: () => {
        closed = true
      },
      onShowAllInMainWindow: (id) => {
        openedId = id
      },
    })
    setQuery('*.pdf')
    setMode('filename')
    await seedResults()
    await tick()

    // Find and click the "Open in pane" footer button.
    const btn = document.body.querySelector('button[aria-label="Show all in main window"]') as HTMLButtonElement
    expect(btn).not.toBeNull()
    btn.click()
    // Let the (sync) handler run and any micro-tasks resolve.
    await tick()
    await Promise.resolve()

    expect(openedId).toMatch(/^sr-\d+$/)
    expect(closed).toBe(true)
    expect(addRecentSearchMock).toHaveBeenCalledTimes(1)
    const firstCall = addRecentSearchMock.mock.calls[0] as unknown[] | undefined
    expect(firstCall).toBeDefined()
    const entry = firstCall?.[0] as { mode: string; query: string; resultCount: number }
    expect(entry.mode).toBe('filename')
    expect(entry.query).toBe('*.pdf')
    expect(entry.resultCount).toBe(1)

    cleanup()
  })

  it('persists to recent searches when the user opens a single result ("Go to file")', async () => {
    let navigatedTo: string | null = null
    const { cleanup } = await mountDialog({
      onNavigate: (path: string) => {
        navigatedTo = path
      },
    })
    setQuery('*.pdf')
    setMode('filename')
    await seedResults()
    await tick()

    // "Go to file" (the secondary footer action) opens the cursor result in the active pane.
    // The host's `onNavigate` is what closes the dialog, so we don't assert close here.
    const btn = document.body.querySelector('button[aria-label="Go to file"]') as HTMLButtonElement
    expect(btn).not.toBeNull()
    btn.click()
    await tick()
    await Promise.resolve()

    // Opening a result is a signal-rich act, so the search is remembered (mirrors "Open in pane").
    expect(addRecentSearchMock).toHaveBeenCalledTimes(1)
    const firstCall = addRecentSearchMock.mock.calls[0] as unknown[] | undefined
    const entry = firstCall?.[0] as { mode: string; query: string; resultCount: number }
    expect(entry.mode).toBe('filename')
    expect(entry.query).toBe('*.pdf')
    expect(entry.resultCount).toBe(1)
    expect(navigatedTo).toBe('/Users/test/docs/doc.pdf')

    cleanup()
  })

  it('stores the snapshot in the snapshot store under the returned id', async () => {
    let openedId: string | null = null
    const { cleanup } = await mountDialog({
      onShowAllInMainWindow: (id) => {
        openedId = id
      },
    })
    setQuery('foo')
    setMode('filename')
    await seedResults()
    await tick()

    const btn = document.body.querySelector('button[aria-label="Show all in main window"]') as HTMLButtonElement
    btn.click()
    await tick()

    const { getSnapshot, getLastAttemptId } = await import('./snapshot-store.svelte')
    expect(openedId).not.toBeNull()
    // `openedId` is mutated through the onShowAllInMainWindow callback above; TS
    // narrowing doesn't follow that, so we assert non-null after the expect.
    const snap = getSnapshot(openedId as unknown as string)
    expect(snap).toBeDefined()
    expect(snap?.mode).toBe('filename')
    expect(snap?.entries.length).toBe(1)
    // The "last attempt" slot is pinned to the new id (refcount-wise).
    expect(getLastAttemptId()).toBe(openedId as unknown as string)

    cleanup()
  })

  it('uses the original AI prompt for the snapshot label when in AI mode', async () => {
    testSettings.aiProvider = 'cloud'
    let openedId: string | null = null
    const { cleanup } = await mountDialog({
      onShowAllInMainWindow: (id) => {
        openedId = id
      },
    })
    const { setLastAiPrompt } = await import('./search-state.svelte')
    setMode('ai')
    setQuery('*.pdf') // AI translation overwrote the natural-language query
    setLastAiPrompt('find my pdf invoices')
    await seedResults()
    await tick()

    const btn = document.body.querySelector('button[aria-label="Show all in main window"]') as HTMLButtonElement
    btn.click()
    await tick()

    const { getSnapshot } = await import('./snapshot-store.svelte')
    expect(openedId).not.toBeNull()
    const snap = getSnapshot(openedId as unknown as string)
    expect(snap?.label).toBe('find my pdf invoices')

    cleanup()
  })

  it('renders the Show all button disabled and does nothing when there are no results', async () => {
    let opened = false
    const { cleanup } = await mountDialog({
      onShowAllInMainWindow: () => {
        opened = true
      },
    })
    // No results seeded.
    await tick()
    // The button stays VISIBLE when resultCount === 0, just rendered disabled. Yanking
    // it would jump the layout while the user is mid-thought.
    const btn = document.body.querySelector<HTMLButtonElement>('button[aria-label="Show all in main window"]')
    expect(btn).not.toBeNull()
    expect(btn?.disabled).toBe(true)
    btn?.click()
    expect(opened).toBe(false)
    cleanup()
  })
})
