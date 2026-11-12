/**
 * "Open in pane" while the search is still walking, from the dialog's side.
 *
 * One regression anchor carries this file, and it cost an afternoon in the running app
 * before it was found: **closing the dialog must spare the run it just handed to a
 * pane.** Dialog close cancels every live run, so if the close doesn't name the
 * handed-off run, the walk stops the instant the pane appears — the pane fills with
 * whatever had arrived by then, the toast says "still searching" over a walk that
 * isn't, and nothing anywhere reports a problem. Exactly the confident-wrong-answer
 * shape this whole effort exists to remove.
 *
 * `walk-handoff.svelte.test.ts` covers what happens AFTER the handoff (the toast state
 * machine, the snapshot appends, resuming). This is the wiring into it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import { writable } from 'svelte/store'
import SearchDialog from './SearchDialog.svelte'
import type { SearchResultEntry } from '#lib/ipc/bindings.js'
import { clearSearchState, setQuery } from './search-state.svelte'
import { _resetWalkHandoffForTesting } from './walk-handoff.svelte'
import { _resetForTesting as resetSnapshots, getSnapshot } from './snapshot-store.svelte'
import { clearAllToasts, getToasts } from '#lib/ui/toast/toast-store.svelte.js'
import { footerButton } from './test-search-dialog-harness'

const {
  searchFilesMock,
  searchFilesStreamingMock,
  releaseSearchIndexMock,
  cancelSearchMock,
  liveListeners,
  testSettings,
} = vi.hoisted(() => ({
  searchFilesMock: vi.fn((_query: { limit?: number | null }) =>
    Promise.resolve({ entries: [] as unknown[], totalCount: 0, targetVolumeId: 'root' }),
  ),
  searchFilesStreamingMock: vi.fn(() => Promise.resolve({ runId: 'ignored', targetVolumeId: 'root' })),
  releaseSearchIndexMock: vi.fn(() => Promise.resolve()),
  cancelSearchMock: vi.fn(() => Promise.resolve(true)),
  liveListeners: { progress: new Set<(event: unknown) => void>() },
  testSettings: { autoApply: false },
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  prepareSearchIndex: vi.fn(() => Promise.resolve({ ready: true, entryCount: 10, loading: false })),
  searchFiles: searchFilesMock,
  searchFilesStreaming: searchFilesStreamingMock,
  cancelSearch: cancelSearchMock,
  isSearchRunLive: vi.fn(() => Promise.resolve(true)),
  onSearchProgress: vi.fn((handler: (event: unknown) => void) => {
    liveListeners.progress.add(handler)
    return Promise.resolve(() => liveListeners.progress.delete(handler))
  }),
  onSearchComplete: vi.fn(() => Promise.resolve(() => {})),
  onSearchCancelled: vi.fn(() => Promise.resolve(() => {})),
  onSearchError: vi.fn(() => Promise.resolve(() => {})),
  releaseSearchIndex: releaseSearchIndexMock,
  translateSearchQuery: vi.fn(() => Promise.resolve({ display: {}, query: {} })),
  parseSearchScope: vi.fn(() => Promise.resolve({ includePaths: [], excludePatterns: [] })),
  getSystemDirExcludes: vi.fn(() => Promise.resolve([])),
  onSearchIndexReady: vi.fn(() => Promise.resolve(() => {})),
  getRecentSearches: vi.fn(() => Promise.resolve([])),
  addRecentSearch: vi.fn(() => Promise.resolve()),
  removeRecentSearch: vi.fn(() => Promise.resolve()),
  clearRecentSearches: vi.fn(() => Promise.resolve()),
  applyRecentSearchesMaxCount: vi.fn(() => Promise.resolve()),
  showFileContextMenu: vi.fn(() => Promise.resolve()),
  showInFinder: vi.fn(() => Promise.resolve()),
  trackEvent: vi.fn(() => Promise.resolve()),
  enableDriveIndex: vi.fn(() => Promise.resolve({ status: 'ok', data: { status: 'started' } })),
  mediaIndexSearchOcr: vi.fn(() => Promise.resolve([])),
  mediaIndexSearchSemantic: vi.fn(() => Promise.resolve([])),
  mediaIndexVolumeState: vi.fn(() => Promise.resolve({ enabled: false })),
  mediaIndexThumbnailToken: vi.fn(() => Promise.resolve(null)),
  mediaIndexDropThumbnailTokens: vi.fn(() => Promise.resolve()),
}))

vi.mock('../../routes/viewer/media-view', () => ({
  mediaUrl: (token: string) => `cmdr-media://localhost/${token}`,
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'ai.provider') return 'off'
    if (key === 'search.autoApply') return testSettings.autoApply
    if (key === 'mediaIndex.enabled') return false
    if (key === 'mediaIndex.showInSearch') return false
    if (key === 'indexing.silencedDrives') return '[]'
    return undefined
  }),
  setSetting: vi.fn(),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: vi.fn(() => []) }))
vi.mock('#lib/indexing/index.js', () => ({
  isVolumeScanning: vi.fn(() => false),
  getEntriesScanned: vi.fn(() => 0),
  ROOT_VOLUME_ID: 'root',
}))
vi.mock('#lib/icon-cache.js', () => ({
  iconCacheVersion: writable(0),
  getCachedIcon: vi.fn(() => undefined),
  getCachedCustomFolderIcon: () => undefined,
}))

function entry(name: string): SearchResultEntry {
  return {
    name,
    path: `/w/${name}`,
    parentPath: '/w',
    isDirectory: false,
    size: 1,
    modifiedAt: 1,
    iconId: 'ext:txt',
  }
}

/** Let the mount round-trips (prepare, history load, listener install) resolve. */
async function settle(): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await tick()
    await new Promise((r) => setTimeout(r, 0))
  }
}

