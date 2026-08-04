/**
 * Coverage honesty and the per-target readiness gate.
 *
 * Two regression anchors carry this milestone:
 *
 *   1. **A search runs on a machine with no root index.** The old gate waited for
 *      root's arena before calling `runQuery`, and on a machine that declined
 *      indexing no `search-index-ready` was ever coming, so the dialog sat inert:
 *      typing and pressing Enter did nothing at all, silently.
 *   2. **The coverage note belongs to the run that produced it.** A note left
 *      standing over a later, fully-covered answer is a lie with a fresh result set
 *      under it.
 *
 * Everything else here pins the distinct copy per typed field, and the per-drive
 * offer's gates.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import { writable } from 'svelte/store'
import SearchDialog from './SearchDialog.svelte'
import type { SearchResult, SearchResultEntry } from '$lib/ipc/bindings'
import { tString } from '$lib/intl/messages.svelte'
import { clearSearchState, setQuery } from './search-state.svelte'

const NAS: SearchResultEntry[] = []

const {
  prepareSearchIndexMock,
  searchFilesMock,
  readyListeners,
  enableDriveIndexMock,
  silencedDrives,
  setSettingMock,
  volumesMock,
  addToastMock,
} = vi.hoisted(() => ({
  prepareSearchIndexMock: vi.fn(() => Promise.resolve({ ready: true, entryCount: 1234, loading: false })),
  searchFilesMock: vi.fn(
    (): Promise<{
      entries: SearchResultEntry[]
      totalCount: number
      uncoveredScopes?: string[]
      unresolvedScopes?: string[]
      targetVolumeId?: string
    }> => Promise.resolve({ entries: [], totalCount: 0 }),
  ),
  readyListeners: new Set<(volumeId: string, entryCount: number) => void>(),
  enableDriveIndexMock: vi.fn(() => Promise.resolve({ status: 'ok', data: { status: 'started' } })),
  silencedDrives: { value: '[]' },
  setSettingMock: vi.fn(),
  volumesMock: vi.fn(() => [] as { id: string; name: string; path: string; category: string }[]),
  addToastMock: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  prepareSearchIndex: prepareSearchIndexMock,
  searchFiles: searchFilesMock,
  releaseSearchIndex: vi.fn(() => Promise.resolve()),
  translateSearchQuery: vi.fn(() => Promise.resolve({ display: {}, query: {} })),
  parseSearchScope: vi.fn(() => Promise.resolve({ includePaths: [], excludePatterns: [] })),
  getSystemDirExcludes: vi.fn(() => Promise.resolve([])),
  onSearchIndexReady: vi.fn((handler: (volumeId: string, entryCount: number) => void) => {
    readyListeners.add(handler)
    return Promise.resolve(() => readyListeners.delete(handler))
  }),
  getRecentSearches: vi.fn(() => Promise.resolve([])),
  addRecentSearch: vi.fn(() => Promise.resolve()),
  removeRecentSearch: vi.fn(() => Promise.resolve()),
  clearRecentSearches: vi.fn(() => Promise.resolve()),
  applyRecentSearchesMaxCount: vi.fn(() => Promise.resolve()),
  showFileContextMenu: vi.fn(() => Promise.resolve()),
  showInFinder: vi.fn(() => Promise.resolve()),
  trackEvent: vi.fn(() => Promise.resolve()),
  enableDriveIndex: enableDriveIndexMock,
  // The image-OCR grid is off in these tests (`mediaIndex.enabled` false), so it fires nothing.
  mediaIndexSearchOcr: vi.fn(() => Promise.resolve([])),
  mediaIndexSearchSemantic: vi.fn(() => Promise.resolve([])),
  mediaIndexVolumeState: vi.fn(() => Promise.resolve({ enabled: false })),
  mediaIndexThumbnailToken: vi.fn(() => Promise.resolve(null)),
  mediaIndexDropThumbnailTokens: vi.fn(() => Promise.resolve()),
}))

vi.mock('../../routes/viewer/media-view', () => ({
  mediaUrl: (token: string) => `cmdr-media://localhost/${token}`,
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'ai.provider') return 'off'
    // Auto-apply off: every run in this file is an explicit Enter, so run counts are exact.
    if (key === 'search.autoApply') return false
    if (key === 'mediaIndex.enabled') return false
    if (key === 'indexing.silencedDrives') return silencedDrives.value
    return undefined
  }),
  setSetting: setSettingMock,
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: volumesMock,
}))

vi.mock('$lib/ui/toast', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return { ...actual, addToast: addToastMock }
})

vi.mock('$lib/indexing', () => ({
  isVolumeScanning: vi.fn(() => false),
  getEntriesScanned: vi.fn(() => 0),
  ROOT_VOLUME_ID: 'root',
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
  getCachedIcon: vi.fn(() => undefined),
}))

const live: { component: ReturnType<typeof mount>; target: HTMLDivElement }[] = []

afterEach(() => {
  while (live.length > 0) {
    const entry = live.pop()
    if (!entry) break
    try {
      void unmount(entry.component)
    } catch {
      /* already gone */
    }
    entry.target.remove()
  }
  readyListeners.clear()
})

