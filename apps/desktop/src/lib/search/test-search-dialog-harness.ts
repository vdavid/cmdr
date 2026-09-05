/**
 * Shared fixture for the mounted-`SearchDialog.svelte` behavior tests.
 *
 * Mounting the dialog means standing up its whole IPC surface (search, streaming,
 * translate, recent searches, the media index the image grid reaches), the settings it
 * mirrors, and the index/icon stores it reads. That preamble is ~200 lines, and it was
 * the same 200 lines in every file that mounts the dialog, so it lives here once.
 *
 * How a test file uses it (the factories are dynamically imported INSIDE the `vi.mock`
 * callbacks, so import order and hoisting can't bite):
 *
 * ```ts
 * import SearchDialog from './SearchDialog.svelte'
 *
 * vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
 * vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
 * vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
 * vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
 * vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())
 *
 * useSearchDialog(SearchDialog)
 * ```
 *
 * ⚠️ **The component comes IN, statically imported by the test file.** The harness can't
 * import it (see below), but a test file can: `vi.mock` is hoisted above its imports, so
 * the mocks are registered before the component loads. That placement is also what keeps
 * the dialog's module graph OFF the clock — Vite transforms it during the file's import
 * phase, which no test's timeout is charged for. A dynamic `import()` inside `mountDialog`
 * instead bills that ~12 s transform to whichever test mounts first, and under load the
 * first two or three tests of every file time out at 5 s while the rest run in ~50 ms.
 *
 * ❌ Nothing here may import a mocked module at module scope (`$lib/tauri-commands`,
 * `$lib/settings`, `$lib/indexing`, `$lib/icon-cache`, the viewer's `media-view`, or
 * anything that reaches them, `SearchDialog.svelte` and `search-state.svelte` included):
 * this module is what the mock factories load, so such an import would ask for the module
 * whose factory is still running. The mount + seed helpers below dynamically import them
 * instead.
 *
 * Sibling fixtures that deliberately DON'T use this one: `SearchDialog.coverage.svelte.test.ts`
 * and `SearchDialog.handoff.svelte.test.ts` each drive their own live-run fakes.
 */

import { vi } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import { writable } from 'svelte/store'
import type { SearchResultEntry, TranslateResult } from '$lib/ipc/bindings'

/** Type-only, so naming the component here still imports nothing at runtime. */
type SearchDialogComponent = typeof import('./SearchDialog.svelte').default

let searchDialog: SearchDialogComponent | null = null

/**
 * Hands the harness the dialog to mount. Call it once at module scope, right after the
 * `vi.mock` block, passing the statically imported component.
 */
export function useSearchDialog(component: SearchDialogComponent): void {
  searchDialog = component
}

// ─────────────────────────────────────────────────────────────────────────────
// The spies. Exported so a test can seed an answer (`mockResolvedValueOnce`) or
// count calls; each test FILE gets its own instances (one module registry per file).
// ─────────────────────────────────────────────────────────────────────────────

export const searchFilesMock = vi.fn(
  (_query?: unknown): Promise<{ entries: SearchResultEntry[]; totalCount: number }> =>
    Promise.resolve({ entries: [], totalCount: 0 }),
)

export const liveListeners = {
  progress: new Set<(event: unknown) => void>(),
  complete: new Set<(event: unknown) => void>(),
}

/**
 * The backend's side of a live run (the path Enter and the ⏎ button take).
 * It answers from `searchFilesMock`, which stays the one spy for "a search asked this
 * query", and reports a run that had nothing to walk. Emitting inside the start call
 * is faithful: the real source installs its listeners before invoking, precisely so a
 * run that finishes at once isn't missed. Sibling fake:
 * `SearchDialog.coverage.svelte.test.ts`.
 */
export const searchFilesStreamingMock = vi.fn(async (query: unknown, runId: string) => {
  const answer = await searchFilesMock(query)
  for (const listener of liveListeners.progress) {
    listener({
      runId,
      phase: 'readingIndex',
      entries: answer.entries,
      matchCount: answer.totalCount,
      dirsFound: 0,
      currentPath: null,
      capped: false,
    })
  }
  for (const listener of liveListeners.complete) {
    listener({
      runId,
      matchCount: answer.totalCount,
      coverage: {
        walk: 'nothingToWalk',
        permissionDenied: [],
        declined: [],
        stillCovering: [],
        unresolvedScopes: [],
        capped: false,
        targetVolumeId: 'root',
      },
    })
  }
  return { runId, targetVolumeId: 'root' }
})

export const translateSearchQueryMock = vi.fn(() => Promise.resolve({ display: {}, query: {} } as TranslateResult))

export const addRecentSearchMock = vi.fn(() => Promise.resolve())

export const parseSearchScopeMock = vi.fn((_scope: string) =>
  Promise.resolve({ includePaths: [] as string[], excludePatterns: [] as string[] }),
)