const live: { component: ReturnType<typeof mount>; target: HTMLElement }[] = []

beforeEach(() => {
  clearSearchState()
  resetSnapshots()
  _resetWalkHandoffForTesting()
  releaseSearchIndexMock.mockClear()
  cancelSearchMock.mockClear()
  searchFilesMock.mockClear()
  searchFilesStreamingMock.mockClear()
  liveListeners.progress.clear()
  testSettings.autoApply = false
  clearAllToasts()
})

afterEach(() => {
  while (live.length > 0) {
    const held = live.pop()
    if (!held) break
    try {
      void unmount(held.component)
    } catch {
      /* already gone */
    }
    held.target.remove()
  }
})

/** Mounts the dialog and returns its overlay plus a close-and-unmount helper. */
async function mountDialog(
  props: { onShowAllInMainWindow?: (snapshotId: string) => void } = {},
): Promise<{ overlay: Element; closeDialog: () => Promise<void> }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SearchDialog, {
    target,
    props: {
      onNavigate: () => {},
      onClose: () => {},
      scopePresets: { currentFolder: '/w', currentFolderUnavailableReason: '', volumeRoot: '/' },
      ...props,
    },
  })
  live.push({ component, target })
  await settle()
  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('dialog overlay not found')
  return {
    overlay,
    closeDialog: async () => {
      await unmount(component)
      target.remove()
      live.length = 0
      await settle()
    },
  }
}

/**
 * The run id the runner minted for the first (and only) live run of a test.
 *
 * `searchFilesStreaming` is the seam it crosses, so the mock's call record is where the
 * frontend's own id is observable at all.
 */
function startedRunId(): string {
  const call = searchFilesStreamingMock.mock.calls[0] as unknown[] | undefined
  const runId = call?.[1]
  if (typeof runId !== 'string') throw new Error('no live run was started')
  return runId
}

