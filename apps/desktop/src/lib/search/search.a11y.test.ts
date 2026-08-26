/**
 * Tier 3 a11y tests for the Search surfaces: the coverage note, the image-search
 * grid, the dialog itself, and the walk-handoff toast.
 *
 * One file per component would cost about four times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, fixtures, props,
 * and assertions.
 *
 * `$lib/settings` is the one genuine disagreement: the image grid answers on
 * `mediaIndex.enabled`, the dialog on `ai.provider` and `search.autoApply`. It's a
 * mutable stub each of those two blocks installs in its own `beforeEach`, and
 * `null` means "use the real export" — which is what the coverage-note and
 * walk-handoff blocks, which never stubbed settings, always saw.
 *
 * The image grid's fake timers stay INSIDE its own block: file-wide they'd stall
 * the other blocks' async renders.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import { writable } from 'svelte/store'
import CoverageNote from './CoverageNote.svelte'
import SearchDialog from './SearchDialog.svelte'
import WalkHandoffToastContent from './WalkHandoffToastContent.svelte'
import { _resetWalkHandoffForTesting } from './walk-handoff.svelte'
import { setWalkHandoff } from './walk-handoff-state.svelte'
import type { CoverageNote as Note } from './coverage-note'
import type { MediaIndexVolumeState, OcrHit, SimilarImage } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const searchOcr =
  vi.fn<(payload: { volumeId: string; query: string; limit: number | null }) => Promise<OcrHit[]>>()
const searchSemantic =
  vi.fn<
    (payload: { volumeId: string; query: string; limit: number | null }) => Promise<{ path: string; score: number }[]>
  >()
const volumeState = vi.fn<(volumeId: string) => Promise<MediaIndexVolumeState>>()
const thumbnailToken = vi.fn<(path: string) => Promise<string | null>>()
const dropTokens = vi.fn<(tokens: string[]) => Promise<void>>()
const findSimilar =
  vi.fn<(payload: { volumeId: string; sourcePath: string; limit: number | null }) => Promise<SimilarImage[]>>()

// What `getSetting` answers. `null` means "use the real export", which is what the
// blocks that never stubbed settings saw. The two blocks that did install theirs in
// their own `beforeEach`.
let settingsStub: ((key: string) => unknown) | null = null

// The union of the IPC the image grid and the dialog reach for; the two sets are
// disjoint, so neither block's stub changes what the other sees. The real module is
// spread first so a call outside the union behaves as it does un-merged.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  mediaIndexSearchOcr: (volumeId: string, query: string, limit: number | null) =>
    searchOcr({ volumeId, query, limit }),
  mediaIndexSearchSemantic: (volumeId: string, query: string, limit: number | null) =>
    searchSemantic({ volumeId, query, limit }),
  mediaIndexVolumeState: (v: string) => volumeState(v),
  mediaIndexThumbnailToken: (p: string) => thumbnailToken(p),
  mediaIndexDropThumbnailTokens: (t: string[]) => dropTokens(t),
  mediaIndexFindSimilar: (volumeId: string, sourcePath: string, limit: number | null) =>
    findSimilar({ volumeId, sourcePath, limit }),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  prepareSearchIndex: vi.fn(() => Promise.resolve({ ready: true, entryCount: 1234 })),
  searchFiles: vi.fn(() => Promise.resolve({ entries: [], totalCount: 0 })),
  releaseSearchIndex: vi.fn(() => Promise.resolve()),
  translateSearchQuery: vi.fn(() => Promise.resolve({ display: {}, query: {} })),
  parseSearchScope: vi.fn(() => Promise.resolve({ includePaths: [], excludePatterns: [] })),
  getSystemDirExcludes: vi.fn(() => Promise.resolve(['node_modules', 'target', '.git'])),
  onSearchIndexReady: vi.fn(() => Promise.resolve(() => {})),
}))

// The viewer's `mediaUrl`; a plain string is all the grid needs for render + axe.
vi.mock('../../routes/viewer/media-view', () => ({
  mediaUrl: (token: string) => `cmdr-media://localhost/${token}`,
}))

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<{
    getSetting: (key: string) => unknown
    onSpecificSettingChange: (...args: unknown[]) => () => void
  }>()
  return {
    ...actual,
    getSetting: vi.fn((key: string): unknown => (settingsStub ? settingsStub(key) : actual.getSetting(key))),
    // The two stubbing blocks don't toggle settings, so a no-op unsubscribe is
    // enough for them; the others keep the real subscription.
    onSpecificSettingChange: vi.fn((...args: unknown[]): (() => void) =>
      settingsStub ? () => {} : actual.onSpecificSettingChange(...args),
    ),
  }
})

vi.mock('$lib/indexing', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isVolumeScanning: vi.fn(() => false),
  getEntriesScanned: vi.fn(() => 0),
  ROOT_VOLUME_ID: 'root',
}))

vi.mock('$lib/icon-cache', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  iconCacheVersion: writable(0),
}))

// Imported AFTER the mocks so the component picks them up.
const { default: ImageSearchResults } = await import('./ImageSearchResults.svelte')

// These components share one jsdom document, the dialog portals into
// `document.body`, and axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `CoverageNote.svelte`: the strip that says why a search came
 * back empty (message, the skipped scope paths, and the per-drive offer) must have no
 * axe violations, in both the offered and the silenced shape.
 */