// The image-OCR grid's IPC. Defaults: enrichment on, one hit, so the grid actually
// queries the passed volume (its state gates all work). Path is index-relative.
export const mediaSearchOcrMock = vi.fn((_v: string, _q: string, _l: number | null) =>
  Promise.resolve([{ path: '/DCIM/photo.png', snippet: 'an [invoice] scan' }]),
)

// No CLIP model in these tests: semantic search returns nothing, so the grid runs
// OCR-only (the degraded path).
export const mediaSearchSemanticMock = vi.fn((_v: string, _q: string, _l: number | null) =>
  Promise.resolve([] as { path: string; score: number }[]),
)

export const mediaVolumeStateMock = vi.fn((_v: string) =>
  Promise.resolve({
    enabled: true,
    indexing: false,
    enrichedCount: 3,
    networkOptIn: true,
    alwaysIndexed: false,
    paused: false,
  }),
)

// ─────────────────────────────────────────────────────────────────────────────
// The settings the dialog mirrors. Mutable so a test can flip a provider or the
// auto-apply setting between mounts.
// ─────────────────────────────────────────────────────────────────────────────

export const testSettings: { aiProvider: 'off' | 'local' | 'cloud'; autoApply: boolean } = {
  aiProvider: 'off',
  autoApply: true,
}

const autoApplyListeners = new Set<(value: boolean) => void>()

/** Test helper: simulate a settings.json change for `search.autoApply` and notify subscribers. */
export function setAutoApplyForTest(value: boolean): void {
  testSettings.autoApply = value
  for (const listener of autoApplyListeners) listener(value)
}

// ─────────────────────────────────────────────────────────────────────────────
// The module factories. One per `vi.mock` in the consuming test file.
// ─────────────────────────────────────────────────────────────────────────────

export function tauriCommandsMock(): Record<string, unknown> {
  return {
    notifyDialogOpened: vi.fn(() => Promise.resolve()),
    notifyDialogClosed: vi.fn(() => Promise.resolve()),
    prepareSearchIndex: vi.fn(() => Promise.resolve({ ready: true, entryCount: 1234 })),
    searchFiles: searchFilesMock,
    searchFilesStreaming: searchFilesStreamingMock,
    cancelSearch: vi.fn(() => Promise.resolve(true)),
    onSearchProgress: vi.fn((handler: (event: unknown) => void) => {
      liveListeners.progress.add(handler)
      return Promise.resolve(() => liveListeners.progress.delete(handler))
    }),
    onSearchComplete: vi.fn((handler: (event: unknown) => void) => {
      liveListeners.complete.add(handler)
      return Promise.resolve(() => liveListeners.complete.delete(handler))
    }),
    onSearchCancelled: vi.fn(() => Promise.resolve(() => {})),
    onSearchError: vi.fn(() => Promise.resolve(() => {})),
    releaseSearchIndex: vi.fn(() => Promise.resolve()),
    translateSearchQuery: translateSearchQueryMock,
    parseSearchScope: parseSearchScopeMock,
    getSystemDirExcludes: vi.fn(() => Promise.resolve([])),
    onSearchIndexReady: vi.fn(() => Promise.resolve(() => {})),
    getRecentSearches: vi.fn(() => Promise.resolve([])),
    addRecentSearch: addRecentSearchMock,
    removeRecentSearch: vi.fn(() => Promise.resolve()),
    clearRecentSearches: vi.fn(() => Promise.resolve()),
    applyRecentSearchesMaxCount: vi.fn(() => Promise.resolve()),
    showFileContextMenu: vi.fn(() => Promise.resolve()),
    showInFinder: vi.fn(() => Promise.resolve()),
    trackEvent: vi.fn(() => Promise.resolve()),
    // The image-OCR grid (`ImageSearchResults`, rendered via `resultsExtra`) reaches these.
    mediaIndexSearchOcr: mediaSearchOcrMock,
    mediaIndexSearchSemantic: mediaSearchSemanticMock,
    mediaIndexVolumeState: mediaVolumeStateMock,
    mediaIndexThumbnailToken: vi.fn(() => Promise.resolve(null)),
    mediaIndexDropThumbnailTokens: vi.fn(() => Promise.resolve()),
  }
}

/** The viewer's `mediaUrl`; a plain string is all the grid needs to render a tile. */
export function mediaViewMock(): Record<string, unknown> {
  return { mediaUrl: (token: string) => `cmdr-media://localhost/${token}` }
}

export function settingsMock(): Record<string, unknown> {
  return {
    getSetting: vi.fn((key: string) => {
      if (key === 'ai.provider') return testSettings.aiProvider
      if (key === 'search.autoApply') return testSettings.autoApply
      // Image indexing on, so the "text in images" grid renders and fires its IPC (the
      // grid is a no-op when this is off — see `ImageSearchResults.gating.test.ts`).
      if (key === 'mediaIndex.enabled') return true
      return undefined
    }),
    onSpecificSettingChange: vi.fn((id: string, listener: (value: boolean) => void) => {
      if (id !== 'search.autoApply') return () => {}
      autoApplyListeners.add(listener)
      return () => autoApplyListeners.delete(listener)
    }),
  }
}