interface MountOptions {
  searchVolume?: { volumeId: string; mountRoot: string; isNetwork: boolean }
}

async function mountDialog(opts: MountOptions = {}): Promise<{ overlay: Element; target: HTMLElement }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SearchDialog, {
    target,
    props: {
      onNavigate: () => {},
      onClose: () => {},
      scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      ...(opts.searchVolume ? { searchVolume: opts.searchVolume } : {}),
    },
  })
  live.push({ component, target })
  await settle()
  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('dialog overlay not found')
  return { overlay, target }
}

/** Let the mount round-trips (prepare, history load, run) resolve. */
async function settle(): Promise<void> {
  await tick()
  await new Promise((r) => setTimeout(r, 0))
  await tick()
  await new Promise((r) => setTimeout(r, 0))
  await tick()
}

async function runSearch(overlay: Element): Promise<void> {
  overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
  await settle()
}

/** The rendered coverage strip's text, with the whitespace the markup adds collapsed. */
function noteText(target: HTMLElement): string {
  return (target.querySelector('.coverage-note')?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function result(overrides: Partial<SearchResult>): SearchResult {
  return { entries: NAS, totalCount: 0, ...overrides }
}

beforeEach(() => {
  clearSearchState()
  searchFilesMock.mockReset()
  searchFilesMock.mockResolvedValue({ entries: [], totalCount: 0 })
  prepareSearchIndexMock.mockReset()
  prepareSearchIndexMock.mockResolvedValue({ ready: true, entryCount: 1234, loading: false })
  enableDriveIndexMock.mockReset()
  enableDriveIndexMock.mockResolvedValue({ status: 'ok', data: { status: 'started' } })
  addToastMock.mockReset()
  setSettingMock.mockReset()
  volumesMock.mockReturnValue([])
  silencedDrives.value = '[]'
})

describe('the readiness gate is per target, not "is root loaded"', () => {
  it('runs the search on a machine with no root index at all', async () => {
    // The population this whole plan exists for: indexing was declined, so root has no
    // arena to load and no `search-index-ready` will ever fire. The dialog must still
    // ASK — the answer comes back with its coverage gap named, which is honest, where
    // waiting forever was silently inert.
    prepareSearchIndexMock.mockResolvedValue({ ready: false, entryCount: 0, loading: false })
    const { overlay } = await mountDialog()

    setQuery('*.pdf')
    await runSearch(overlay)

    expect(searchFilesMock).toHaveBeenCalledTimes(1)
  })

  it('waits while the target volume’s arena is genuinely on its way, then runs', async () => {
    // The other half of the same gate: when the backend promises an event is coming,
    // waiting is right — it saves a blocking IPC per keystroke.
    prepareSearchIndexMock.mockResolvedValue({ ready: false, entryCount: 0, loading: true })
    const { overlay } = await mountDialog()

    setQuery('*.pdf')
    await runSearch(overlay)
    expect(searchFilesMock).not.toHaveBeenCalled()

    for (const listener of readyListeners) listener('root', 42)
    await settle()
    expect(searchFilesMock).toHaveBeenCalledTimes(1)
  })

  it('does not wait for root when the search targets another volume', async () => {
    // Root's pre-load says nothing about a NAS. Waiting for it would make a search of
    // the drive the user is standing on hostage to a volume they didn't ask about.
    prepareSearchIndexMock.mockResolvedValue({ ready: false, entryCount: 0, loading: true })
    const { overlay } = await mountDialog({
      searchVolume: { volumeId: 'smb-naspi', mountRoot: '/Volumes/naspi', isNetwork: true },
    })

    setQuery('*.pdf')
    await runSearch(overlay)

    expect(searchFilesMock).toHaveBeenCalledTimes(1)
  })

  it('keeps searching after ⌘N, which resets the query and not what the backend reported', async () => {
    // ⌘N used to wipe the readiness flag along with the query state, and no second
    // `search-index-ready` was ever coming, so every later search silently did nothing.
    const { overlay } = await mountDialog()
    overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', metaKey: true, bubbles: true, cancelable: true }))
    await settle()

    setQuery('*.pdf')
    await runSearch(overlay)

    expect(searchFilesMock).toHaveBeenCalledTimes(1)
  })
})

describe('the coverage note answers "why is this empty?"', () => {
  it('clears on the next run', async () => {
    // The regression anchor: a note is about the run that produced it. Left standing, it
    // explains away a fresh, fully-covered answer that needs no explaining.
    searchFilesMock.mockResolvedValueOnce(
      result({ uncoveredScopes: ['/Volumes/naspi/photos'], targetVolumeId: 'smb-naspi' }),
    )
    const { overlay, target } = await mountDialog()

    setQuery('*.pdf')
    await runSearch(overlay)
    expect(noteText(target)).toContain('/Volumes/naspi/photos')

    searchFilesMock.mockResolvedValueOnce(result({ entries: [], totalCount: 0 }))
    setQuery('*.png')
    await runSearch(overlay)
    expect(noteText(target)).toBe('')
  })

  it('renders distinct copy for an uncovered volume and an unresolved path', async () => {
    // Two typed fields, two different truths: the drive has no index at all, versus the
    // drive is indexed but this folder isn't in it. Branching on emptiness, never on text.
    volumesMock.mockReturnValue([{ id: 'smb-naspi', name: 'Naspolya', path: '/Volumes/naspi', category: 'network' }])
    searchFilesMock.mockResolvedValueOnce(
      result({ uncoveredScopes: ['/Volumes/naspi/photos'], targetVolumeId: 'smb-naspi' }),
    )
    const { overlay, target } = await mountDialog()

    setQuery('*.pdf')
    await runSearch(overlay)
    const uncovered = noteText(target)
    expect(uncovered).toContain(tString('search.coverage.uncovered.network', { drive: 'Naspolya' }))

    searchFilesMock.mockResolvedValueOnce(result({ unresolvedScopes: ['/Users/test/gone'], targetVolumeId: 'root' }))
    setQuery('*.png')
    await runSearch(overlay)
    const unresolved = noteText(target)
    expect(unresolved).toContain(tString('search.coverage.unresolved', { count: 1 }))
    expect(unresolved).not.toBe(uncovered)
  })

  it('says a local drive isn’t indexed in the local voice', async () => {
    volumesMock.mockReturnValue([{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume' }])
    searchFilesMock.mockResolvedValueOnce(result({ uncoveredScopes: ['/Users/test'], targetVolumeId: 'root' }))
    const { overlay, target } = await mountDialog()

    setQuery('*.pdf')
    await runSearch(overlay)

    expect(noteText(target)).toContain(tString('search.coverage.uncovered.local', { drive: 'Macintosh HD' }))
  })
})

describe('the per-drive indexing offer', () => {
  const NAS_VOLUME = { id: 'smb-naspi', name: 'Naspolya', path: '/Volumes/naspi', category: 'network' }

  function offerButton(target: HTMLElement): HTMLButtonElement | null {
    const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('.coverage-note button'))
    return buttons.find((b) => b.textContent.trim() === tString('search.coverage.indexDrive')) ?? null
  }

  async function runUncovered(): Promise<{ overlay: Element; target: HTMLElement }> {
    searchFilesMock.mockResolvedValue(
      result({ uncoveredScopes: ['/Volumes/naspi/photos'], targetVolumeId: 'smb-naspi' }),
    )
    const mounted = await mountDialog()
    setQuery('*.pdf')
    await runSearch(mounted.overlay)
    return mounted
  }

  it('turns on indexing for the drive the backend actually searched', async () => {
    volumesMock.mockReturnValue([NAS_VOLUME])
    const { target } = await runUncovered()

    offerButton(target)?.click()
    await settle()

    expect(enableDriveIndexMock).toHaveBeenCalledWith('smb-naspi')
  })

  it('stays quiet for a drive the user silenced, while still telling the truth', async () => {
    // The silence is exactly the "stop offering me this" the first-connect prompt writes.
    // The gap is still real, so the note stays; only the offer goes.
    volumesMock.mockReturnValue([NAS_VOLUME])
    silencedDrives.value = JSON.stringify(['smb-naspi'])
    const { target } = await runUncovered()

    expect(offerButton(target)).toBeNull()
    expect(noteText(target)).toContain('/Volumes/naspi/photos')
  })

  it('sticks: "Don’t ask again" persists the per-drive silence', async () => {
    volumesMock.mockReturnValue([NAS_VOLUME])
    const { target } = await runUncovered()

    const dismiss = Array.from(target.querySelectorAll<HTMLButtonElement>('.coverage-note button')).find(
      (b) => b.textContent.trim() === tString('search.coverage.dontAskAgain'),
    )
    dismiss?.click()
    await settle()

    expect(setSettingMock).toHaveBeenCalledWith('indexing.silencedDrives', JSON.stringify(['smb-naspi']))
  })

  it('offers nothing for an unresolved path, whose drive is already indexed', async () => {
    volumesMock.mockReturnValue([{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume' }])
    searchFilesMock.mockResolvedValue(result({ unresolvedScopes: ['/Users/test/gone'], targetVolumeId: 'root' }))
    const { overlay, target } = await mountDialog()

    setQuery('*.pdf')
    await runSearch(overlay)

    expect(offerButton(target)).toBeNull()
  })
})