describe('CoverageNote a11y', () => {
  const UNCOVERED: Note = {
    uncoveredScopes: ['/Volumes/naspi/photos'],
    unresolvedScopes: [],
    volumeId: 'smb-naspi',
  }

  function mountNote(note: Note | null, onIndexDrive: (() => void) | null) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CoverageNote, {
      target,
      props: {
        note,
        driveName: 'Naspolya',
        isNetwork: true,
        isIndexing: false,
        onIndexDrive,
        onSilenceDrive: () => {},
      },
    })
    flushSync()
    return target
  }

  it('the uncovered note with its offer has no violations', async () => {
    const target = mountNote(UNCOVERED, () => {})
    expect(target.querySelector('.coverage-note button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the note without an offer (a silenced drive) has no violations', async () => {
    const target = mountNote(UNCOVERED, null)
    expect(target.querySelector('.coverage-note button')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('an unresolved-path note has no violations', async () => {
    const target = mountNote(
      { uncoveredScopes: [], unresolvedScopes: ['/Users/test/gone', '/Users/test/also-gone'], volumeId: 'root' },
      null,
    )
    await expectNoA11yViolations(target)
  })

  it('stays mounted with nothing to say, so the live region survives to announce the next run', async () => {
    const target = mountNote(null, null)
    const strip = target.querySelector('.coverage-note')
    expect(strip).not.toBeNull()
    expect(strip?.getAttribute('role')).toBe('status')
    expect(strip?.textContent.trim()).toBe('')
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ImageSearchResults.svelte` (the "text in images" OCR grid).
 *
 * Covers the coverage-honesty notices (indexing off, still indexing, not indexed yet, a
 * genuine no-match) and the populated thumbnail grid with highlighted snippets. The IPC
 * commands are mocked so the component drives each state deterministically; timers are
 * faked to fire the debounced fetch.
 */
describe('ImageSearchResults a11y', () => {
  // The master "Index image contents" toggle. These a11y cases exercise the ENABLED
  // states (the section renders), so keep it on; `beforeEach` resets it.
  let masterEnabled = true

  function state(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
    return {
      enabled: true,
      indexing: false,
      enrichedCount: 5,
      qualifyingCount: null,
      networkOptIn: false,
      alwaysIndexed: false,
      paused: false,
      waitingForImportance: false,
      coveredQualifyingCount: null,
      keptCount: null,
      ...overrides,
    }
  }

  async function mountAndSettle(props: Record<string, unknown> = {}): Promise<HTMLElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ImageSearchResults, {
      target,
      props: { query: 'invoice', volumeId: 'root', active: true, onOpen: () => {}, ...props },
    })
    flushSync()
    // Fire the 300 ms debounce and let the awaited IPC mocks resolve.
    await vi.advanceTimersByTimeAsync(400)
    await tick()
    // axe.run relies on real timers internally; leaving fake timers on hangs it.
    vi.useRealTimers()
    return target
  }

  beforeEach(() => {
    masterEnabled = true
    settingsStub = (key: string): unknown => (key === 'mediaIndex.enabled' ? masterEnabled : undefined)
    vi.useFakeTimers()
    searchOcr.mockResolvedValue([])
    searchSemantic.mockResolvedValue([])
    volumeState.mockResolvedValue(state())
    thumbnailToken.mockResolvedValue('tok123')
    dropTokens.mockResolvedValue()
    findSimilar.mockResolvedValue([])
  })

  afterEach(() => {
    settingsStub = null
    vi.useRealTimers()
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('the "still indexing" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ indexing: true }))
    const target = await mountAndSettle()
    expect(target.querySelector('.ir-notice-indexing')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the "not indexed yet" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ enrichedCount: 0 }))
    const target = await mountAndSettle()
    expect(target.querySelector('.ir-notice')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('a genuine no-match has no a11y violations', async () => {
    const target = await mountAndSettle()
    expect(target.querySelector('.ir-empty')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the network "not opted in" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ networkOptIn: false }))
    const target = await mountAndSettle({ isNetwork: true })
    expect(target.querySelector('.ir-notice')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the network "disconnected / paused" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ networkOptIn: true, paused: true }))
    const target = await mountAndSettle({ isNetwork: true, mountRoot: '/Volumes/naspi' })
    expect(target.querySelector('.ir-notice')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the populated grid with highlighted snippets has no a11y violations', async () => {
    searchOcr.mockResolvedValue([
      { path: '/photos/receipt.png', snippet: 'total [invoice] amount' },
      { path: '/photos/scan.jpg', snippet: 'an [invoice] copy' },
    ] satisfies OcrHit[])
    const target = await mountAndSettle()
    expect(target.querySelectorAll('.ir-tile').length).toBe(2)
    expect(target.querySelector('.ir-snippet mark')?.textContent).toBe('invoice')
    await expectNoA11yViolations(target)
  })

  it('find-similar re-queries the grid, then back returns to the text results', async () => {
    searchOcr.mockResolvedValue([{ path: '/photos/receipt.png', snippet: 'total [invoice] amount' }] satisfies OcrHit[])
    findSimilar.mockResolvedValue([
      { path: '/photos/similar-a.jpg', score: 0.98 },
      { path: '/photos/similar-b.jpg', score: 0.91 },
    ] satisfies SimilarImage[])
    const target = await mountAndSettle()
    expect(target.querySelectorAll('.ir-tile').length).toBe(1)

    // Enter "similar" mode from the tile's find-similar button.
    ;(target.querySelector('.ir-similar-btn') as HTMLButtonElement).click()
    await vi.waitFor(() => {
      expect(target.querySelector('.ir-title-similar')).not.toBeNull()
    })
    // The command keys on the STORED (index-relative == absolute for local) path, capped at 48.
    expect(findSimilar).toHaveBeenCalledWith({ volumeId: 'root', sourcePath: '/photos/receipt.png', limit: 48 })
    expect(target.querySelectorAll('.ir-tile').length).toBe(2)
    await expectNoA11yViolations(target)

    // Back exits similar mode and restores the OCR results for the current query.
    ;(target.querySelector('.ir-back') as HTMLButtonElement).click()
    await vi.waitFor(() => {
      expect(target.querySelector('.ir-title-similar')).toBeNull()
    })
    expect(target.querySelectorAll('.ir-tile').length).toBe(1)
  })
})

/**
 * Tier 3 a11y tests for `SearchDialog.svelte`.
 *
 * The dialog pulls in several Tauri commands (prepareSearchIndex,
 * searchFiles, translateSearchQuery, etc.) and reactive settings. We
 * mock the IPC + settings boundary and then run axe against the three
 * macro-states that matter structurally:
 *   - AI disabled, index not ready (loading state)
 *   - AI disabled, index ready (default search UI)
 *   - AI enabled, index ready (AI prompt row + search UI)
 */
describe('SearchDialog a11y', () => {
  let aiProvider: 'off' | 'local' | 'cloud' = 'off'

  beforeEach(() => {
    aiProvider = 'off'
    settingsStub = (key: string): unknown => {
      if (key === 'ai.provider') return aiProvider
      if (key === 'search.autoApply') return true
      return undefined
    }
  })

  afterEach(() => {
    settingsStub = null
  })

  it('default state (AI off, index loading) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      },
    })
    await tick()
    // Don't await the IPC chain: we're auditing the first paint.
    await expectNoA11yViolations(target)
  })

  it('after index ready (AI off) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      },
    })
    // Flush microtasks so prepareSearchIndex resolves and isIndexReady flips.
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('AI enabled (cloud provider) has no violations', async () => {
    aiProvider = 'cloud'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchDialog, {
      target,
      props: {
        onNavigate: () => {},
        onClose: () => {},
        scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      },
    })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('WalkHandoffToastContent a11y', () => {
  afterEach(() => {
    _resetWalkHandoffForTesting()
  })

  it('renders with no a11y violations while a walk is running', async () => {
    // The component is prop-free by design (a toast replaced in place keeps its
    // original props), so the module state IS the fixture here.
    setWalkHandoff({
      runId: 'run-1',
      snapshotId: 'sr-1',
      label: '*.pdf',
      view: {
        phase: 'walking',
        matchCount: 1234,
        dirsFound: 5678,
        currentPath: '/Volumes/Backups/photos/2019',
        capped: false,
        running: true,
        incomplete: false,
      },
    })
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(WalkHandoffToastContent, { target })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders nothing, and nothing broken, with no walk to speak about', async () => {
    setWalkHandoff(null)
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(WalkHandoffToastContent, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