/** Runs a live search that keeps going: one batch of rows, no terminal event. */
async function runWalkingSearch(overlay: Element): Promise<void> {
  setQuery('report')
  overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
  await settle()
  const runId = startedRunId()
  for (const handler of liveListeners.progress) {
    handler({
      runId,
      phase: 'walking',
      entries: [entry('report.pdf')],
      matchCount: 1,
      dirsFound: 12,
      currentPath: '/w/deep',
      capped: false,
    })
  }
  await settle()
}

describe('handing a running walk to a pane', () => {
  it('closes without stopping the run the pane is now being fed by', async () => {
    const { overlay, closeDialog } = await mountDialog()
    await runWalkingSearch(overlay)
    const runId = startedRunId()

    const openInPane = footerButton('Show all in main window')
    expect(openInPane?.disabled).toBe(false)
    openInPane?.click()
    await settle()
    await closeDialog()

    // The whole milestone in one assertion: the close names the run it must not stop.
    // `null` here means the walk dies the moment the pane appears, and the only sign is
    // a pane that stops filling.
    //
    // ❌ `toHaveBeenCalledWith` alone is NOT enough, and that gap hid the bug once: a
    // SECOND release carrying `null` cancels the run just as dead, and the assertion
    // still passes because the first call matched. Count the calls.
    expect(releaseSearchIndexMock.mock.calls).toEqual([[runId]])
    expect(cancelSearchMock).not.toHaveBeenCalled()
  })

  it('stops every run when the dialog closes with nothing handed over', async () => {
    const { overlay, closeDialog } = await mountDialog()
    await runWalkingSearch(overlay)
    await closeDialog()

    // The default, and the reason the case above needs saying at all: a walk nobody is
    // waiting on is work the app promises not to do.
    expect(releaseSearchIndexMock).toHaveBeenCalledWith(null)
  })
})

/** `count` distinct rows, so a pane's size says which answer filled it. */
function rows(count: number): SearchResultEntry[] {
  return Array.from({ length: count }, (_, i) => entry(`report-${String(i)}.pdf`))
}

describe('an index answer that replaces a live run', () => {
  it('opens the pane with every hit and hands off no walk the dialog stopped following', async () => {
    // The index knows 1,439 matches: a dialog-sized ask gets 30 rows, a pane-sized one all.
    searchFilesMock.mockImplementation((query) =>
      Promise.resolve({
        entries: rows((query.limit ?? 0) > 30 ? 1439 : 30),
        totalCount: 1439,
        targetVolumeId: 'root',
      }),
    )
    testSettings.autoApply = true
    const opened: { snapshotId: string | null } = { snapshotId: null }
    const { overlay, closeDialog } = await mountDialog({
      onShowAllInMainWindow: (id) => {
        opened.snapshotId = id
      },
    })

    // Enter starts a live run that hasn't said anything yet...
    setQuery('report')
    overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await settle()
    expect(searchFilesStreamingMock).toHaveBeenCalledTimes(1)

    // ...and a keystroke's auto-apply answers from the index instead. The dialog stops
    // following the live run here: its events, terminal one included, never reach it.
    vi.useFakeTimers()
    try {
      const input = overlay.querySelector<HTMLInputElement>('.query-bar input.text-field-control')
      if (!input) throw new Error('query input not found')
      input.value = 'reports'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      vi.advanceTimersByTime(1000)
    } finally {
      vi.useRealTimers()
    }
    await settle()
    expect(overlay.querySelectorAll('.result-row').length).toBeGreaterThan(0)

    footerButton('Show all in main window')?.click()
    await settle()
    await closeDialog()

    // The regression (#1067): the stale live run was handed off, so the pane opened with the
    // dialog's 30 and a "Still searching" toast waited forever on a run nobody followed.
    expect(opened.snapshotId).not.toBeNull()
    expect(getSnapshot(opened.snapshotId ?? '')?.entries.length).toBe(1439)
    expect(getToasts()).toEqual([])
    expect(releaseSearchIndexMock.mock.calls).toEqual([[null]])
  })
})