export function indexingMock(): Record<string, unknown> {
  return {
    isVolumeScanning: vi.fn(() => false),
    getEntriesScanned: vi.fn(() => 0),
    ROOT_VOLUME_ID: 'root',
  }
}

export function iconCacheMock(): Record<string, unknown> {
  return {
    iconCacheVersion: writable(0),
    getCachedIcon: vi.fn(() => undefined),
    getCachedCustomFolderIcon: vi.fn(() => undefined),
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mounting, and the per-test reset.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The shared `beforeEach` body: clears Search state and puts the mirrored settings back
 * to the defaults (AI off, auto-apply on). A file that wants something else passes it,
 * and mock call records stay the caller's business (they differ per file).
 */
export async function resetSearchDialogTest(
  settings: { aiProvider?: 'off' | 'local' | 'cloud'; autoApply?: boolean } = {},
): Promise<void> {
  const { clearSearchState } = await import('./search-state.svelte')
  clearSearchState()
  testSettings.aiProvider = settings.aiProvider ?? 'off'
  testSettings.autoApply = settings.autoApply ?? true
  autoApplyListeners.clear()
}

export function dispatchKey(target: Element, key: string, meta = false, shift = false): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    metaKey: meta,
    shiftKey: shift,
    bubbles: true,
    cancelable: true,
  })
  target.dispatchEvent(event)
  return event
}

export interface MountDialogOptions {
  onClose?: () => void
  onShowAllInMainWindow?: (snapshotId: string) => void
  onNavigate?: (path: string) => void
  searchVolume?: { volumeId: string; mountRoot: string; isNetwork: boolean }
  scopePresets?: { currentFolder: string | null; currentFolderUnavailableReason: string; volumeRoot: string }
}

/**
 * Tracks every mounted dialog so a per-test `afterEach` can tear down anything
 * the test forgot (or never reached) to clean up. Without this, a failing
 * assertion before `cleanup()` leaves the dialog in `document.body`, and the
 * NEXT test's input events route to the stale dialog (which then quietly
 * fires `scheduleSearch` / `executeQuery` with its old `autoApplyEnabled`,
 * triggering hard-to-diagnose cascade failures).
 */
const liveDialogs: { component: ReturnType<typeof mount>; target: HTMLDivElement }[] = []

/** The `afterEach` body every mounting test file registers. */
export function unmountAllDialogs(): void {
  while (liveDialogs.length > 0) {
    const entry = liveDialogs.pop()
    if (!entry) break
    try {
      void unmount(entry.component)
    } catch {
      /* component may already be gone if the test called cleanup() */
    }
    entry.target.remove()
  }
}

export async function mountDialog(opts: MountDialogOptions = {}): Promise<{ overlay: Element; cleanup: () => void }> {
  if (searchDialog === null) {
    throw new Error('Call useSearchDialog(SearchDialog) at module scope before mounting.')
  }
  const SearchDialog = searchDialog
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SearchDialog, {
    target,
    props: {
      onNavigate: opts.onNavigate ?? ((): void => {}),
      onClose: opts.onClose ?? ((): void => {}),
      scopePresets: opts.scopePresets ?? {
        currentFolder: '/Users/test',
        currentFolderUnavailableReason: '',
        volumeRoot: '/',
      },
      onShowAllInMainWindow: opts.onShowAllInMainWindow,
      ...(opts.searchVolume ? { searchVolume: opts.searchVolume } : {}),
    },
  })
  const entry = { component, target }
  liveDialogs.push(entry)
  await tick()
  // Let prepareSearchIndex resolve so isIndexReady flips and aiEnabled stabilizes.
  await new Promise((r) => setTimeout(r, 0))
  await tick()
  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('dialog overlay not found')
  return {
    overlay,
    cleanup: () => {
      const idx = liveDialogs.indexOf(entry)
      if (idx >= 0) liveDialogs.splice(idx, 1)
      void unmount(component)
      target.remove()
    },
  }
}

/** One stand-in result row in Search state, for the paths that act on a result. */
export async function seedResults(): Promise<void> {
  const { setResults, setTotalCount } = await import('./search-state.svelte')
  setResults([
    {
      name: 'doc.pdf',
      path: '/Users/test/docs/doc.pdf',
      parentPath: '/Users/test/docs',
      isDirectory: false,
      size: 1024,
      modifiedAt: 1_700_000_000,
      iconId: 'ext:pdf',
    },
  ])
  setTotalCount(1)
}

/**
 * The AI flow chains a few microtasks: translateSearchQuery -> applyAiFilters -> executeSearch.
 * Resolve all of them so the strip stabilizes before we assert.
 */
export async function flushAi(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0))
  await tick()
  await new Promise((r) => setTimeout(r, 0))
  await tick()
}
